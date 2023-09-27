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

//#[cfg(feature = "debump")]
pub mod debump;

pub const fn align(addr: usize, align: usize) -> usize {
    // Since align is a power of two, its binary representation has only a single bit set (e.g. 0b000100000). This means that align - 1 has all the lower bits set (e.g. 0b00011111).
    // By creating the bitwise NOT through the ! operator, we get a number that has all the bits set except for the bits lower than align (e.g. 0b…111111111100000).
    // By performing a bitwise AND on an address and !(align - 1), we align the address downwards. This works by clearing all the bits that are lower than align.
    // Since we want to align upwards instead of downwards, we increase the addr by align - 1 before performing the bitwise AND. This way, already aligned addresses remain the same while non-aligned addresses are rounded to the next alignment boundary.
    // from https://os.phil-opp.com/allocator-designs/
    (addr + align - 1) & !(align - 1)
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
        include!("alloqator.template.rs");
    };
}
