use core::{alloc::Layout, mem, ops::Range, ptr::null_mut};
use spin::Mutex;

use crate::Alloqator;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlloqMetaData {
    pub start: *const u8,
    pub last_meta: *const u8,
    pub used: bool,
}

impl AlloqMetaData {
    pub fn new(start: *const u8, last_meta: *const u8) -> Self {
        Self {
            start,
            last_meta,
            used: true,
        }
    }

    pub unsafe fn write_meta(&self, obj_start: *mut u8, end: *const u8) -> (*mut u8, *const u8) {
        let ptr_to_write = end.offset(-(mem::size_of::<Self>() as isize));
        *(ptr_to_write as *mut AlloqMetaData) = *self;
        (obj_start, ptr_to_write)
    }

    pub unsafe fn previous_alloc<'a>(ptr: *const u8) -> &'a mut Self {
        let ptr = ptr.offset(-(mem::size_of::<Self>() as isize));
        &mut *(ptr as *mut AlloqMetaData)
    }

    pub unsafe fn from_alloc_ptr<'a>(ptr: *const u8, layout: Layout) -> &'a mut Self {
        let end = ptr.offset(layout.size() as isize);
        let start_meta = crate::align_up(end as usize, mem::align_of::<AlloqMetaData>()) as *mut u8;
        &mut *(start_meta as *mut Self)
    }

    pub unsafe fn after(&self) -> *const u8 {
        let ptr = (self as *const AlloqMetaData) as *const u8;
        ptr.offset(mem::size_of::<AlloqMetaData>() as isize)
    }
}
/// A Deallocation-able Bump allocator. Works like `crate::bump::Alloq`, but has severalmechanisms
/// to deallocate in a stack-ish allocator
pub struct Alloq {
    pub heap_start: *const u8,
    pub iter: Mutex<(usize, *mut u8, *const u8)>,
    pub heap_end: *const u8,
}

impl Alloqator for Alloq {
    type Metadata = AlloqMetaData;

    fn new(heap_range: Range<*const u8>) -> Self {
        Self {
            heap_start: heap_range.start,
            iter: Mutex::new((0, heap_range.start as *mut u8, null_mut())),
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

    /// Similar to `crate::bump::Bump::alloc` (O(1) so), but also allocates a `AlloqMetaData` in the top of
    /// stack, containing where is the block, where is the last `AlloqMetaData` allocated and if
    /// it's being used
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut lock = self.iter.lock();
        let block_start = lock.1;
        let start = crate::align_up(lock.1 as usize, layout.align()) as *mut u8;
        let end = start.offset(layout.size() as isize);
        let start_meta = crate::align_up(end as usize, mem::align_of::<AlloqMetaData>()) as *mut u8;
        let end_meta = start_meta.offset(mem::size_of::<AlloqMetaData>() as isize);
        if end_meta > self.heap_end as *mut u8 {
            panic!("no available memory")
        }
        lock.1 = end_meta;
        lock.0 += 1;
        let (md, last_meta) = AlloqMetaData::new(block_start, lock.2).write_meta(start, end_meta);
        lock.2 = last_meta;
        md
    }

/// Set the `ptr` metadata as unused. If it's on the top of stack, starts to deallocate (return the
/// stack pointer to `last_meta`) all the
/// last areas marked as unused
    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut lock = *self.iter.lock();
        lock.0 -= 1;
        if lock.0 == 0 {
            lock.1 = self.heap_start as *mut u8;
            return;
        }
        let mut meta = AlloqMetaData::from_alloc_ptr(ptr, layout);
        meta.used = false;
        if meta.after() == lock.1 {
            while !meta.used {
                lock.1 = meta.start as *mut u8;
                if meta.last_meta.is_null() {
                    return;
                }
                meta = &mut *(meta.last_meta as *mut AlloqMetaData);
                lock.2 = meta.last_meta;
            }
        }
    }

    #[inline(always)]
    fn reset(&self) {
        let mut lock = self.iter.lock();
        lock.0 = 0;
        lock.1 = self.heap_start() as *mut u8;
        lock.2 = null_mut();
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
        let heap_stackish = [0u8; 1024];
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
            * VECTOR_SIZE
            + 512];
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

    #[test]
    fn non_linear_drop() {
        let heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let b = Box::new_in(10, &alloqer);
        let c = Box::new_in(10, &alloqer);
        drop(b);
        drop(c);
    }
}
