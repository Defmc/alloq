use spin::Mutex;
use std::{
    alloc::{Allocator, GlobalAlloc, Layout},
    ops::Range,
    ptr::NonNull,
};

pub struct Alloq {
    pub heap_start: *const u8,
    pub iter: Mutex<(usize, *mut u8)>,
    pub heap_end: *const u8,
}

unsafe impl Allocator for Alloq {
    fn allocate(&self, layout: Layout) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        println!("allocating");
        let p = unsafe { self.alloc(layout) };
        println!("slicing {p:?}");
        let slice = unsafe { core::slice::from_raw_parts_mut(p, layout.size()) };
        println!("passing test");
        Ok(NonNull::new(slice).expect("oh null pointer"))
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: Layout) {
        self.dealloc(ptr.as_ptr(), layout);
    }
}

unsafe impl GlobalAlloc for Alloq {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc(ptr, layout)
    }
}

impl Alloq {
    pub const fn assume_init() -> Self {
        Self::new(0..0)
    }

    pub const fn new(heap_range: Range<usize>) -> Self {
        Self {
            heap_start: heap_range.start as *const u8,
            iter: Mutex::new((0, heap_range.start as *mut u8)),
            heap_end: heap_range.end as *const u8,
        }
    }

    pub const fn from_ptr(heap_range: Range<*const u8>) -> Self {
        Self {
            heap_start: heap_range.start,
            iter: Mutex::new((0, heap_range.start as *mut u8)),
            heap_end: heap_range.end,
        }
    }
    pub const fn align(addr: usize, align: usize) -> usize {
        // Since align is a power of two, its binary representation has only a single bit set (e.g. 0b000100000). This means that align - 1 has all the lower bits set (e.g. 0b00011111).
        // By creating the bitwise NOT through the ! operator, we get a number that has all the bits set except for the bits lower than align (e.g. 0b…111111111100000).
        // By performing a bitwise AND on an address and !(align - 1), we align the address downwards. This works by clearing all the bits that are lower than align.
        // Since we want to align upwards instead of downwards, we increase the addr by align - 1 before performing the bitwise AND. This way, already aligned addresses remain the same while non-aligned addresses are rounded to the next alignment boundary.
        // from https://os.phil-opp.com/allocator-designs/
        (addr + align - 1) & !(align - 1)
    }

    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut lock = self.iter.lock();
        let start = Self::align(lock.1 as usize, layout.align()) as *mut u8;
        let end = start.offset(layout.size() as isize);
        if end > self.heap_end as *mut u8 {
            panic!("no available memory")
        }
        lock.1 = end;
        lock.0 += 1;
        start
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut lock = *self.iter.lock();
        lock.0 -= 1;
        if lock.0 == 0 {
            lock.1 = self.heap_start as *mut u8;
            return;
        }
    }
}

