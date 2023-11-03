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
    pub start: Option<NonNull<u8>>,
    pub last_meta: *const AlloqMetaData,
}

impl AlloqMetaData {
    pub fn new(start: *mut u8, last_meta: *const AlloqMetaData) -> Self {
        Self {
            start: NonNull::new(start),
            last_meta,
        }
    }

    /// # Safety
    pub unsafe fn write_meta(&self, layout: Layout) -> *mut AlloqMetaData {
        let addr = crate::align_up(
            self.start.unwrap().as_ptr().add(layout.size()) as usize,
            mem::align_of::<Self>(),
        ) as *mut Self;
        (*addr) = *self;
        addr
    }

    /// # Safety
    /// `ptr` must be valid and previous allocated.
    pub unsafe fn from_alloc_ptr<'a>(ptr: *const u8, layout: Layout) -> &'a mut Self {
        let end = ptr.add(layout.size());
        let start_meta = crate::align_up(end as usize, mem::align_of::<AlloqMetaData>()) as *mut u8;
        &mut *(start_meta as *mut Self)
    }
}
/// A Deallocation-able Bump allocator. Works like `crate::bump::Alloq`, but has several mechanisms
/// to deallocate in a stack-ish allocator
pub struct Alloq {
    pub heap_start: *mut u8,
    pub heap_end: *mut u8,
    pub last_meta: Mutex<*const AlloqMetaData>,
}

impl Alloq {
    pub fn pad_alloc(heap_range: Range<*mut u8>) -> *const AlloqMetaData {
        let layout = Layout::new::<()>();
        let aligned = crate::align_up(heap_range.start as usize, layout.align()) as *mut u8;
        let end = unsafe { AlloqMetaData::new(aligned, null_mut()).write_meta(layout) };
        unsafe {
            assert!(
                heap_range.contains(&end.add(1).cast()),
                "debump: can't allocate any blocks. heap_size: {heap_range:?} ({} bytes)",
                heap_range.end.offset_from(heap_range.start) as usize
            );
        }
        end
    }
}

unsafe impl Allocator for Alloq {
    /// Similar to `crate::bump::Bump::alloc` (O(1) so), but also allocates a `AlloqMetaData` in the top of
    /// stack, containing where is the block, where is the last `AlloqMetaData` allocated and if
    /// it's being used
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut last_meta = self.last_meta.lock();
        let ptr = unsafe {
            let end = last_meta.add(1);
            let obj_addr = crate::align_up(end as usize, layout.align()) as *mut u8;
            *last_meta = AlloqMetaData::new(obj_addr, *last_meta).write_meta(layout);
            debug_assert!(
                self.heap_range().contains(&(last_meta.add(1) as *mut u8)),
                "can't allocate a new layout. heap range {:?} ({} bytes)",
                self.heap_range(),
                self.heap_end.offset_from(self.heap_start) as usize
            );
            (**last_meta).start.unwrap().as_ptr()
        };
        let slice = unsafe { core::slice::from_raw_parts_mut(ptr, layout.size()) };
        NonNull::new(slice).ok_or(AllocError)
    }

    /// Set the `ptr` metadata as unused. If it's on the top of stack, starts to deallocate (return the
    /// stack pointer to `last_meta`) all the
    /// last areas marked as unused
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let mut last_meta = self.last_meta.lock();
        let meta = AlloqMetaData::from_alloc_ptr(ptr.as_ptr(), layout);
        meta.start = None;
        if meta as *const AlloqMetaData == *last_meta {
            while (**last_meta).start.is_none() {
                *last_meta = (**last_meta).last_meta;
            }
        }
    }
}

impl Alloqator for Alloq {
    type Metadata = AlloqMetaData;

    fn new(heap_range: Range<*mut u8>) -> Self {
        Self {
            heap_start: heap_range.start,
            last_meta: Mutex::new(Self::pad_alloc(heap_range.clone())),
            heap_end: heap_range.end,
        }
    }

    #[inline(always)]
    fn heap_start(&self) -> *mut u8 {
        self.heap_start
    }

    #[inline(always)]
    fn heap_end(&self) -> *mut u8 {
        self.heap_end
    }

    #[inline(always)]
    unsafe fn reset(&self) {
        let mut lock = self.last_meta.lock();
        *lock = Self::pad_alloc(self.heap_range());
    }
}

crate::impl_allocator!(Alloq);

#[cfg(test)]
pub mod tests {
    use super::Alloq;

    include!("test.template.rs");
}
