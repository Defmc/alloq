use core::{mem, ops::Range, ptr::null_mut};

use crate::Alloqator;
use spin::Mutex;

pub const DEFAULT_CHUNK_SIZE: usize = 512;
pub const DEFAULT_ALIGNMENT: usize = 2;

#[derive(Clone, Debug)]
pub struct RawChunk {
    pub addr: *const u8,
    pub chunk: *const u8,
    pub next: *mut Self,
    pub back: *mut Self,
}

impl RawChunk {
    pub unsafe fn new(bind: *const u8, align: usize) -> Self {
        Self {
            addr: crate::align_up(bind as usize, align) as *const u8,
            chunk: bind,
            next: null_mut(),
            back: null_mut(),
        }
    }

    pub unsafe fn allocate<'a>(&self, end: *mut u8) -> *mut Self {
        let ptr = end.offset(-(mem::size_of::<Self>() as isize));
        let aligned_ptr = crate::align_down(ptr as usize, mem::align_of::<Self>()) as *mut Self;
        let aligned = aligned_ptr as *mut Self;
        *aligned = self.clone();
        aligned
    }

    pub fn next(mut self, next: *mut Self) -> Self {
        self.next = next;
        self
    }

    pub fn back(mut self, back: *mut Self) -> Self {
        self.back = back;
        self
    }

    pub unsafe fn alloc_next<'a>(
        &mut self,
        list_end: &mut *const RawChunk,
        chunk_size: usize,
    ) -> *mut Self {
        let last_alloc = *list_end as *mut Self;
        let addr = (*last_alloc).addr.offset(chunk_size as isize);
        let next = Self::new(addr, 1 /* already aligned*/)
            .back(last_alloc)
            .allocate(last_alloc.offset(-1) as *mut u8);
        assert!(
            next.offset(1) <= last_alloc,
            "filling the previous node {:?}-{:?} in {last_alloc:?}",
            // rust-analyzer can't rename inside strings?
            next,
            next.offset(mem::size_of::<Self>() as isize)
        );
        *list_end = next;
        Self::connect_unchecked(self, &mut *next);
        next
    }

    pub unsafe fn disconnect(&mut self) {
        let next = self.next;
        let back = self.back;
        if !self.back.is_null() {
            (*back).next = next;
        }
        if !self.next.is_null() {
            (*next).back = back;
        }
        self.next = null_mut();
        self.back = null_mut();
    }

    pub unsafe fn insert_in_list(&mut self, back: &mut RawChunk) {
        self.disconnect();
        let next = back.next;
        Self::connect_unchecked(back, self);
        if !next.is_null() {
            Self::connect_unchecked(self, &mut *next);
        }
    }

    pub unsafe fn connect(back: &mut Self, next: &mut Self) {
        assert!(back.next.is_null());
        assert!(next.back.is_null());
        Self::connect_unchecked(back, next);
    }

    pub unsafe fn connect_unchecked(back: &mut Self, next: &mut Self) {
        back.next = next as *mut Self;
        next.back = back as *mut Self;
    }

    pub unsafe fn iter(&self) -> RawChunkIter {
        RawChunkIter((self as *const RawChunk) as *mut RawChunk)
    }
}

pub struct RawChunkIter(*mut RawChunk);

impl Iterator for RawChunkIter {
    type Item = *mut RawChunk;
    fn next(&mut self) -> Option<Self::Item> {
        let r = self.0;
        if r.is_null() {
            None
        } else {
            self.0 = unsafe { (*self.0).next };
            Some(r)
        }
    }
}

#[derive(Debug)]
pub struct Alloq {
    heap_start: *const u8,
    heap_end: *const u8,
    chunk_size: usize,
    pooler: Mutex<Pool>,
}

#[derive(Debug)]
pub struct Pool {
    used_head: *mut RawChunk,
    free_last: *mut RawChunk,
    used_last: *mut RawChunk,
    list_end: *const RawChunk,
}

impl Pool {
    pub unsafe fn push_used(&mut self, raw_chunk: *mut RawChunk) {
        if self.used_head.is_null() {
            self.used_head = raw_chunk;
        }
        if self.used_last.is_null() {
            self.used_last = raw_chunk;
        } else {
            let last = &mut *self.used_last;
            if !last.next.is_null() {
                panic!("there's a `next` in `used_last`");
            }
            RawChunk::connect(last, &mut *raw_chunk);
            self.used_last = raw_chunk;
        }
    }

    pub unsafe fn get_free_chunk(&mut self, chunk_size: usize) -> *mut RawChunk {
        let last = &mut *self.free_last;
        let freed = if last.next.is_null() {
            &mut *last.alloc_next(&mut self.list_end, chunk_size)
        } else {
            &mut *last.next
        };
        freed.disconnect();
        freed
    }

    pub unsafe fn remove_used(&mut self, addr: *const u8) {
        let last_free = &mut *self.free_last;
        for raw_chunk in (*self.used_head).iter() {
            let raw_chunk = &mut *(raw_chunk);
            if raw_chunk.addr == addr {
                if self.used_head == raw_chunk {
                    self.used_head = raw_chunk.next;
                }
                if self.used_last == raw_chunk {
                    self.used_last = raw_chunk.back;
                }
                raw_chunk.insert_in_list(last_free);
            }
            return;
        }
        panic!("invalid chunk");
    }
}

