use core::{alloc::Layout, ops::Range};
use spin::Mutex;

use crate::Alloqator;

pub struct Alloq {
    pub heap_start: *const u8,
    pub iter: Mutex<(usize, *mut u8)>,
    pub heap_end: *const u8,
}

impl Alloq {
    pub const fn align(addr: usize, align: usize) -> usize {
        // Since align is a power of two, its binary representation has only a single bit set (e.g. 0b000100000). This means that align - 1 has all the lower bits set (e.g. 0b00011111).
        // By creating the bitwise NOT through the ! operator, we get a number that has all the bits set except for the bits lower than align (e.g. 0b…111111111100000).
        // By performing a bitwise AND on an address and !(align - 1), we align the address downwards. This works by clearing all the bits that are lower than align.
        // Since we want to align upwards instead of downwards, we increase the addr by align - 1 before performing the bitwise AND. This way, already aligned addresses remain the same while non-aligned addresses are rounded to the next alignment boundary.
        // from https://os.phil-opp.com/allocator-designs/
        (addr + align - 1) & !(align - 1)
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

    #[inline(always)]
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut lock = *self.iter.lock();
        lock.0 -= 1;
        if lock.0 == 0 {
            lock.1 = self.heap_start as *mut u8;
            return;
        }
    }
}

crate::impl_allocator!(Alloq);

#[cfg(test)]
pub mod tests {
    use crate::bump::{Alloq, Alloqator};
    extern crate alloc;
    use alloc::{boxed::Box, vec::Vec};

    #[test]
    fn vec_grow() {
        let heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v = Vec::with_capacity_in(10, &alloqer);
        for x in 0..10 {
            v.push(x);
        }
        v.push(255);
    }

    #[test]
    fn boxed() {
        let heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let b = Box::new_in(255u8, &alloqer);
        let c = Box::new_in(127u8, &alloqer);
        let b_ptr = &*b as *const u8;
        let c_ptr = &*c as *const u8;
        assert_ne!(b_ptr, core::ptr::null_mut());
        assert_ne!(c_ptr, core::ptr::null_mut());
        assert_ne!(b_ptr, c_ptr);
    }

    #[test]
    fn custom_structs() {
        struct S {
            _foo: i32,
            _bar: [u16; 8],
            _baz: &'static str,
        }
        let heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v = Vec::with_capacity_in(10, &alloqer);
        for x in 0..10 {
            let y = x as u16;
            let s = S {
                _foo: (x - 5) * 255,
                _bar: [
                    y * 8,
                    y * 8 + 1,
                    y * 8 + 2,
                    y * 8 + 3,
                    y * 8 + 4,
                    y * 8 + 5,
                    y * 8 + 6,
                    y * 8 + 7,
                ],
                _baz: "uga",
            };
            v.push(s)
        }
    }

    #[test]
    fn full_heap() {
        use core::mem::size_of;
        const VECTOR_SIZE: usize = 16;
        let heap_stackish = [0u8; size_of::<[u16; 32]>() * VECTOR_SIZE];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        for x in 0..VECTOR_SIZE {
            let ar: [u16; 32] = core::array::from_fn(|i| (i * x) as u16);
            v.push(ar);
        }
        assert_eq!(alloqer.iter.lock().1 as *const u8, alloqer.heap_end);
    }

    #[test]
    fn zero_sized() {
        use core::mem::size_of;
        const VECTOR_SIZE: usize = 1024;
        let heap_stackish = [0u8; size_of::<()>() * VECTOR_SIZE];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        for _ in 0..VECTOR_SIZE {
            v.push(());
        }
        let b = Box::new_in((), &alloqer);
        assert_eq!(*b, ());
        assert_eq!(v.len(), VECTOR_SIZE);
    }
}
