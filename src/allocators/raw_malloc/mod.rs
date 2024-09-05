//! A singlethreaded memory allocator.
//!
// For a general view of the allocator's operational semantics see the [`crate`] level documentation.
//
// # Additional implementation notes
// ## Object requirements and layout augmentation
// Many of the private functions assume that their parameters adhere to
// the allocator's "object requirements" -
// To faciliate allocation the `RawMalloc` divides the heap into blocks,
// all of which have [`HEADER_ALIGN`] alignment and content size that is a multiple of [`HEADER_SIZE`].
// To comform to these requirements the allocator adjusts parameters passed to alloc/realloc.
// The is called layout augmentation and is achieved via the
// [`util::augment_layout`] and [`util::augment_size`] functions.
//
// [`HEADER_ALIGN`]: HEADER_ALIGN
// [`HEADER_SIZE`]: HEADER_SIZE

use self::util::{augment_layout, augment_size, find_place, to_nonnull_slice};
use crate::freelist::{Freelist, Node, NODE_ALIGN, NODE_SIZE};
use crate::growers::Grower;
use crate::header::{Header, HEADER_ALIGN, HEADER_SIZE};
use crate::util::{checked_add, raw_ptr};

use core::alloc::{Allocator, GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use std::alloc::AllocError;
use std::fmt::Debug;
use std::intrinsics::copy_nonoverlapping;

use static_assertions::const_assert;
use tracing::{debug, error, instrument, Level};

mod util;

pub(crate) const BLOCK_CONTENT_MIN_SIZE: usize = NODE_SIZE;
pub(crate) const BLOCK_CONTENT_MIN_ALIGN: usize = NODE_ALIGN;

pub(crate) const BLOCK_MIN_SIZE: usize = HEADER_SIZE + BLOCK_CONTENT_MIN_SIZE;

// Header-tagging requires block content to be at least 2-byte-aligned.
const_assert!(BLOCK_CONTENT_MIN_ALIGN >= 2);
const_assert!(NODE_ALIGN <= HEADER_ALIGN);

/// A single threaded memory allocator.
#[repr(C)]
pub struct RawMalloc<T: Grower> {
    freelist: UnsafeCell<Freelist>,
    grower: UnsafeCell<T>,
}

impl<T: Grower> Debug for RawMalloc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawMalloc")
            .field("grower", &self.grower)
            .finish()
    }
}

impl<T: Grower> RawMalloc<T> {
    /// Creates an allocator instance with the specified grower.
    ///
    /// # Safety
    /// Callers must make sure that the provided grower will be the only object
    /// managing it's underlying buffer for the lifetime of the returned allocator.
    pub const unsafe fn with_grower(grower: T) -> Self {
        RawMalloc {
            freelist: UnsafeCell::new(Freelist::new()),
            grower: UnsafeCell::new(grower),
        }
    }
}

impl<T: Grower> RawMalloc<T> {
    #[instrument(level = "info", ret(level = Level::INFO), err(Debug, level = Level::ERROR))]
    unsafe fn __alloc(&self, layout: Layout) -> Result<NonNull<u8>, ()> {
        let augmented_layout = augment_layout(layout)?;
        debug!(?augmented_layout, "Layout augmented.");

        let obj_size = augmented_layout.size();
        let obj_align = augmented_layout.align();

        let obj_start = match unsafe { self.place_in_first_free_block(obj_size, obj_align) } {
            Ok(p) => {
                debug!(obj_start = ?p.as_ptr(), "Found free block to accomodate object.");
                p
            }
            Err(()) => {
                debug!("Couldn't find free block to accomodate object, requesting heap growth.");
                unsafe { self.grow_and_place(obj_size, obj_align)? }
            }
        };

        Ok(obj_start)
    }

    #[instrument(level = "info", ret(level = Level::INFO), err(Debug, level = Level::ERROR))]
    unsafe fn __realloc(
        &self,
        obj_start: *mut u8,
        layout: Layout,
        new_obj_size: usize,
    ) -> Result<NonNull<u8>, ()> {
        let new_obj_size = augment_size(new_obj_size)?;
        debug!(augmented_size = ?new_obj_size, "Augmented new_obj_size.");

        let block_start = obj_start.sub(HEADER_SIZE);
        let block_header: *mut Header = block_start.cast();
        debug_assert!(
            !(*block_header).is_tagged(),
            "Objects should be preceded by untagged headers."
        );
        let obj_size = (*block_header).__content_size;

        if self.try_adjust(block_start, new_obj_size).is_ok() {
            return Ok(NonNull::new_unchecked(obj_start));
        }
        debug_assert!(new_obj_size > layout.size());
        debug!("Couldn't adjust current block, attempting reallocation to a new block.");

        let new_obj_start = self
            .__alloc(Layout::from_size_align_unchecked(
                new_obj_size,
                layout.align(),
            ))?
            .as_ptr();
        copy_nonoverlapping(obj_start, new_obj_start, obj_size);
        self.dealloc(obj_start, layout);
        Ok(NonNull::new_unchecked(new_obj_start))
    }