impl Alloq {
    pub unsafe fn with_chunk_size(
        heap_range: Range<*const u8>,
        chunk_size: usize,
        align: usize,
    ) -> Self {
        let free_last = RawChunk::new(heap_range.start, align).allocate(heap_range.end as *mut u8);
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            chunk_size,
            pooler: Pool {
                used_head: null_mut(),
                free_last,
                used_last: null_mut(),
                list_end: free_last,
            }
            .into(),
        }
    }
}

// Why rustfmt is removing comments?
// impl /*Alloqator for*/ Pool {
impl Alloqator for Alloq {
    type Metadata = ();

    fn new(heap_range: Range<*const u8>) -> Self {
        unsafe { Self::with_chunk_size(heap_range, 512, 2) }
    }

    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut pooler = self.pooler.lock();
        let chunk = pooler.get_free_chunk(self.chunk_size);
        pooler.push_used(chunk);
        if layout.size() > self.chunk_size {
            todo!(
                "layout (size {} bytes and align {} bytes) cannot be allocated in a chunk ({} bytes)",
                layout.size(),
                layout.align(),
                self.chunk_size
            );
        }
        let addr = crate::align_up(chunk as usize, layout.align()) as *mut u8;
        (*chunk).addr = addr;
        addr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        let mut pooler = self.pooler.lock();
        pooler.remove_used(ptr);
    }

    // TODO: Improve shrink and grow by simply link another pointer
}

crate::impl_allocator!(Alloq);

#[cfg(test)]
pub mod tests {
    extern crate alloc;
    extern crate std;

    use alloc::{boxed::Box, vec::Vec};

    use super::Alloq;
    use crate::Alloqator;
    use core::{alloc::Layout, ptr::null_mut};

    #[test]
    fn simple_alloc() {
        let heap = [0u8; 512 * 8];
        let alloqer = Alloq::new(heap.as_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let ptr = unsafe { alloqer.alloc(layout) };
        unsafe { alloqer.dealloc(ptr, layout) };
    }

    #[test]
    fn linear_allocs() {
        let heap = [0u8; 512 * 512];
        let alloqer = Alloq::new(heap.as_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let mut chunks_allocated = [null_mut(); 512];
        for chunk in chunks_allocated.iter_mut() {
            *chunk = unsafe { alloqer.alloc(layout) };
        }

        for &mut chunk in chunks_allocated.iter_mut() {
            unsafe { alloqer.dealloc(chunk, layout) }
        }
    }
    //
    // #[test]
    // fn vec_grow() {
    //     let heap_stackish = [0u8; 512];
    //     let alloqer = Alloq::new(heap_stackish.as_ptr_range());
    //     let mut v = Vec::with_capacity_in(10, &alloqer);
    //     for x in 0..10 {
    //         v.push(x);
    //     }
    //     v.push(255);
    // }
    //
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
    //
    // #[test]
    // fn fragmented_heap() {
    //     let heap_stackish = [0u8; 1024 * 1024];
    //     let alloqer = Alloq::new(heap_stackish.as_ptr_range());
    //     let mut v: Vec<u8, _> = Vec::new_in(&alloqer);
    //     let mut w: Vec<u8, _> = Vec::new_in(&alloqer);
    //     for x in 0..128 {
    //         match x % 2 {
    //             0 => v.push(x),
    //             1 => w.push(x),
    //             _ => unreachable!(),
    //         }
    //     }
    //     assert!(v.iter().all(|i| i % 2 == 0));
    //     assert!(w.iter().all(|i| i % 2 == 1));
    // }
    //
    // #[test]
    // fn custom_structs() {
    //     struct S {
    //         _foo: i32,
    //         _bar: [u16; 8],
    //         _baz: &'static str,
    //     }
    //     let heap_stackish = [0u8; 512];
    //     let alloqer = Alloq::new(heap_stackish.as_ptr_range());
    //     let mut v = Vec::with_capacity_in(10, &alloqer);
    //     for x in 0..10 {
    //         let y = x as u16;
    //         let s = S {
    //             _foo: (x - 5) * 255,
    //             _bar: [
    //                 y * 8,
    //                 y * 8 + 1,
    //                 y * 8 + 2,
    //                 y * 8 + 3,
    //                 y * 8 + 4,
    //                 y * 8 + 5,
    //                 y * 8 + 6,
    //                 y * 8 + 7,
    //             ],
    //             _baz: "uga",
    //         };
    //         v.push(s)
    //     }
    // }
    //
    // #[test]
    // fn full_heap() {
    //     use core::mem::size_of;
    //     const VECTOR_SIZE: usize = 16;
    //     let heap_stackish = [0u8; (size_of::<<Alloq as Alloqator>::Metadata>()
    //         + size_of::<[u16; 32]>())
    //         * VECTOR_SIZE];
    //     let alloqer = Alloq::new(heap_stackish.as_ptr_range());
    //     let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
    //     for x in 0..VECTOR_SIZE {
    //         let ar: [u16; 32] = core::array::from_fn(|i| (i * x) as u16);
    //         v.push(ar);
    //     }
    // }
    //
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
