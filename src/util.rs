//! Utility functions.

use core::ptr::{null_mut, NonNull};

/// Returns the smallest (in address) `align`-aligned pointer
/// with an address greater or equal to that of `ptr`
/// or `None` if no such pointer exists.
///
/// # Panics
/// Panics if `align` is not a power-of-two.
#[inline]
pub(super) fn find_aligned(ptr: *const u8, align: usize) -> Option<*const u8> {
    unsafe {
        let offset = ptr.align_offset(align);
        debug_assert_ne!(
            offset,
            usize::MAX,
            "align_offset() on a *const u8 should never fail."
        );
        if usize::MAX - offset < ptr as usize {
            return None;
        }
        Some(ptr.add(offset))
    }
}

#[inline(always)]
pub(super) fn raw_ptr<T>(p: Option<NonNull<T>>) -> *mut T {
    p.map_or(null_mut(), |p| p.as_ptr())
}

#[inline(always)]
pub(super) fn checked_add(ptr: *const u8, offset: usize) -> Option<*const u8> {
    unsafe { (ptr as usize <= usize::MAX - offset).then_some(ptr.add(offset)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null;

    #[test]
    fn test_find_aligned_1() {
        for i in 0..1000 {
            for j in 0..=5 {
                let alignment = 1 << j;
                let align_mask = !(alignment - 1);
                assert_eq!(
                    find_aligned(i as *const u8, alignment).unwrap() as usize,
                    ((i + alignment - 1) & align_mask)
                );
            }
        }
    }

    #[test]
    fn test_find_aligned_2() {
        for i in usize::MAX - 14..=usize::MAX {
            assert!(find_aligned(i as *mut u8, 16).is_none());
        }
        assert_eq!(
            find_aligned((usize::MAX - 15) as *const u8, 16),
            Some((usize::MAX - 15) as *const u8)
        );
    }

    #[test]
    #[should_panic]
    fn test_find_aligned_3() {
        find_aligned(null(), 5);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_find_aligned_4() {
        for i in 0..=u16::MAX {
            for j in 0..size_of::<usize>() {
                let _ = find_aligned(i as *const u8, 1 << j);
            }
        }
    }
}