    /// Tries to adjust (that is shrink or expand) an occupied block for an object with size `new_obj_size`.
    ///
    /// # Notes
    /// This adjustment might consume subsequent free blocks and/or create new padding blocks.
    /// Shrinks are guaranteed to be carried out successfully whereas grows might fail if there
    /// is not enough space in which case `Err(())` is returned.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that `block_start` is pointing to a valid occupied block
    /// and `new_obj_size` is properly augmented for an allocation (see the [`module`](self) level
    /// documentation). Additionally callers must ensure that no allocator fields are currently borrowed.
    #[instrument(level = "debug", ret(level = Level::DEBUG), err(Debug, level = Level::ERROR))]
    unsafe fn try_adjust(&self, block_start: *mut u8, new_obj_size: usize) -> Result<(), ()> {
        debug_assert!(self.heap_end().is_some());
        let heap_end = raw_ptr(self.heap_end());
        let block_header: *mut Header = block_start.cast();
        let obj_start = block_start.add(HEADER_SIZE);

        debug_assert!(
            !(*block_header).is_tagged(),
            "Objects should be preceded by untagged headers."
        );

        let new_block_end = checked_add(obj_start, new_obj_size).ok_or(())?;

        loop {
            let block_end = obj_start.add((*block_header).__content_size);

            if block_end as *const u8 >= new_block_end {
                // Shrink
                debug_assert_eq!(obj_start as usize - block_start as usize, HEADER_SIZE);
                self.place_raw(block_start, block_end, obj_start, new_obj_size);
                return Ok(());
            }

            if block_end == heap_end {
                break;
            }

            let next_block_start = block_end;
            let next_block_header: &Header = &*next_block_start.cast();

            if !next_block_header.is_tagged() {
                break;
            }

            let next_block_node: *mut Node = next_block_start.add(HEADER_SIZE).cast();
            (*self.freelist.get()).remove(next_block_node);
            (*block_header).__content_size += HEADER_SIZE + next_block_header.content_size();
            debug!(
                ?next_block_start,
                ?next_block_header,
                adjusted_block_header = ?block_header,
                "Merging with successive free block."
            );
        }

        Err(())
    }

    /// Grows the heap for an allocation of size `obj_size` and alignment `obj_align`.
    /// Returns the old heap end, the growth ammount and a pointer to where to put the allocation
    /// or `Err(())` if the heap can not grow to accomodate the object.
    /// A space for a preceding header is always accounted for and if necessary a space for
    /// a padding free block is also considered.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that `obj_align` and `obj_size`
    /// conform to the allocator object requirements (See the [`module`](self) level documentation).
    /// Additionally callers must ensure that the allocator's grower is not currently borrowed.
    #[instrument(level = "debug", ret(level = Level::DEBUG), err(Debug, level=Level::ERROR))]
    unsafe fn grow(
        &self,
        obj_size: usize,
        obj_align: usize,
    ) -> Result<(NonNull<u8>, usize, NonNull<u8>), ()> {
        debug_assert_eq!(obj_size % HEADER_SIZE, 0);

        let old_heap_end: *mut u8;
        let obj_start: *mut u8;
        let obj_end: *mut u8;

        {
            match self.heap_end() {
                Some(end) => old_heap_end = end.as_ptr(),
                None => {
                    error!("Growth failiure, couldn't get heap end.");
                    return Err(());
                }
            };
            debug_assert_eq!(old_heap_end as usize % HEADER_ALIGN, 0);

            match find_place(old_heap_end.cast(), obj_align) {
                Some(p) => obj_start = p.as_ptr(),
                None => {
                    error!("Growth failiure, object alignment is too big.");
                    return Err(());
                }
            }
            debug_assert_eq!(obj_start as usize % HEADER_ALIGN, 0);

            match checked_add(obj_start, obj_size) {
                Some(p) => obj_end = p as *mut u8,
                None => {
                    error!("Growth failure, object is too big.");
                    return Err(());
                }
            }
        }

        let growth_amount = obj_end as usize - old_heap_end as usize;
        debug!(growth_amount, "Calculated growth ammount.");

        match (*self.grower.get()).grow(growth_amount) {
            Err(()) => {
                error!("Growth failiure, no memory.");
                Err(())
            }
            Ok((__old_heap_end, growth_amount)) => {
                debug_assert_eq!(old_heap_end, __old_heap_end.as_ptr());
                Ok((
                    __old_heap_end,
                    growth_amount,
                    NonNull::new_unchecked(obj_start),
                ))
            }
        }
    }

