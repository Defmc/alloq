#![feature(allocator_api)]
#![feature(pointer_is_aligned)]
#![no_std]

use core::{
    alloc::Layout,
    ops::Range,
    ptr::{null, NonNull},
};

pub mod list;

//#[cfg(feature = "bump")]
pub mod bump;

}

pub trait Alloqator {
    type Metadata;

    fn assume_init() -> Self
    where
        Self: Sized,
    {
        Self::new(null()..null())
    }

    fn new(heap_range: Range<*const u8>) -> Self
    where
        Self: Sized;

    unsafe fn reset(&self) {
        core::slice::from_raw_parts_mut(
            self.heap_start() as *mut u8,
            self.heap_end().offset_from(self.heap_start()) as usize,
        )
        .fill(0)
    }

    fn heap_start(&self) -> *const u8;
    fn heap_end(&self) -> *const u8;

    unsafe fn alloc(&self, layout: Layout) -> *mut u8;
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);

    fn allocate(
        &self,
        layout: Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        let p = unsafe { self.alloc(layout) };
        let slice = unsafe { core::slice::from_raw_parts_mut(p, layout.size()) };
        Ok(NonNull::new(slice).expect("oh null pointer"))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: core::alloc::Layout) {
        self.dealloc(ptr.as_ptr(), layout);
    }
}

#[macro_export]
macro_rules! impl_allocator {
    ($typ:ty) => {
        unsafe impl core::alloc::Allocator for $typ {
            fn allocate(
                &self,
                layout: core::alloc::Layout,
            ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
                Alloqator::allocate(self, layout)
            }

            unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
                Alloqator::deallocate(self, ptr, layout)
            }
        }

        unsafe impl core::alloc::GlobalAlloc for Alloq {
            unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
                Alloqator::alloc(self, layout)
            }

            unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
                Alloqator::dealloc(self, ptr, layout)
            }
        }
    };
}
