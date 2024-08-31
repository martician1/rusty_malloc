//! Defines the [`Header`] struct and associated constants and functions.

use std::mem::{align_of, size_of};

pub const HEADER_SIZE: usize = size_of::<Header>();
pub const HEADER_ALIGN: usize = align_of::<Header>();

/// Stores information about a block.
/// Currently this is the block content size (excludes the size of the header itself)
/// and whether the block is free or occupied.
///
/// # Tagging
/// To reduce the memory footprint of headers the block free status is kept
/// in the least significant bit of the `__content_size` field. This technique is called
/// tagging, in our case a tagged header denotes a free block and an untagged
/// header denotes an occupied block.
///
/// Relying on tagging is safe since [`BLOCK_CONTENT_MIN_ALIGN`]
/// is guaranteed to be at least 2 bytes and thus the size of any block content would always be
/// divisible by 2.
///
/// [`BLOCK_CONTENT_MIN_ALIGN`]: crate::allocators::raw_malloc::BLOCK_CONTENT_MIN_ALIGN
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Header {
    /// Because of tagging this value does not always correspond to the size of the block content.
    /// If you wish to access the real content size use [`content_size()`](Header::content_size)
    pub __content_size: usize,
}

impl Header {
    /// Creates a new header for a block with the specified content size and free status.
    ///
    /// # Safety
    /// This function is unsafe since it assumes that `content_size` is even.
    #[inline(always)]
    pub unsafe fn new_unchecked(content_size: usize, is_free: bool) -> Header {
        debug_assert_eq!(content_size % 2, 0, "size should be even.");
        match is_free {
            true => (Header { __content_size: content_size }).tagged(),
            false => Header { __content_size: content_size },
        }
    }

    /// Returns a tagged version of the header.
    #[inline(always)]
    pub fn tagged(&self) -> Header {
        Header { __content_size: self.__content_size | 1 }
    }

    /// Returns an untagged version of the header.
    #[inline(always)]
    pub fn untagged(&self) -> Header {
        Header { __content_size: self.__content_size & !1 }
    }

    /// Returns whether the header is tagged.
    #[inline(always)]
    pub fn is_tagged(&self) -> bool {
        self.__content_size & 1 != 0
    }

    /// Returns the size of the block contents.
    #[inline(always)]
    pub fn content_size(&self) -> usize {
        self.untagged().__content_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn test_1() {
        // Should panic because of debug assertion in new_unchecked().
        let _h = unsafe { Header::new_unchecked(21, false) };
    }

    #[test]
    fn test_2() {
        let h = unsafe {
            Header::new_unchecked(20, true)
        };

        assert!(h.is_tagged());
        assert_eq!(h.content_size(), 20);
        assert_eq!(h.__content_size, 21);

        let h = h.untagged();
        assert_eq!(h.content_size(), 20);
        assert_eq!(h.content_size(), h.__content_size);
    }

    #[test]
    fn test_3() {
        let h = unsafe {
            Header::new_unchecked(20, false)
        };

        assert!(!h.is_tagged());
        assert_eq!(h.content_size(), 20);
        assert_eq!(h.content_size(), h.__content_size);

        let h = h.tagged();
        assert_eq!(h.content_size(), 20);
        assert_eq!(h.__content_size, 21);
    }

    #[test]
    fn test_4() {
        let h = unsafe {
            Header::new_unchecked(20, false)
        };

        assert_eq!(h.untagged(), h);
        assert_eq!(h.tagged().untagged(), h);
    }

    #[test]
    fn test_5() {
        let h = unsafe {
            Header::new_unchecked(20, true)
        };

        assert_eq!(h.tagged(), h);
        assert_eq!(h.untagged().tagged(), h);
    }
}
