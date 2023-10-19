use core::{
    alloc::{AllocError, Allocator, Layout},
    mem,
    ops::Range,
    ptr::{null_mut, NonNull},
};
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
/// A Deallocation-able Bump allocator. Works like `crate::bump::Alloq`, but has several mechanisms
/// to deallocate in a stack-ish allocator
pub struct Alloq {
    pub heap_start: *const u8,
    pub iter: Mutex<(usize, *mut u8, *const u8)>,
    pub heap_end: *const u8,
}

unsafe impl Allocator for Alloq {
    /// Similar to `crate::bump::Bump::alloc` (O(1) so), but also allocates a `AlloqMetaData` in the top of
    /// stack, containing where is the block, where is the last `AlloqMetaData` allocated and if
    /// it's being used
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut lock = self.iter.lock();
        let block_start = lock.1;
        let start = crate::align_up(lock.1 as usize, layout.align()) as *mut u8;
        let end = unsafe { start.offset(layout.size() as isize) };
        let start_meta = crate::align_up(end as usize, mem::align_of::<AlloqMetaData>()) as *mut u8;
        let end_meta = unsafe { start_meta.offset(mem::size_of::<AlloqMetaData>() as isize) };
        if end_meta > self.heap_end as *mut u8 {
            panic!("no available memory")
        }
        lock.1 = end_meta;
        lock.0 += 1;
        let (md, last_meta) =
            unsafe { AlloqMetaData::new(block_start, lock.2).write_meta(start, end_meta) };
        lock.2 = last_meta;
        let slice = unsafe { core::slice::from_raw_parts_mut(md, layout.size()) };
        NonNull::new(slice).ok_or(AllocError)
    }

    /// Set the `ptr` metadata as unused. If it's on the top of stack, starts to deallocate (return the
    /// stack pointer to `last_meta`) all the
    /// last areas marked as unused
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let mut lock = *self.iter.lock();
        lock.0 -= 1;
        if lock.0 == 0 {
            lock.1 = self.heap_start as *mut u8;
            return;
        }
        let mut meta = AlloqMetaData::from_alloc_ptr(ptr.as_ptr(), layout);
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

    include!("test.template.rs");
}
