use core::ptr::null;

use spin::Mutex;

use crate::Alloqator;
extern crate std;

#[derive(Debug)]
pub struct Pool<const N: usize> {
    chunks: [*const u8; N],
    idx: usize,
}

impl<const N: usize> Pool<N> {
    pub fn new() -> Self {
        Self::from_chunks([null(); N])
    }

    pub fn from_chunks(chunks: [*const u8; N]) -> Self {
        Self { chunks, idx: 0 }
    }

    pub unsafe fn pop_left(&mut self) -> *const u8 {
        let ptr = self.chunks[self.idx];
        self.idx += 1;
        ptr
    }

    pub unsafe fn pop_right(&mut self) -> *const u8 {
        let ptr = self.chunks[self.idx];
        self.idx -= 1;
        ptr
    }

    pub unsafe fn push_left(&mut self, ptr: *const u8) {
        // TODO: Handle stack overflow
        self.chunks[self.idx] = ptr;
        self.idx += 1;
    }

    pub unsafe fn push_right(&mut self, ptr: *const u8) {
        // TODO: Handle stack overflow
        self.chunks[self.idx] = ptr;
        self.idx -= 1;
    }

    pub unsafe fn remove_swap_left(&mut self, idx: usize) {
        self.swap(idx, self.idx);
        self.idx += 1;
    }

    pub unsafe fn remove_swap_right(&mut self, idx: usize) {
        self.swap(idx, self.idx);
        self.chunks[self.idx] = null();
        if self.chunks[self.idx].is_null() {
            self.sort();
        }
        self.idx -= 1;
    }

    pub unsafe fn sort(&mut self) {
        self.chunks.sort_by(|a, b| b.cmp(a));
    }

    pub unsafe fn swap(&mut self, src: usize, dst: usize) {
        self.chunks.swap(src, dst)
    }
}

pub struct Alloq<const N: usize> {
    free: Mutex<Pool<N>>,
    used: Mutex<Pool<N>>,
    heap_start: *const u8,
    heap_end: *const u8,
}

impl<const N: usize> Alloqator for Alloq<N> {
    type Metadata = ();
    fn new(heap_range: core::ops::Range<*const u8>) -> Self
    where
        Self: Sized,
    {
        let offset = crate::align(heap_range.start as usize, N) as isize;
        let mut chunks = [null(); N];
        let mut index = offset as *const u8;
        for chunk in chunks.iter_mut() {
            if unsafe { index.offset(N as isize) } > heap_range.end {
                break;
            }
            *chunk = index;
            index = unsafe { index.offset(N as isize) };
        }
        Self {
            free: Mutex::new(Pool::from_chunks(chunks)),
            used: Mutex::new(Pool::new()),
            heap_start: heap_range.start,
            heap_end: heap_range.end,
        }
    }

    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut free_lock = self.free.lock();
        let mut used_lock = self.used.lock();
        if layout.size() > N {
            todo!("alloc multiple chunks");
        }
        let chunk = free_lock.pop_left();
        used_lock.push_left(chunk);
        crate::align(chunk as usize, layout.align()) as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        let mut free_lock = self.free.lock();
        let mut used_lock = self.used.lock();

        let idx = used_lock
            .chunks
            .iter()
            .position(|&p| p == ptr as *const u8)
            .expect("use after free");
        let freed = used_lock.chunks[idx];
        used_lock.remove_swap_right(idx);
        free_lock.push_right(freed);
    }
}

unsafe impl<const N: usize> core::alloc::Allocator for Alloq<N> {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        Alloqator::allocate(self, layout)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        Alloqator::deallocate(self, ptr, layout)
    }
}

unsafe impl<const N: usize> core::alloc::GlobalAlloc for Alloq<N> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        Alloqator::alloc(self, layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        Alloqator::dealloc(self, ptr, layout)
    }
}

#[cfg(test)]
pub mod tests {
    use super::Alloq;
    use crate::Alloqator;
    extern crate alloc;
    extern crate std;
    use alloc::boxed::Box;

    #[test]
    fn simple() {
        let heap_stackish = [0u8; 512];
        let alloqer: Alloq<32> = Alloq::new(heap_stackish.as_ptr_range());
        std::println!("mapped");
        std::println!("free pool: {:#?}", alloqer.free.lock());
        std::println!("used pool: {:#?}\n", alloqer.used.lock());
        let b = Box::new_in(31, &alloqer);
        std::println!("b allocated");
        std::println!("free pool: {:#?}", alloqer.free.lock());
        std::println!("used pool: {:#?}\n", alloqer.used.lock());
        let c = Box::new_in(32, &alloqer);
        std::println!("c allocated");
        std::println!("free pool: {:#?}", alloqer.free.lock());
        std::println!("used pool: {:#?}\n", alloqer.used.lock());
        drop(b);
        std::println!("b freed");
        std::println!("free pool: {:#?}", alloqer.free.lock());
        std::println!("used pool: {:#?}\n", alloqer.used.lock());
        std::assert_ne!(*c, 32 + 1);
        let d = Box::new_in(30, &alloqer);
        std::println!("d allocated");
        std::println!("free pool: {:#?}", alloqer.free.lock());
        std::println!("used pool: {:#?}\n", alloqer.used.lock());
        std::assert_ne!(*c, *d + 1);
        drop(d);
        drop(c);
        std::println!("everyone dropped");
        std::println!("free pool: {:#?}", alloqer.free.lock());
        std::println!("used pool: {:#?}\n", alloqer.used.lock());
    }
}
