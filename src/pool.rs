use core::{
    alloc::{AllocError, Allocator},
    mem,
    ops::Range,
    ptr::{null_mut, NonNull},
};

use crate::Alloqator;
use spin::Mutex;

pub const DEFAULT_CHUNK_SIZE: usize = 64;
pub const DEFAULT_ALIGNMENT: usize = 2;

#[derive(Clone, Debug)]
pub struct RawChunk {
    pub addr: *mut u8,
    pub chunk: *const u8,
    /// When freed, they point to the next free block. When allocated, to the continuous part.
    pub next: *mut Self,
    pub back: *mut Self,
}

impl RawChunk {
    /// # Safety
    /// `bind` must refer to a valid chunk.
    pub unsafe fn new(bind: *const u8, align: usize) -> Self {
        Self {
            addr: crate::align_up(bind as usize, align) as *mut u8,
            chunk: bind,
            next: null_mut(),
            back: null_mut(),
        }
    }

    /// # Safety
    /// `end` must be a valid raw chunk and previous allocated.
    pub unsafe fn allocate(&self, end: &mut *mut RawChunk, chunk_size: usize) -> *mut Self {
        let ptr = end.sub(1);
        let aligned = crate::align_down(ptr as usize, mem::align_of::<Self>()) as *mut Self;
        assert!(
            self.chunk.add(chunk_size) < aligned.cast(),
            "too low memory: can't allocate a chunk ({} bytes in {:?}) and metadata ({} bytes in {:?})",
            chunk_size, self.chunk, mem::size_of::<Self>(), aligned
        );
        debug_assert!(
            aligned.offset(1).cast_const() <= *end,
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

    pub fn alloc_next(
        &mut self,
        list_end: &mut *mut RawChunk,
        chunk_size: usize,
        align: usize,
    ) -> *mut Self {
        let last_alloc = *list_end;
        unsafe {
            let addr = (*last_alloc).chunk.add(chunk_size);
            let next = Self::new(addr, align /* already aligned*/).allocate(list_end, chunk_size);
            Self::connect_unchecked(self, &mut *next);
            next
        }
    }

    #[inline(always)]
    pub fn disconnect(&mut self) {
        let next = self.next;
        let back = self.back;
        if !self.back.is_null() {
            unsafe {
                (*back).next = next;
            }
        }
        if !self.next.is_null() {
            unsafe {
                (*next).back = back;
            }
        }
        self.next = null_mut();
        self.back = null_mut();
    }

    #[inline(always)]
    pub fn insert_in_list(&mut self, back: &mut RawChunk) {
        self.disconnect();
        let next = back.next;
        Self::connect_unchecked(back, self);
        if !next.is_null() {
            unsafe {
                Self::connect_unchecked(self, &mut *next);
            }
        }
    }

    #[inline(always)]
    pub fn connect(back: &mut Self, next: &mut Self) {
        assert!(back.next.is_null());
        assert!(next.back.is_null());
        Self::connect_unchecked(back, next);
    }

    #[inline(always)]
    pub fn connect_unchecked(back: &mut Self, next: &mut Self) {
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

    pub fn log_list(&self) {
        for _node in unsafe { (*self.first()).iter() } {
            //    std::print!("({node:?}) .:. {:?} -> ", unsafe { &*node });
        }
        //std::println!("{:?}", null_mut::<Self>());
    }

    pub fn sort<'a>(&'a mut self) -> &'a mut Self {
        // TODO: To avoid a `first` call, merge reverse.
        // Divide it into a tree:
        // e c b d a == MERGE(e, c, MERGE(b, d, MERGE(a, -, -)))
        // e c
        //     b d
        //         a
        let mut it = self.iter();
        let l = it.next().unwrap();
        if let Some(r) = it.next() {
            unsafe {
                (*l).disconnect();
                (*r).disconnect();
                let merged = Self::merge(&mut *l, &mut *r);
                if let Some(n) = it.next() {
                    let f = (*n).sort();
                    Self::merge(merged, f)
                } else {
                    merged
                }
            }
        } else {
            unsafe { &mut *l }
        }
    }

    pub unsafe fn merge<'a>(l: &'a mut Self, r: &'a mut Self) -> &'a mut Self {
        let mut lit = l.iter().peekable();
        let mut rit = r.iter().peekable();
        let mut list = null_mut::<Self>();
        let mut put_it = |node| {
            if list.is_null() {
                list = node;
            } else {
                Self::connect_unchecked(&mut *list, &mut *node);
                list = node;
            }
        };
        loop {
            match (lit.peek(), rit.peek()) {
                (None, None) => break,
                (Some(_), None) => put_it(lit.next().unwrap()),
                (None, Some(_)) => put_it(rit.next().unwrap()),
                (Some(&le), Some(&re)) => {
                    if (*le).chunk < (*re).chunk {
                        put_it(lit.next().unwrap())
                    } else {
                        put_it(rit.next().unwrap())
                    }
                }
            }
        }
        &mut *list
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
    heap_start: *mut u8,
    heap_end: *mut u8,
    chunk_size: usize,
    align: usize,
    pooler: Mutex<Pool>,
}

#[derive(Debug)]
pub struct Pool {
    free_last: *mut RawChunk,
    list_end: *mut RawChunk,
}

impl Pool {
    /// # Safety
    /// `raw_chunk` must be a valid and previous allocated raw chunk.
    pub unsafe fn push_used(&mut self, _raw_chunk: *mut RawChunk) {}

    pub fn get_free_chunk(&mut self, chunk_size: usize, align: usize) -> *mut RawChunk {
        let last = unsafe { &mut *self.free_last };
        let freed = if last.back.is_null() {
            unsafe { &mut *last.alloc_next(&mut self.list_end, chunk_size, align) }
        } else {
            unsafe { &mut *last.back }
        };
        freed.disconnect();
        freed
    }

    /// # Safety
    /// `ptr` must be previous returned by `Alloq::allocate`.
    pub unsafe fn remove_used(&mut self, raw_chunk_ptr: *mut RawChunk) {
        let last_free = &mut *self.free_last;
        let raw_chunk = &mut *raw_chunk_ptr;
        raw_chunk.insert_in_list(last_free);
        self.free_last = raw_chunk_ptr;
    }
}

impl Alloq {
    /// # Safety
    /// `heap_range` must be a valid heap block.
    pub unsafe fn with_chunk_size(
        heap_range: Range<*mut u8>,
        chunk_size: usize,
        align: usize,
    ) -> Self {
        // SAFE: Its will not be even used as a `RawChunk`.
        let mut end = heap_range.end as *mut RawChunk;
        let free_last = RawChunk::new(heap_range.start, align).allocate(&mut end, chunk_size);
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            chunk_size,
            align,
            pooler: Pool {
                free_last,
                list_end: end,
            }
            .into(),
        }
    }

    /// Return the `RawChunk` that stores that chunk.
    /// # Safety
    /// `ptr` must be previous returned by `Alloq::allocate`.
    pub unsafe fn get_raw_chunk_from(
        &self,
        ptr: *const u8,
        _layout: core::alloc::Layout,
    ) -> *const RawChunk {
        // SOUND: `ptr` is always `crate::align_up`, which garantees `*ptr` > `*chunk`
        let chunk = crate::align_down(ptr as usize, self.align);
        let first_chunk = crate::align_up(self.heap_start() as usize, self.align);
        let chunk_idx = (chunk - first_chunk) / self.chunk_size;
        let first_raw = crate::align_down(
            self.heap_end().sub(mem::size_of::<RawChunk>()) as usize,
            mem::align_of::<RawChunk>(),
        ) as *const RawChunk;
        // FIXME: Is size always multiple of alignment?
        let raw = first_raw.offset(-(chunk_idx as isize));
        debug_assert_eq!(
            ptr,
            (*raw).addr,
            "invalid `ptr`: can't find a correspondent chunk"
        );
        raw
    }
}

unsafe impl Allocator for Alloq {
    /// Pass pre-allocated block and add its to the used list. If there's no available blocks, map
    /// one.
    fn allocate(&self, layout: core::alloc::Layout) -> Result<NonNull<[u8]>, AllocError> {
        let chunk = {
            let mut pooler = self.pooler.lock();
            let chunk = pooler.get_free_chunk(self.chunk_size, layout.align());
            unsafe { pooler.push_used(chunk) };
            if unsafe {
                (*chunk).addr.add(layout.size()) > (*chunk).chunk.add(self.chunk_size).cast_mut()
            } {
                todo!(
                "layout (size {} bytes and align {} bytes) cannot be allocated in a chunk ({} bytes)",
                layout.size(),
                layout.align(),
                self.chunk_size
            );
            }
            debug_assert!(
                self.heap_range()
                    .contains(unsafe { &(*chunk).chunk.cast_mut() }),
                "out of heap"
            );
            chunk
        };
        let ptr = unsafe { (*chunk).addr };
        let slice = unsafe {
            core::slice::from_raw_parts_mut(
                ptr,
                self.chunk_size - (*chunk).addr.offset(-((*chunk).chunk as isize)) as usize,
            )
        };
        NonNull::new(slice).ok_or(AllocError)
    }

