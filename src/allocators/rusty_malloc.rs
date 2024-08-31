//! A multithreaded memory allocator.

use crate::allocators::RawMalloc;
use crate::growers::Grower;

use core::ptr::NonNull;
use core::alloc::{Allocator, GlobalAlloc, AllocError, Layout};
use std::sync::Mutex;

/// A multithreaded memory allocator.
///
/// This allocator is just a `Mutex` wrapper over [`RawMalloc`] to allow for multithreading.
#[repr(C)]
pub struct RustyMalloc<T: ?Sized + Grower> {
    inner: Mutex<RawMalloc<T>>,
}

impl<T: Grower> RustyMalloc<T> {
    /// # Safety
    /// Callers must make sure that the provided grower will be the only object
    /// managing it's underlying buffer for the lifetime of the returned allocator.
    pub const unsafe fn with_grower(grower: T) -> Self {
        RustyMalloc {
            inner: Mutex::new(RawMalloc::with_grower(grower)),
        }
    }
}

unsafe impl<T: Grower> Sync for RustyMalloc<T> {}

//---------------impl Allocator for RustyMalloc---------------//

unsafe impl<T: Grower> Allocator for RustyMalloc<T> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        (*self.inner.lock().unwrap()).allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        (*self.inner.lock().unwrap()).deallocate(ptr, layout)
    }

    unsafe fn grow(
            &self,
            ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
        Allocator::grow(&*self.inner.lock().unwrap(), ptr, old_layout, new_layout)
    }

    unsafe fn shrink(
            &self,
            ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
        (*self.inner.lock().unwrap()).shrink(ptr, old_layout, new_layout)
    }
}

//---------------impl GlobalAlloc for RustyMalloc---------------//

unsafe impl<T: Grower> GlobalAlloc for RustyMalloc<T> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        (*self.inner.lock().unwrap()).alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        (*self.inner.lock().unwrap()).dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        (*self.inner.lock().unwrap()).realloc(ptr, layout, new_size)
    }
}
