use core::{alloc::Layout, ops::Range};
use spin::Mutex;

use crate::Alloqator;

/// A simple linear allocator.
/// # Allocation
pub struct Alloq {
    pub heap_start: *const u8,
    pub iter: Mutex<(usize, *mut u8)>,
    pub heap_end: *const u8,
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

/// Introducing an element is O(1). It just set the stack's top to the end of the allocated area
/// and add 1 to the counter
#[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut lock = self.iter.lock();
        let start = crate::align_up(lock.1 as usize, layout.align()) as *mut u8;
        let end = start.offset(layout.size() as isize);
        if end > self.heap_end as *mut u8 {
            panic!("no available memory")
        }
        lock.1 = end;
        lock.0 += 1;
        start
    }
/// Can't deallocate. Just reset. When the counter reaches 0, it reset the stack's top, since
/// counter being 0 means that there is no vvalue allocated

    #[inline(always)]
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut lock = *self.iter.lock();
        lock.0 -= 1;
        if lock.0 == 0 {
            lock.1 = self.heap_start as *mut u8;
            return;
        }
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
    use crate::Alloqator;
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
    fn fragmented_heap() {
        let heap_stackish = [0u8; 1024 * 1024];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v: Vec<u8, _> = Vec::new_in(&alloqer);
        let mut w: Vec<u8, _> = Vec::new_in(&alloqer);
        for x in 0..128 {
            match x % 2 {
                0 => v.push(x),
                1 => w.push(x),
                _ => unreachable!(),
            }
        }
        assert!(v.iter().all(|i| i % 2 == 0));
        assert!(w.iter().all(|i| i % 2 == 1));
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
        let heap_stackish = [0u8; (size_of::<<Alloq as Alloqator>::Metadata>()
            + size_of::<[u16; 32]>())
            * VECTOR_SIZE];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        for x in 0..VECTOR_SIZE {
            let ar: [u16; 32] = core::array::from_fn(|i| (i * x) as u16);
            v.push(ar);
        }
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
