#![feature(allocator_api)]
#![no_std]
// #![warn(
//     clippy::all,
//     clippy::restriction,
//     clippy::pedantic,
//     clippy::nursery,
//     clippy::cargo
// )]
use core::{
    alloc::{Allocator, Layout},
    mem,
    ops::Range,
    ptr::NonNull,
};

//#[cfg(feature = "bump")]
pub mod bump;

//#[cfg(feature = "debump")]
pub mod debump;

//#[cfg(feature = "pool")]
pub mod pool;

//#[cfg(feature = "list")]
pub mod list;

//#[cfg(feature = "static")]
pub mod statiq;

//#[cfg(feature = "system")]
pub mod system;

pub const fn align_up(addr: usize, align: usize) -> usize {
    // Since align is a power of two, its binary representation has only a single bit set (e.g. 0b000100000). This means that align - 1 has all the lower bits set (e.g. 0b00011111).
    // By creating the bitwise NOT through the ! operator, we get a number that has all the bits set except for the bits lower than align (e.g. 0b…111111111100000).
    // By performing a bitwise AND on an address and !(align - 1), we align the address downwards. This works by clearing all the bits that are lower than align.
    // Since we want to align upwards instead of downwards, we increase the addr by align - 1 before performing the bitwise AND. This way, already aligned addresses remain the same while non-aligned addresses are rounded to the next alignment boundary.
    // from https://os.phil-opp.com/allocator-designs/
    (addr + align - 1) & !(align - 1)
}

// took from https://docs.rs/polymorph-allocator/latest/src/polymorph_allocator/align.rs.html#9-17
// thanks, ~ren!
pub const fn align_down(addr: usize, align: usize) -> usize {
    if align.is_power_of_two() {
        addr & !(align - 1)
    } else if align == 0 {
        addr
    } else {
        panic!("non power-of-two alignment");
    }
}

pub trait Alloqator: Allocator {
    type Metadata;

    fn new(heap_range: Range<*mut u8>) -> Self
    where
        Self: Sized;

    /// Resets the allocator.
    /// # Safety
    /// Can corrupt previous allocation it they was not deallocated. Make sure to deallocate
    /// everything before call it.
    unsafe fn reset(&self);

    /// Resets the allocator and the heap, setting all bytes to zero.
    /// # Safety
    /// Can corrupt previous allocation it they was not deallocated. Make sure to deallocate
    /// everything before call it.
    unsafe fn hard_reset(&self) {
        let len = self.heap_end().offset_from(self.heap_start()) as usize;
        core::slice::from_raw_parts_mut(self.heap_start(), len).fill(0);
        self.reset();
    }

    fn heap_start(&self) -> *mut u8;
    fn heap_end(&self) -> *mut u8;
    fn heap_range(&self) -> Range<*mut u8> {
        self.heap_start()..self.heap_end()
    }

    fn alloq(&self, layout: Layout) -> *mut u8 {
        unsafe { (*self.allocate(layout).unwrap().as_ptr()).as_ptr() as *mut u8 }
    }

    /// # Safety
    /// It should just be called ONCE per allocation. Some allocators like `crate::list` and
    /// `crate::system` can handle it and panic, others will silently start to cause UB.
    unsafe fn dealloq(&self, ptr: *mut u8, layout: Layout) {
        self.deallocate(NonNull::new(ptr).unwrap(), layout);
    }
}

pub const fn get_size_hint_in<T, A: Alloqator>(count: usize) -> usize {
    const fn max(x: usize, y: usize) -> usize {
        if x > y {
            x
        } else {
            y
        }
    }
    let meta_align = max(mem::align_of::<A::Metadata>(), 1);
    let obj_align = max(mem::align_of::<A::Metadata>(), 1);
    (mem::size_of::<T>() + obj_align - 1 + mem::size_of::<A::Metadata>() + meta_align - 1) * count
}
#[macro_export]
macro_rules! impl_allocator {
    ($typ:ty) => {
        include!("alloqator.template.rs");
    };
}
