use core::{mem, ops::Range, ptr::null_mut};

use crate::Alloqator;
use spin::Mutex;

pub const DEFAULT_CHUNK_SIZE: usize = 64;
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

    pub unsafe fn allocate<'a>(&self, end: &mut *const RawChunk, chunk_size: usize) -> *mut Self {
        let ptr = end.offset(-1);
        let aligned = crate::align_down(ptr as usize, mem::align_of::<Self>()) as *mut Self;
        assert!(
            self.chunk.offset(chunk_size as isize) < aligned as *const u8,
            "too low memory: can't allocate a chunk ({} bytes in {:?}) and metadata ({} bytes in {:?})",
            chunk_size, self.chunk, mem::size_of::<Self>(), aligned
        );
        debug_assert!(
            aligned.offset(1) as *const Self <= *end,
            "using an already allocated RawChunk area"
        );
        *aligned = self.clone();
        *end = aligned;
        aligned
    }

    #[inline(always)]
    pub fn next(mut self, next: *mut Self) -> Self {
        self.next = next;
        self
    }

    #[inline(always)]
    pub fn back(mut self, back: *mut Self) -> Self {
        self.back = back;
        self
    }

    pub unsafe fn alloc_next<'a>(
        &mut self,
        list_end: &mut *const RawChunk,
        chunk_size: usize,
        align: usize,
    ) -> *mut Self {
        let last_alloc = *list_end;
        let addr = (*last_alloc).chunk.offset(chunk_size as isize);
        let next = Self::new(addr, align /* already aligned*/).allocate(list_end, chunk_size);
        Self::connect_unchecked(self, &mut *next);
        next
    }

    #[inline(always)]
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

    #[inline(always)]
    pub unsafe fn insert_in_list(&mut self, back: &mut RawChunk) {
        self.disconnect();
        let next = back.next;
        Self::connect_unchecked(back, self);
        if !next.is_null() {
            Self::connect_unchecked(self, &mut *next);
        }
    }

    #[inline(always)]
    pub unsafe fn connect(back: &mut Self, next: &mut Self) {
        assert!(back.next.is_null());
        assert!(next.back.is_null());
        Self::connect_unchecked(back, next);
    }

    #[inline(always)]
    pub unsafe fn connect_unchecked(back: &mut Self, next: &mut Self) {
        back.next = next as *mut Self;
        next.back = back as *mut Self;
    }

    #[inline(always)]
    pub fn iter(&self) -> RawChunkIter {
        RawChunkIter((self as *const RawChunk) as *mut RawChunk)
    }

    pub fn first(&self) -> *const RawChunk {
        let mut first = self;
        while !first.back.is_null() {
            first = unsafe { &*first.back };
        }
        first
    }

    #[inline(always)]
    pub fn last(&self) -> *const RawChunk {
        self.iter().last().unwrap()
    }

    pub fn log_list(&self, w: &mut impl core::fmt::Write) {
        for node in unsafe { (*self.first()).iter() } {
            write!(w, "({node:?}) .:. {:?} -> ", unsafe { &*node }).unwrap();
        }
        writeln!(w, "{:?}", null_mut::<Self>()).unwrap();
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
/// An fixed-size allocator with a pool memory managment. Using a native reverse link-list, its map
/// the block and use two lists to cache them.
#[derive(Debug)]
pub struct Alloq {
    heap_start: *const u8,
    heap_end: *const u8,
    chunk_size: usize,
    align: usize,
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
            debug_assert!(last.next.is_null(), "`used_last` is not on top");
            RawChunk::connect(last, &mut *raw_chunk);
            self.used_last = raw_chunk;
        }
    }

    pub unsafe fn get_free_chunk(&mut self, chunk_size: usize, align: usize) -> *mut RawChunk {
        let last = &mut *self.free_last;
        let freed = if last.back.is_null() {
            &mut *last.alloc_next(&mut self.list_end, chunk_size, align)
        } else {
            &mut *last.back
        };
        freed.disconnect();
        freed
    }

    pub unsafe fn remove_used(&mut self, addr: *const u8) {
        let last_free = &mut *self.free_last;
        for raw_chunk_ptr in (*self.used_head).iter() {
            let raw_chunk = &mut *(raw_chunk_ptr);
            if (*raw_chunk).addr == addr {
                if self.used_head == raw_chunk_ptr {
                    self.used_head = raw_chunk.next;
                }
                if self.used_last == raw_chunk_ptr {
                    self.used_last = raw_chunk.back;
                }
                raw_chunk.insert_in_list(last_free);
                self.free_last = raw_chunk_ptr;
                return;
            }
        }
        panic!("double-free on {addr:?}");
    }
}

impl Alloq {
    pub unsafe fn with_chunk_size(
        heap_range: Range<*const u8>,
        chunk_size: usize,
        align: usize,
    ) -> Self {
        let mut end = heap_range.end as *const RawChunk;
        let free_last = RawChunk::new(heap_range.start, align).allocate(&mut end, chunk_size);
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            chunk_size,
            align,
            pooler: Pool {
                used_head: null_mut(),
                free_last,
                used_last: null_mut(),
                list_end: end,
            }
            .into(),
        }
    }
}

