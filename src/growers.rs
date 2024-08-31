//! [`Grower`] trait and structures that implement it.
//!
//! The [`Grower`] trait allows users to easily change the underlying
//! buffer on which allocators in [`rusty_malloc::allocators`](crate::allocators) operate.

use core::ptr::NonNull;

use libc::{brk, sbrk};

use super::header::HEADER_ALIGN;
use super::util::{checked_add, find_aligned};

/// A trait for types that act as if they were a contiguous growable buffer.
///
/// # Safety
/// * copying, cloning, or moving the grower must not invalidate any pointers to the buffer
///   managed by the grower. This generally means that growers should not own but
///   reference their underlying buffers.
pub unsafe trait Grower {
    /// Tries to grow the heap with `size` bytes.
    /// Returns the old end of the heap or `Err(())` if
    /// the growth failed.
    fn grow(&mut self, size: usize) -> Result<NonNull<u8>, ()>;
}

/// A grower that internally uses [`libc::brk`] to operate
/// on the end of the process's data segment.
pub struct BrkGrower {
    heap_end: Option<NonNull<u8>>,
}

impl BrkGrower {
    #[inline(always)]
    pub const fn new() -> Self {
        BrkGrower { heap_end: None }
    }

    /// Tries to initialize the grower by calling `sbrk(0)` to get the initial heap end.
    /// Returns `Err(())` if the grower could not be initialized.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that the grower
    /// wasn't previously initialized and that there aren't any other
    /// objects (growers or not) managing the program brake.
    unsafe fn try_init(&mut self) -> Result<(), ()> {
        debug_assert!(self.heap_end.is_none());
        let heap_end = unsafe { sbrk(0) };
        debug_assert_ne!(heap_end as isize, -1, "Calling sbrk(0) should never fail.");
        debug_assert_ne!(heap_end as usize, 0);
        unsafe {
            self.heap_end = Some(NonNull::new_unchecked(
                find_aligned(heap_end.cast(), HEADER_ALIGN).ok_or(())? as *mut u8,
            ))
        };
        Ok(())
    }
}

impl Default for BrkGrower {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Grower for BrkGrower {
    fn grow(&mut self, size: usize) -> Result<NonNull<u8>, ()> {
        if self.heap_end.is_none() {
            unsafe { self.try_init()? };
        }
        let heap_end = self.heap_end.unwrap();
        if size == 0 {
            return Ok(heap_end);
        }
        let new_heap_end: *mut u8 = checked_add(heap_end.as_ptr(), size).ok_or(())? as _;
        if unsafe { brk(new_heap_end.cast()) == -1 } {
            return Err(());
        }
        self.heap_end = unsafe { Some(NonNull::new_unchecked(new_heap_end)) };
        Ok(heap_end)
    }
}

#[cfg(test)]
pub mod arena_grower {
    use super::Grower;
    use crate::util::checked_add;
    use core::ptr::NonNull;

    /// An inherently unsafe grower that operates on an arena.
    /// This structure is intended solely for debugging purposes.
    pub struct ArenaGrower {
        heap_end: *mut u8,
        arena_end: *mut u8,
    }

    impl ArenaGrower {
        /// Creates a new arena that operates on the provided buffer.
        pub const fn new(buf: *mut u8, size: usize) -> Self {
            let heap_end = buf;
            let arena_end = unsafe { buf.add(size) };
            ArenaGrower {
                heap_end,
                arena_end,
            }
        }
    }

    unsafe impl Grower for ArenaGrower {
        fn grow(&mut self, size: usize) -> Result<NonNull<u8>, ()> {
            let heap_end = self.heap_end;
            let new_heap_end = checked_add(heap_end, size).ok_or(())? as *mut u8;
            if new_heap_end > self.arena_end {
                return Err(());
            }
            self.heap_end = new_heap_end;
            Ok(NonNull::new(heap_end).unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_grower::ArenaGrower;

    #[test]
    fn test_arena_grower_1() {
        let mut buf = [0_u8; 2048];
        let mut arena = ArenaGrower::new(buf.as_mut_ptr(), buf.len());
        let p = buf.as_mut_ptr();
        unsafe {
            assert_eq!(p, arena.grow(0).unwrap().as_ptr());
            assert_eq!(p, arena.grow(20).unwrap().as_ptr());
            assert_eq!(p.add(20), arena.grow(20).unwrap().as_ptr());
            assert_eq!(p.add(40), arena.grow(24).unwrap().as_ptr());
            assert_eq!(p.add(64), arena.grow(2048 - 64).unwrap().as_ptr());
            assert_eq!(p.add(2048), arena.grow(0).unwrap().as_ptr());
            assert!(arena.grow(1).is_err());
            assert!(arena.grow(8).is_err());
        }
    }

    #[test]
    fn test_arena_grower_2() {
        let mut buf = [0_u8; 64];
        let mut arena = ArenaGrower::new(buf.as_mut_ptr(), 0);
        assert!(arena.grow(1).is_err());
        assert!(arena.grow(4).is_err());
        assert!(arena.grow(8).is_err());
    }

    #[test]
    fn test_arena_grower_3() {
        let mut buf = [0_u8; 128];
        let mut arena = ArenaGrower::new(buf.as_mut_ptr(), 13);
        let p = buf.as_mut_ptr();
        unsafe {
            assert_eq!(p, arena.grow(1).unwrap().as_ptr());
            assert_eq!(p.add(1), arena.grow(4).unwrap().as_ptr());
            assert_eq!(p.add(5), arena.grow(8).unwrap().as_ptr());
            assert_eq!(p.add(13), arena.grow(0).unwrap().as_ptr());
            assert!(arena.grow(1).is_err());
        }
    }
}
