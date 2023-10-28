extern crate std;
use core::alloc::Allocator;
use std::alloc::System;

use crate::Alloqator;

pub struct Alloq(System);

// It's verbose to garantee the system optimizations for each method instead of using a generic
// impl.
unsafe impl Allocator for Alloq {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.0.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        self.0.deallocate(ptr, layout)
    }

    unsafe fn grow(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.0.grow(ptr, old_layout, new_layout)
    }

    unsafe fn shrink(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.0.shrink(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.0.grow_zeroed(ptr, old_layout, new_layout)
    }

    fn allocate_zeroed(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        self.0.allocate_zeroed(layout)
    }
}

impl Alloqator for Alloq {
    type Metadata = ();
    fn new(_heap_range: core::ops::Range<*mut u8>) -> Self
    where
        Self: Sized,
    {
        Self(System)
    }

    unsafe fn reset(&self) {}

    fn heap_start(&self) -> *mut u8 {
        panic!("there's no heap start");
    }

    fn heap_end(&self) -> *mut u8 {
        panic!("there's no heap end");
    }
}

#[cfg(test)]
pub mod tests {
    use super::Alloq;

    include!("test.template.rs");
}