// Why rustfmt is removing comments?
// impl /*Alloqator for*/ Pool {
impl Alloqator for Alloq {
    type Metadata = RawChunk;

    fn new(heap_range: Range<*const u8>) -> Self {
        unsafe { Self::with_chunk_size(heap_range, DEFAULT_CHUNK_SIZE, DEFAULT_ALIGNMENT) }
    }

    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    /// Pass pre-allocated block and add its to the used list. If there's no available blocks, map
    /// one.
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let chunk = {
            let mut pooler = self.pooler.lock();
            let chunk = pooler.get_free_chunk(self.chunk_size, layout.align());
            pooler.push_used(chunk);
            if (*chunk).addr.offset(layout.size() as isize)
                > (*chunk).chunk.offset(self.chunk_size as isize)
            {
                todo!(
                "layout (size {} bytes and align {} bytes) cannot be allocated in a chunk ({} bytes)",
                layout.size(),
                layout.align(),
                self.chunk_size
            );
            }
            debug_assert!(self.heap_range().contains(&(*chunk).chunk), "out of heap");
            chunk
        };
        (*chunk).addr as *mut u8
    }

    /// Moves the block to the free list
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        self.pooler.lock().remove_used(ptr);
    }

    fn reset(&self) {
        let mut pooler = self.pooler.lock();
        let mut end = self.heap_end() as *const RawChunk;
        let free_last = unsafe {
            RawChunk::new(self.heap_start(), self.align).allocate(&mut end, self.chunk_size)
        };
        pooler.used_head = null_mut();
        pooler.used_last = null_mut();
        pooler.free_last = free_last;
        pooler.list_end = end;
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
    use core::{alloc::Layout, mem::MaybeUninit, ptr::null_mut};
    use std::thread;

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
        let mut chunks_allocated = [null_mut(); 4];
        for chunk in chunks_allocated.iter_mut() {
            *chunk = unsafe { alloqer.alloc(layout) };
        }

        for &mut chunk in chunks_allocated.iter_mut() {
            unsafe { alloqer.dealloc(chunk, layout) }
        }
    }

    #[test]
    fn stack_allocs() {
        let heap = [0u8; 512 * 512];
        let alloqer = Alloq::new(heap.as_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let mut chunks_allocated = [null_mut(); 4];
        for chunk in chunks_allocated.iter_mut() {
            *chunk = unsafe { alloqer.alloc(layout) };
        }

        for &mut chunk in chunks_allocated.iter_mut().rev() {
            unsafe { alloqer.dealloc(chunk, layout) }
        }
    }

    #[test]
    fn multithread_allocs() {
        static mut ALLOQER: MaybeUninit<Alloq> = MaybeUninit::uninit();
        let heap = [0u8; 1024];
        unsafe {
            ALLOQER = MaybeUninit::new(Alloq::new(heap.as_ptr_range()));
        }
        let layout = Layout::from_size_align(32, 2).unwrap();
        let thread = thread::spawn(|| {
            let layout = Layout::from_size_align(32, 2).unwrap();
            for _ in 0..100 {
                let ptr = unsafe { ALLOQER.assume_init_mut().alloc(layout) };
                unsafe { ALLOQER.assume_init_mut().dealloc(ptr, layout) };
            }
        });
        for _ in 0..100 {
            let ptr = unsafe { ALLOQER.assume_init_mut().alloc(layout) };
            unsafe { ALLOQER.assume_init_mut().dealloc(ptr, layout) };
        }
        thread.join().unwrap();
    }

    #[test]
    fn vec_grow() {
        let heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_ptr_range());
        let mut v = Vec::new_in(&alloqer);
        for x in 0..10 {
            v.push(x);
        }
        assert_eq!(v.iter().sum::<i32>(), 45i32);
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
        let heap_stackish = [0u8; crate::get_size_hint_in::<S, Alloq>(10) * 2];
        let alloqer = unsafe { Alloq::with_chunk_size(heap_stackish.as_ptr_range(), 512, 2) };
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
        const VECTOR_SIZE: usize = 16;
        let heap_stackish = [0u8; crate::get_size_hint_in::<[u16; 32], Alloq>(VECTOR_SIZE) * 2];
        let alloqer = unsafe { Alloq::with_chunk_size(heap_stackish.as_ptr_range(), 1024, 2) };
        let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        for x in 0..VECTOR_SIZE {
            let ar: [u16; 32] = core::array::from_fn(|i| (i * x) as u16);
            v.push(ar);
        }
    }

    #[test]
    fn zero_sized() {
        const VECTOR_SIZE: usize = 1024;
        // FIXME: 32768 bytes for ZST? Really?
        let heap_stackish = [0u8; crate::get_size_hint_in::<(), Alloq>(VECTOR_SIZE)];
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
