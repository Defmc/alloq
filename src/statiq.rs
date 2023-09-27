use crate::Alloqator;

use spin::Mutex;

pub struct Alloq {
    heap_start: *const u8,
    heap_end: *const u8,
    end: Mutex<(/* left */ *const u8, /* right */ *const u8)>,
}

impl Alloq {
    pub unsafe fn r_alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut lock = self.end.lock();
        let ptr = lock.1.offset(-(layout.size() as isize));
        let ptr = crate::align_down(ptr as usize, layout.align()) as *mut u8;
        lock.1 = ptr;
        assert!(ptr as *const u8 >= lock.0, "no available memory");
        ptr
    }

    pub unsafe fn l_alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut lock = self.end.lock();
        let ptr = lock.0.offset(-(layout.size() as isize));
        let ptr = crate::align_down(ptr as usize, layout.align()) as *mut u8;
        lock.0 = ptr;
        assert!(ptr as *const u8 <= lock.1, "no available memory");
        ptr
    }
}

impl Alloqator for Alloq {
    type Metadata = ();

    fn new(heap_range: core::ops::Range<*const u8>) -> Self
    where
        Self: Sized,
    {
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            end: (heap_range.start, heap_range.end).into(),
        }
    }

    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }

    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }

    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.r_alloc(layout)
    }

    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) {}
}

crate::impl_allocator!(Alloq);

#[cfg(test)]
pub mod tests {
    use super::Alloq;

    include!("test.template.rs");
}