    /// Grows the heap for an allocation of size `obj_size` and alignment `obj_align` and
    /// divides the newly allocated space into blocks one of which delegated to the allocation.
    /// Returns a pointer to the new allocation or `Err(())` if the growth failed
    /// (see [`grow`](RawMalloc::grow) for details on when this happens).
    ///
    /// Safety:
    /// This function is unsafe since it assumes that `obj_align` and `obj_size`
    /// conform to the allocator object requirements (See the [`module`](self) level documentation).
    /// Additionally callers must ensure that no allocator field is currently borrowed.
    #[instrument(level = "debug", ret(level = Level::DEBUG), err(Debug, level=Level::ERROR))]
    unsafe fn grow_and_place(&self, obj_size: usize, obj_align: usize) -> Result<NonNull<u8>, ()> {
        let (old_heap_end, growth_amount, obj_start) = self
            .grow(obj_size, obj_align)
            .map(|p| (p.0.as_ptr(), p.1, p.2.as_ptr()))
            .inspect_err(|_| error!("Couldn't grow heap"))?;

        debug!(?obj_start, "Heap growth successful.");
        self.place_raw(
            old_heap_end,
            old_heap_end.add(growth_amount),
            obj_start,
            obj_size,
        );
        Ok(NonNull::new_unchecked(obj_start))
    }

    /// Tries to place an object into the block pointed to by `block_start`,
    /// creating additional free blocks if padding is necessary.
    /// On success a pointer to the newly allocated object is returned.
    ///
    /// Safety:
    /// This function is unsafe since it assumes
    /// that `block_start` is pointing to the header of a valid *free* block,
    /// and that `obj_align` and `obj_size` conform to the allocator object requirements
    /// (See the [`module`](self) level documentation). Additionally callers must ensure
    /// that the allocator's freelist is not currently borrowed.
    #[instrument(level = "debug", ret(level = Level::DEBUG), err(Debug, level=Level::DEBUG))]
    unsafe fn try_place(
        &self,
        block_start: *mut u8,
        obj_size: usize,
        obj_align: usize,
    ) -> Result<NonNull<u8>, ()> {
        let block_header: *mut Header = block_start.cast();
        debug_assert!((*block_header).is_tagged(), "Block should be free.");

        let block_content_size = (*block_header).content_size();
        let block_end = block_start.add(HEADER_SIZE + block_content_size);

        let obj_start: *mut u8;

        {
            match find_place(block_start, obj_align) {
                Some(p) => obj_start = p.as_ptr(),
                None => {
                    debug!("Couldn't place object, alignment is too big.");
                    return Err(());
                }
            }
            debug!(?obj_start);

            match checked_add(obj_start, obj_size) {
                Some(p) if p <= block_end as *const u8 => {}
                _ => {
                    debug!("Couldn't place object, size is too large.");
                    return Err(());
                }
            }
        }

        let block_freenode = block_start.add(HEADER_SIZE).cast();
        (*self.freelist.get()).remove(block_freenode);
        self.place_raw(block_start, block_end, obj_start, obj_size);
        Ok(NonNull::new_unchecked(obj_start))
    }

