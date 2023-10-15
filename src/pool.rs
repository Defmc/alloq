use core::{mem, ops::Range, ptr::null_mut};

extern crate std;

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
        let alignment = (end as usize) % mem::size_of::<Self>();
        let ptr = end.offset(-(mem::size_of::<Self>() as isize));
        let ptr = ptr as *mut Self;
        *ptr = self.clone();
        ptr
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
        std::println!("setting {list_end:?} -> {next:?}");
        *list_end = next;
        Self::connect_unchecked(self, &mut *next);
        next
    }

    pub unsafe fn disconnect(&mut self) {
        let next = self.next;
        let back = self.back;
        std::println!("\t\tnext: {:?}", next);
        std::println!("\t\tback: {:?}", back);
        if !self.back.is_null() {
            (*back).next = next;
        }
        if !self.next.is_null() {
            (*next).back = back;
        }
        self.next = null_mut();
        self.back = null_mut();
        std::println!("\t\treturning");
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
        back.next = next as *mut Self;
        next.back = back as *mut Self;
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
pub struct Pool {
    heap_start: *const u8,
    heap_end: *const u8,
    used_head: *mut RawChunk,
    free_last: *mut RawChunk,
    used_last: *mut RawChunk,
    chunk_size: usize,
    list_end: *const RawChunk,
}

impl Pool {
    pub unsafe fn with_chunk_size(
        heap_range: Range<*const u8>,
        chunk_size: usize,
        align: usize,
    ) -> Self {
        let free_last = RawChunk::new(heap_range.start, align).allocate(heap_range.end as *mut u8);
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            used_head: null_mut(),
            free_last,
            used_last: null_mut(),
            chunk_size,
            list_end: free_last,
        }
    }

    pub unsafe fn get_free_chunk(&mut self) -> *mut RawChunk {
        let last = &mut *self.free_last;
        std::println!("\tfree_last: {:?} -> {last:?}", last as *mut RawChunk);
        let freed = if last.next.is_null() {
            std::println!("\tallocating raw chunk");
            &mut *last.alloc_next(&mut self.list_end, self.chunk_size)
        } else {
            std::println!("\tusing previous raw chunk");
            &mut *last.next
        };
        std::println!("\tfreed: {:?} -> {freed:?}", freed as *mut RawChunk);
        std::println!("\tdisconnecting freed");
        freed.disconnect();
        std::println!("\treturning freed: {freed:?}");
        freed
    }

    pub unsafe fn push_used(&mut self, raw_chunk: *mut RawChunk) {
        if self.used_head.is_null() {
            self.used_head = raw_chunk;
        }
        if self.used_last.is_null() {
            self.used_last = raw_chunk;
        } else {
            let last = &mut *self.used_last;
            std::println!("setting used last: {:?} -> {last:?}", self.used_last);
            if !last.next.is_null() {
                panic!("there's a `next` in `used_last`");
            }
            RawChunk::connect(last, &mut *raw_chunk);
            std::println!("raw chunk: {raw_chunk:?} -> {:?}", *raw_chunk);
            self.used_last = raw_chunk;
        }
    }

    pub unsafe fn remove_used(&mut self, addr: *const u8) {
        let last_free = &mut *self.free_last;
        std::println!("\tlast_free: {last_free:?}");
        for raw_chunk in (*self.used_head).iter() {
            std::println!("\traw_chunk: {:?}", *raw_chunk);
            let raw_chunk = &mut *(raw_chunk);
            if raw_chunk.addr == addr {
                std::println!("\t\tfounded");
                if self.used_head == raw_chunk {
                    std::println!("setting used_head to {:?}", raw_chunk.next);
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

// Why rustfmt is removing comments?
// impl /*Alloqator for*/ Pool {
impl Pool {
    //type Metadata = ();
    pub fn new(heap_range: Range<*const u8>) -> Self {
        unsafe { Self::with_chunk_size(heap_range, 512, 2) }
    }

    pub fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    pub fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    pub unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> *mut u8 {
        std::println!("\ngetting free chunk");
        let chunk = self.get_free_chunk();
        std::println!("pushing on used_last");
        self.push_used(chunk);
        std::println!("pushed {:?}", *chunk);
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

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, _layout: core::alloc::Layout) {
        self.remove_used(ptr);
    }

    // TODO: Improve shrink and grow by simply link another pointer
}

#[cfg(test)]
pub mod tests {
    extern crate alloc;
    extern crate std;
    use crate::pool::RawChunk;

    use super::Pool;
    use core::{alloc::Layout, ptr::null_mut};

    #[test]
    fn simple_alloc() {
        std::println!("creating heap");
        let heap = [0u8; 512 * 8];
        std::println!("creating alloqator");
        let mut alloqer = Pool::new(heap.as_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        std::println!("alloqating");
        let ptr = unsafe { alloqer.alloc(layout) };
        std::println!("alloq state: {alloqer:?}");
        std::println!("dealloqating");
        unsafe { alloqer.dealloc(ptr, layout) };
        std::println!("alloq state: {alloqer:?}");
    }

    #[test]
    fn some_allocs() {
        std::println!("creating heap");
        let heap = [0u8; 512 * 512];
        std::println!("creating alloqator");
        let mut alloqer = Pool::new(heap.as_ptr_range());
        let layout = Layout::from_size_align(32, 2).unwrap();
        let mut chunks_allocated = [null_mut(); 3];
        std::println!("alloqating");
        for chunk in chunks_allocated.iter_mut() {
            *chunk = unsafe { alloqer.alloc(layout) };
        }

        std::println!("alloq's used list:");
        for node in unsafe { (*alloqer.used_head).iter() } {
            std::println!("{node:?} -> ");
        }
        std::println!("{:?}", null_mut::<RawChunk>());

        std::println!("dealloqating");
        for &mut chunk in chunks_allocated.iter_mut() {
            unsafe { alloqer.dealloc(chunk, layout) }
        }
        std::println!("alloq state: {alloqer:?}");
    }
}
