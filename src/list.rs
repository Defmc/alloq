use crate::Alloqator;
use core::ops::Range;
use core::{mem, slice};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlloqMetaData {
    size: usize,
}

impl AlloqMetaData {
    pub fn new(size: usize) -> Self {
        Self { size }
    }

    pub unsafe fn write_meta(&self, ptr: *mut u8) -> *mut u8 {
        *(ptr as *mut AlloqMetaData) = *self;
        ptr.offset(core::mem::size_of::<AlloqMetaData>() as isize)
    }

    pub const fn total_size(&self) -> usize {
        self.size + core::mem::size_of::<AlloqMetaData>()
    }

    pub unsafe fn from_ptr(ptr: *const u8) -> AlloqMetaData {
        *(ptr as *const AlloqMetaData)
    }
}
pub struct Alloq {
    heap_start: *const u8,
    heap_end: *const u8,
}

impl Alloqator for Alloq {
    type Metadata = AlloqMetaData;
    fn new(heap_range: Range<*const u8>) -> Self {
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
        }
    }

    #[inline(always)]
    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    #[inline(always)]
    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    #[inline(always)]
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut start = self.heap_start as *mut u8;
        let mut end = self.heap_start as *mut u8;
        let obj_size = (layout.size() + core::mem::size_of::<AlloqMetaData>()) as isize;
        let is_aligned = |p: *mut u8| {
            (p as usize) % (layout.align() + mem::size_of::<AlloqMetaData>()) == 0
                && (p as usize) % mem::align_of::<AlloqMetaData>() == 0
        };
        loop {
            if *end != 0 {
                let metadata = end as *const AlloqMetaData;
                end = end.offset((*metadata).total_size() as isize);
                start = end;
            }
            end = end.offset(1);
            if !is_aligned(start) && is_aligned(end) {
                start = end;
            }
            if end.offset_from(start) >= obj_size && is_aligned(start) {
                return AlloqMetaData::new(layout.size()).write_meta(start as *mut u8);
            }
            if end as *const u8 >= self.heap_end {
                panic!("no available memory");
            }
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let offset = ptr.offset(-(core::mem::size_of::<AlloqMetaData>() as isize));
        slice::from_raw_parts_mut(
            offset,
            layout.size() + core::mem::size_of::<AlloqMetaData>(),
        )
        .fill(0)
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