    /// Places an object with `obj_size` at `obj_start` creting a new block for it.
    /// If necessary additional padding blocks are created so that the `[block_start, block_end]`
    /// range gets populated with contiguous blocks.
    ///
    /// # Notes
    /// This function does not operate on the memory where the block contents are to be placed.
    /// This is useful since it allows for shrinking an object by placing it over itself
    /// with but with a smaller size.
    ///
    /// # Safety:
    /// This function is unsafe since it relies on the assumption that the `[block_start, block_end]` range
    /// does not contain any data that's in currently in use and that it's with proper size and
    /// alignment for populating it with blocks. It's also assumed that the object parameters are valid -
    /// that is the object should not only fit and be properly aligned in the region
    /// but any left padding should also be sufficiently large to hold a
    /// [`BLOCK_MIN_SIZE`]-sized block. Lastly callers should ensure
    /// that the allocator's freelist is not currently borrowed.
    #[instrument(level = "debug")]
    unsafe fn place_raw(
        &self,
        mut block_start: *mut u8,
        block_end: *mut u8,
        obj_start: *mut u8,
        mut obj_size: usize,
    ) {
        debug_assert!(obj_size as isize > 0);
        debug_assert!(block_end as usize >= block_start as usize);
        debug_assert!(
            block_end as usize - block_start as usize >= BLOCK_MIN_SIZE,
            "{:?} {:?}",
            block_start,
            block_end
        );
        debug_assert!(block_start <= obj_start);
        debug_assert!(
            obj_start as usize <= usize::MAX - obj_size,
            "Object end shouldn't be outside of the address space."
        );

        let mut obj_end = obj_start.add(obj_size);

        debug_assert!(block_end >= obj_end);

        let dist = obj_start as usize - block_start as usize;

        if dist != HEADER_SIZE {
            debug_assert!(
                dist >= HEADER_SIZE + BLOCK_MIN_SIZE,
                "Left padding should be at least BLOCK_MIN_SIZE"
            );

            let padding_start = block_start;
            let padding_content_size = dist - 2 * HEADER_SIZE;

            debug!("Placing a free block as left padding.");
            self.create_new_block(padding_start, padding_content_size, true);

            block_start = block_start.add(HEADER_SIZE + padding_content_size).cast();
        }

        let dist = block_end as usize - obj_end as usize;

        if dist >= BLOCK_MIN_SIZE {
            let padding_start = obj_end;
            let padding_content_size = dist - HEADER_SIZE;

            debug!("Placing a free block as right padding.");
            self.create_new_block(padding_start, padding_content_size, true);
        } else {
            obj_end = block_end;
            obj_size = obj_end as usize - obj_start as usize;

            debug!(
                new_obj_size = obj_size,
                "No space for right padding, adjusting object size."
            );
        }

        debug!("Placing block to accomodate object.");
        self.create_new_block(block_start, obj_size, false);
    }

    /// Frees the block pointed to by `block_start`, currently this is equivalent to
    /// tagging the block header and adding the block to the freelist.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that `block_start` points to a block
    /// that is indeed to be freed, i.e. the block shouldn't be free already and should be treated
    /// as free after this function returns. Additionally callers must ensure the allocator's freelist isn't
    /// currently borrowed.
    #[instrument(level = "debug")]
    unsafe fn free_block(&self, block_start: *mut u8) {
        let block_header: *mut Header = block_start.cast();
        let old_header: &Header = &*block_header;

        debug_assert!(!old_header.is_tagged(), "Block shouldn't be free already.");

        let new_header = old_header.tagged();
        *block_header = new_header;

        (*self.freelist.get()).push_front(block_header.add(1).cast());
    }

    /// Creates a new block at the location pointed to by `block_start`.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that the block parameters
    /// are valid and the block doesn't overwrite any block that is currently in use.
    /// Additionally if `is_free` is true callers must ensure the allocator's freelist isn't
    /// currently borrowed.
    #[instrument(level = "debug")]
    unsafe fn create_new_block(&self, block_start: *mut u8, content_size: usize, is_free: bool) {
        let block_header: *mut Header = block_start.cast();
        *block_header = Header::new_unchecked(content_size, is_free);
        if is_free {
            (*self.freelist.get()).push_front(block_header.add(1).cast());
        }
        debug!(?block_start, block_header = ?*block_header, "Created a new block.");
    }

    /// Merges subsequent (in memory) freelist nodes starting from `node` into a single node.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that `node` is a part of a valid free block
    /// and that no allocator field is currently borrowed.
    #[instrument(level = "debug")]
    unsafe fn merge_subsequent_nodes(&self, node: *mut Node) {
        let block_header = &mut *(node.cast::<Header>().sub(1));
        debug_assert!(
            block_header.is_tagged(),
            "Nodes shuld be preceeded by tagged headers."
        );

        debug!(?block_header);

        let heap_end = self
            .heap_end()
            .expect("Couldn't get heap end.")
            .as_ptr()
            .cast();
        loop {
            let block_content_size = block_header.content_size();
            let next_block_start = node.cast::<u8>().add(block_content_size);

            if next_block_start == heap_end {
                debug!("Reached heap end, no more blocks to merge with, stopping.");
                break;
            }

            let next_block_header: &Header = &*next_block_start.cast();

            if !next_block_header.is_tagged() {
                debug!(
                    ?next_block_start,
                    ?next_block_header,
                    "Successive block isn't free, stopping merge."
                );
                break;
            }

            let next_block_size = next_block_header.content_size();
            let next_block_node: *mut Node = next_block_start.add(HEADER_SIZE).cast();

            (*self.freelist.get()).remove(next_block_node);
            block_header.__content_size += HEADER_SIZE + next_block_size;

            debug!(
                ?next_block_start,
                ?next_block_header,
                adjusted_block_header = ?block_header,
                "Merging with successive free block."
            );
        }
    }

