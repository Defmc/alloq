use core::{
    alloc::{AllocError, Allocator, Layout},
    ops::Range,
    ptr::NonNull,
};
use spin::Mutex;

use crate::Alloqator;

/// A simple linear allocator.
/// # Allocation
pub struct Alloq {
    pub heap_start: *const u8,
    pub iter: Mutex<(usize, *mut u8)>,
    pub heap_end: *const u8,
}

unsafe impl Allocator for Alloq {
    /// Introducing an element is O(1). It just set the stack's top to the end of the allocated area
    /// and add 1 to the counter
    fn allocate(&self, layout: core::alloc::Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut lock = self.iter.lock();
        let start = crate::align_up(lock.1 as usize, layout.align()) as *mut u8;
        let end = unsafe { start.offset(layout.size() as isize) };
        if end > self.heap_end as *mut u8 {
            panic!("no available memory")
        }
        lock.1 = end;
        lock.0 += 1;
        let slice = unsafe { core::slice::from_raw_parts_mut(start, layout.size()) };
        NonNull::new(slice).ok_or(AllocError)
    }
    /// Can't deallocate. Just reset. When the counter reaches 0, it reset the stack's top, since
    /// counter being 0 means that there is no vvalue allocated
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        let mut lock = *self.iter.lock();
        lock.0 -= 1;
        if lock.0 == 0 {
            lock.1 = self.heap_start as *mut u8;
            return;
        }
    }
}

impl Alloqator for Alloq {
    type Metadata = ();

    fn new(heap_range: Range<*const u8>) -> Self {
        Self {
            heap_start: heap_range.start,
            iter: Mutex::new((0, heap_range.start as *mut u8)),
            heap_end: heap_range.end,
        }
    }

    #[inline(always)]
    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    #[inline(always)]
    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    #[inline(always)]
    fn reset(&self) {
        let mut lock = self.iter.lock();
        lock.0 = 0;
        lock.1 = self.heap_start() as *mut u8;
    }
}

crate::impl_allocator!(Alloq);

#[cfg(test)]
pub mod tests {
    use super::Alloq;

    include!("test.template.rs");
}