    /// Moves the block to the free list. In these newer versions, deallocating is `O(1)`, as the
    /// block is on a constant place.
    /// Unsafe:
    /// - It's Undefined Behaviour to double-free a value. It can enter twice in the `used` stack
    ///   and be shared across two objects them.
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: core::alloc::Layout) {
        unsafe {
            let raw_chunk = self.get_raw_chunk_from(ptr.as_ptr(), layout);
            self.pooler.lock().remove_used(raw_chunk as *mut RawChunk);
        }
    }
}

// Why rustfmt is removing comments?
// impl /*Alloqator for*/ Pool {
impl Alloqator for Alloq {
    type Metadata = RawChunk;

    fn new(heap_range: Range<*mut u8>) -> Self {
        unsafe { Self::with_chunk_size(heap_range, DEFAULT_CHUNK_SIZE, DEFAULT_ALIGNMENT) }
    }

    fn heap_start(&self) -> *mut u8 {
        self.heap_start
    }

    fn heap_end(&self) -> *mut u8 {
        self.heap_end
    }

    unsafe fn reset(&self) {
        let mut pooler = self.pooler.lock();
        // SAFE: Its will not be even used as a `RawChunk`.
        let mut end = self.heap_end() as *mut RawChunk;
        let free_last = unsafe {
            RawChunk::new(self.heap_start(), self.align).allocate(&mut end, self.chunk_size)
        };
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
        let mut heap = [0u8; 512 * 8];
        let alloqer = Alloq::new(heap.as_mut_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let ptr = alloqer.alloq(layout);
        unsafe { alloqer.dealloq(ptr, layout) };
    }

    #[test]
    fn linear_allocs() {
        let mut heap = [0u8; 512 * 512];
        let alloqer = Alloq::new(heap.as_mut_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let mut chunks_allocated = [null_mut(); 4];
        for chunk in chunks_allocated.iter_mut() {
            *chunk = alloqer.alloq(layout);
        }

        for &mut chunk in chunks_allocated.iter_mut() {
            unsafe { alloqer.dealloq(chunk, layout) }
        }
    }

    #[test]
    fn stack_allocs() {
        let mut heap = [0u8; 512 * 512];
        let alloqer = Alloq::new(heap.as_mut_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let mut chunks_allocated = [null_mut(); 4];
        for chunk in chunks_allocated.iter_mut() {
            *chunk = alloqer.alloq(layout);
        }

        for &mut chunk in chunks_allocated.iter_mut().rev() {
            unsafe { alloqer.dealloq(chunk, layout) }
        }
    }

    #[test]
    fn multithread_allocs() {
        static mut ALLOQER: MaybeUninit<Alloq> = MaybeUninit::uninit();
        let mut heap = [0u8; 1024];
        unsafe {
            ALLOQER = MaybeUninit::new(Alloq::new(heap.as_mut_ptr_range()));
        }
        let layout = Layout::from_size_align(32, 2).unwrap();
        let thread = thread::spawn(|| {
            let layout = Layout::from_size_align(32, 2).unwrap();
            for _ in 0..100 {
                let ptr = unsafe { ALLOQER.assume_init_mut().alloq(layout) };
                unsafe { ALLOQER.assume_init_mut().dealloq(ptr, layout) };
            }
        });
        for _ in 0..100 {
            let ptr = unsafe { ALLOQER.assume_init_mut().alloq(layout) };
            unsafe { ALLOQER.assume_init_mut().dealloq(ptr, layout) };
        }
        thread.join().unwrap();
    }

    #[test]
    fn vec_grow() {
        let mut heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
        let mut v = Vec::new_in(&alloqer);
        for x in 0..10 {
            v.push(x);
        }
        assert_eq!(v.iter().sum::<i32>(), 45i32);
    }

    #[test]
    fn boxed() {
        let mut heap_stackish = [0u8; 512];
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
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
        let mut heap_stackish = [0u8; 1024 * 1024];
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
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
        let mut heap_stackish = [0u8; crate::get_size_hint_in::<S, Alloq>(10) * 2];
        let alloqer = unsafe { Alloq::with_chunk_size(heap_stackish.as_mut_ptr_range(), 512, 2) };
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
        let mut heap_stackish = [0u8; crate::get_size_hint_in::<[u16; 32], Alloq>(VECTOR_SIZE) * 2];
        let alloqer = unsafe { Alloq::with_chunk_size(heap_stackish.as_mut_ptr_range(), 1024, 2) };
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
        let mut heap_stackish = [0u8; crate::get_size_hint_in::<(), Alloq>(VECTOR_SIZE)];
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
        let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        for _ in 0..VECTOR_SIZE {
            v.push(());
        }
        let b = Box::new_in((), &alloqer);
        assert_eq!(*b, ());
        assert_eq!(v.len(), VECTOR_SIZE);
    }

    #[test]
    fn vector_fragmented() {
        const VECTOR_SIZE: usize = 128;
        let mut heap_stackish = [0u8; 1024 * 1024];
        let alloqer = unsafe { Alloq::with_chunk_size(heap_stackish.as_mut_ptr_range(), 1024, 2) };
        let mut v1 = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        let mut v2 = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        let mut v3 = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
        for x in 0..VECTOR_SIZE as isize {
            if x % 2 == 0 {
                v2.push(x);
            } else {
                v1.push(x);
            }
            v3.push(-x);
        }
        assert!(v1.iter().all(|x| x % 2 == 1));
        assert!(v2.iter().all(|x| x % 2 == 0));
        assert_eq!(
            v1.iter().chain(v2.iter()).sum::<isize>(),
            -v3.iter().sum::<isize>()
        )
    }

    #[test]
    fn trash_heap() {
        let mut heap_stackish: [u8; 1024] = core::array::from_fn(|x| (x % 255) as u8);
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
        let mut v = Vec::new_in(&alloqer);
        for x in 0..10 {
            v.push(x);
        }
        assert_eq!(v.iter().sum::<i32>(), 45i32);
    }

    #[test]
    fn corrupted_heap() {
        let mut heap_stackish: [u8; 1024] = [0; 1024];
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
        heap_stackish.fill(0);
        unsafe { alloqer.reset() };
        let mut v = Vec::new_in(&alloqer);
        for x in 0..10 {
            v.push(x);
        }
        assert_eq!(v.iter().sum::<i32>(), 45i32);
    }

    #[test]
    fn sort() {
        let mut heap_stackish = [0; 1024];
        let alloqer = Alloq::new(heap_stackish.as_mut_ptr_range());
        let vec: Vec<_> = (0..5).map(|_| alloqer.alloq(Layout::new::<()>())).collect();
        for p in vec {
            unsafe { alloqer.dealloq(p, Layout::new::<()>()) }
        }
        unsafe {
            let first = {
                let lock = alloqer.pooler.lock();
                (*lock.free_last).first()
            };
            let first = &mut *(first as *mut super::RawChunk);
            let s = first.sort();
            let mut last: *mut super::RawChunk = core::ptr::null_mut();
            for p in s.iter() {
                assert!(last.is_null() || (*last).chunk < (*p).chunk);
                last = p;
            }
        }
    }
}
