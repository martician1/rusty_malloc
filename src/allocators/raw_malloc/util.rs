//! Utility functions specific to the [`RawMalloc`](super::RawMalloc) allocator.

use core::ptr::NonNull;
use std::alloc::Layout;

use super::{BLOCK_CONTENT_MIN_ALIGN, BLOCK_CONTENT_MIN_SIZE, BLOCK_MIN_SIZE};
use crate::header::{Header, HEADER_SIZE};
use crate::util::find_aligned;

/// Returns the smallest integer `z` such that `z ≥ x` and `z = y.k` for some integer `k`.
/// or `None` if that integer can not be contained in a `uzise`.
///
/// # Panics
/// Panics if `y` is 0.
#[inline]
pub fn find_divisible(x: usize, y: usize) -> Option<usize> {
    if x % y == 0 {
        Some(x)
    } else {
        ((x / y) * y).checked_add(y)
    }
}

/// Returns a pointer to the smallest `obj_align`-aligned address after `block_start`
/// such that the gap between the pointer and `block_start` is either
/// [`HEADER_SIZE`] or greater or equal to [`BLOCK_MIN_SIZE`] + [`HEADER_SIZE`]
/// or `None` if no such address exists.
///
/// # Panics
/// Panics if `obj_align` is not a power of 2.
///
/// [`HEADER_SIZE`]: crate::header::HEADER_SIZE
/// [`BLOCK_MIN_SIZE`]: super::BLOCK_MIN_SIZE
pub fn find_place(block_start: *const u8, obj_align: usize) -> Option<NonNull<u8>> {
    let mut obj_start = block_start;
    loop {
        let dist = obj_start as usize - block_start as usize;

        if dist == HEADER_SIZE || dist >= HEADER_SIZE + BLOCK_MIN_SIZE {
            break;
        }
        if obj_start as usize == usize::MAX {
            return None;
        }
        obj_start = find_aligned(unsafe { obj_start.add(1).cast() }, obj_align)?;
    }
    unsafe { Some(NonNull::new_unchecked(obj_start as *mut u8)) }
}

/// Augments `size` to a size that can be used for an allocation
/// or returns `Err(())` if the size can not be augmented.
#[inline]
pub fn augment_size(size: usize) -> Result<usize, ()> {
    // Size of objects should not exceed isize::MAX.
    // https://doc.rust-lang.org/std/ptr/index.html#allocated-object
    match find_divisible(size.max(BLOCK_CONTENT_MIN_SIZE), HEADER_SIZE) {
        Some(new_size) if new_size as isize > 0 => Ok(new_size),
        _ => Err(()),
    }
}

/// Augments `layout` to a layout that can be used by the allocator
/// or returns `Err(())` if the layout can not be augmented.
pub fn augment_layout(layout: Layout) -> Result<Layout, ()> {
    let obj_align = layout.align().max(BLOCK_CONTENT_MIN_ALIGN);
    let obj_size = augment_size(layout.size())?;

    debug_assert!(Layout::from_size_align(obj_size, obj_align).is_ok());
    unsafe { Ok(Layout::from_size_align_unchecked(obj_size, obj_align)) }
}

/// Converts an object pointer to a fat pointer to the full object content.
///
/// # Safety
/// This function is unsafe since it assumes that `obj_start` points to a valid object.
/// In particular, is expected that as part of an occupied block the object is preceded
/// by an untagged header.
#[inline]
pub unsafe fn to_nonnull_slice(obj_start: NonNull<u8>) -> NonNull<[u8]> {
    let block_header: *const Header = unsafe { obj_start.as_ptr().sub(HEADER_SIZE).cast() };
    debug_assert!(!(*block_header).is_tagged());
    let obj_size = (*block_header).__content_size;
    NonNull::slice_from_raw_parts(obj_start, obj_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HEADER_ALIGN;
    use core::ptr::null;

    #[test]
    fn test_find_divisible_1() {
        assert_eq!(find_divisible(5, 5).unwrap(), 5);
        assert_eq!(find_divisible(5, 10).unwrap(), 10);
        assert_eq!(find_divisible(0, 100).unwrap(), 0);
    }

    #[test]
    #[should_panic]
    fn test_find_divisible_2() {
        let _ = find_divisible(5, 0);
    }

    #[test]
    fn test_find_divisible_3() {
        assert!(find_divisible(usize::MAX, 2).is_none());
        assert_eq!(find_divisible(usize::MAX - 7, 8), Some(usize::MAX - 7));
    }

    #[test]
    fn test_find_place_1() {
        assert_eq!(
            find_place(null(), HEADER_ALIGN).unwrap().as_ptr() as usize,
            HEADER_SIZE
        );
        assert!(
            find_place(1 as *const u8, HEADER_ALIGN)
                .unwrap()
                .as_ptr() as usize
                > BLOCK_MIN_SIZE + HEADER_SIZE
        );

        assert!(find_place(usize::MAX as *const u8, HEADER_ALIGN).is_none());
    }

    #[test]
    #[should_panic]
    fn test_find_place_2() {
        find_place(null(), 7);
    }

    #[test]
    fn test_find_place_3() {
        for i in 4000..5000 {
            for j in 0..20 {
                let tmp = find_place(i as *const u8, 1 << j).unwrap().as_ptr();
                assert_eq!(tmp as usize % (1 << j), 0);
                let diff = tmp as usize - i as usize;
                assert!(diff == HEADER_SIZE || diff >= BLOCK_MIN_SIZE + HEADER_SIZE);
            }
        }
    }

    #[test]
    fn test_augment_layout_1() {
        for size in HEADER_SIZE + 1..=2 * HEADER_SIZE {
            for align in (0..=usize::ilog2(HEADER_ALIGN)).map(|i| 1 << i) {
                let layout = Layout::from_size_align(size, align).unwrap();
                let augmented = augment_layout(layout).unwrap();
                assert_eq!(
                    augmented,
                    Layout::from_size_align(HEADER_SIZE * 2, BLOCK_CONTENT_MIN_ALIGN).unwrap()
                );
            }
        }
    }

    #[test]
    fn test_to_nonnull_slice() {
        let obj_size = 20;
        let mut header = unsafe { Header::new_unchecked(obj_size, false) };
        let block_start: *mut u8 = (&mut header as *mut Header).cast();
        let obj_start = unsafe { block_start.add(HEADER_SIZE) };
        let obj_as_slice = unsafe { to_nonnull_slice(NonNull::new(obj_start).unwrap()) };
        assert_eq!(obj_as_slice.as_ptr() as *mut u8, obj_start);
        assert_eq!(obj_as_slice.len(), obj_size);
    }
}