    /// Places the object with the provided parameters into the first free block
    /// that can accomodate the object. Returns a pointer to that object or `Err(())`
    /// if there was no suitable block for the object.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that the object layout is augmented
    /// and that no allocator field is currently borrowed.
    #[instrument(level = "debug", ret(level = Level::DEBUG), err(Debug, level = Level::DEBUG))]
    unsafe fn place_in_first_free_block(
        &self,
        obj_size: usize,
        obj_align: usize,
    ) -> Result<NonNull<u8>, ()> {
        let mut p: *mut Node = raw_ptr((*self.freelist.get()).head());

        while !p.is_null() {
            self.merge_subsequent_nodes(p);

            let free_block_start = p.cast::<u8>().sub(HEADER_SIZE);
            let free_block_header: &Header = &*free_block_start.cast();
            let free_block_content_size = free_block_header.content_size();
            debug!(
                ?free_block_start,
                ?free_block_content_size,
                "Found free block."
            );

            if let Ok(obj_start) = self.try_place(free_block_start, obj_size, obj_align) {
                return Ok(obj_start);
            }

            debug!("Couldn't place object in free block. Continuing...");
            p = (*p).next;
        }

        Err(())
    }

    /// Returns the current end of the heap.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that there are no live references to the
    /// allocator's inner grower.
    #[inline(always)]
    unsafe fn heap_end(&self) -> Option<NonNull<u8>> {
        match (*self.grower.get()).grow(0) {
            Ok((end, _)) => Some(end),
            Err(()) => None,
        }
    }
}

//---------------impl Allocator for RawMalloc---------------//

unsafe impl<T: Grower> Allocator for RawMalloc<T> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe {
            let Ok(obj_start) = self.__alloc(layout) else {
                return Err(AllocError);
            };
            Ok(to_nonnull_slice(obj_start))
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        GlobalAlloc::dealloc(self, ptr.as_ptr(), layout)
    }

    unsafe fn grow(
        &self,
        obj_start: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(old_layout.size() <= new_layout.size());
        debug_assert_eq!(old_layout.align(), new_layout.align());
        let Ok(obj_start) = self.__realloc(obj_start.as_ptr(), old_layout, new_layout.size())
        else {
            return Err(AllocError);
        };
        Ok(to_nonnull_slice(obj_start))
    }

    unsafe fn shrink(
        &self,
        obj_start: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(old_layout.size() >= new_layout.size());
        debug_assert_eq!(old_layout.align(), new_layout.align());

        let Ok(obj_start) = self.__realloc(obj_start.as_ptr(), old_layout, new_layout.size())
        else {
            return Err(AllocError);
        };

        Ok(to_nonnull_slice(obj_start))
    }
}

//---------------impl GlobalAlloc for RawMalloc---------------//

unsafe impl<T: Grower> GlobalAlloc for RawMalloc<T> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        raw_ptr(self.__alloc(layout).ok())
    }

    #[instrument(level = "info")]
    unsafe fn dealloc(&self, obj_start: *mut u8, layout: Layout) {
        let block_start = obj_start.sub(HEADER_SIZE);
        let block_header: &Header = &*block_start.cast();

        debug_assert_eq!(
            obj_start as usize % HEADER_ALIGN,
            0,
            "All allocations should have header alignment."
        );

        debug_assert!(
            !block_header.is_tagged(),
            "Allocations should be preceded by untagged headers."
        );
        debug_assert!(
            block_header.content_size() >= BLOCK_CONTENT_MIN_SIZE,
            "Allocation size should be at least {BLOCK_MIN_SIZE}."
        );

        self.free_block(block_start);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        raw_ptr(self.__realloc(ptr, layout, new_size).ok())
    }
}

impl<T: Grower> PartialEq for RawMalloc<T> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl<T: Grower> Eq for RawMalloc<T> {}

#[cfg(test)]
mod tests;
