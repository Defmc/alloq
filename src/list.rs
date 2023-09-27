use core::{
    alloc::{AllocError, Allocator, Layout},
    marker::PhantomData,
    mem,
    ops::Range,
    ptr::{self, NonNull},
    slice,
};

use crate::Alloqator;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlloqMetaData {
    pub end: *const u8,
    pub next: *mut Self,
    pub back: *mut Self,
}

impl AlloqMetaData {
    pub unsafe fn allocate(
        list: *mut Self,
        range: Range<*const u8>,
        layout: Layout,
    ) -> (Self, *mut Self) {
        let range_start = if list.is_null() {
            range.start
        } else {
            (*list).end
        };
        let aligned_meta =
            crate::align_up(range_start as usize, mem::align_of::<Self>()) as *mut u8;
        let aligned_val = crate::align_up(
            aligned_meta.offset(mem::size_of::<Self>() as isize) as usize,
            layout.align(),
        ) as *mut u8;
        let mut s = Self {
            end: aligned_val.offset(layout.size() as isize),
            next: ptr::null_mut(),
            back: ptr::null_mut(),
        };
        assert!(
            aligned_val.offset(layout.size() as isize) as *const u8 <= range.end,
            "no available memory"
        );
        if !list.is_null() {
            Self::connect_unchecked(&mut (*list), &mut s);
        }
        (s, s.write(aligned_meta) as *mut Self)
    }
    pub unsafe fn write(&self, ptr: *mut u8) -> *mut u8 {
        *(ptr as *mut Self) = *self;
        ptr as *mut u8
    }

    pub fn disconnect(&mut self) {
        let next = self.next;
        let back = self.back;
        if !self.back.is_null() {
            unsafe { *back }.next = next;
        }
        if !self.next.is_null() {
            unsafe { *next }.back = back;
        }
        self.next = ptr::null_mut();
        self.back = ptr::null_mut();
    }

    pub unsafe fn connect_unchecked(back: &mut Self, next: &mut Self) {
        back.next = next as *mut Self;
        next.back = back as *mut Self;
    }

    pub fn end_of_allocation(ptr: *mut u8, layout: Layout) -> *mut u8 {
        let align = crate::align_up(ptr as usize, mem::align_of::<Self>());
        let obj_align = crate::align_up(align + mem::size_of::<Self>(), layout.align());
        (obj_align + layout.size()) as *mut u8
    }

    pub fn iter(&self) -> AlloqMetaDataIter {
        AlloqMetaDataIter(self as *const AlloqMetaData)
    }
}

pub struct AlloqMetaDataIter(*const AlloqMetaData);

impl Iterator for AlloqMetaDataIter {
    type Item = *const AlloqMetaData;
    fn next(&mut self) -> Option<Self::Item> {
        let r = self.0;
        if r.is_null() {
            None
        } else {
            self.0 = unsafe { (*r).next };
            Some(r)
        }
    }
}

pub trait AllocMethod {
    // TODO: Allocate before first
    fn fit(
        first_and_end: (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> *mut AlloqMetaData;
    fn remove(
        first_and_end: (&mut AlloqMetaData, &mut AlloqMetaData),
        ptr: *mut u8,
        layout: Layout,
    ) {
        let end = unsafe { ptr.offset(layout.size() as isize) };
        let node = first_and_end
            .0
            .iter()
            .find(|&n| unsafe { *n }.end == end)
            .expect("use after free");
        unsafe { *(node as *mut AlloqMetaData) }.disconnect();
    }
}

pub struct FirstFit;
impl AllocMethod for FirstFit {
    fn fit(
        (first, _): (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> *mut AlloqMetaData {
        for node_ptr in first.iter() {
            let node = unsafe { *node_ptr };
            let obj_end = AlloqMetaData::end_of_allocation(node.end as *mut u8, layout);
            if node.next.is_null() {
                // TODO: Check if don't overflow heap
                return node_ptr as *mut AlloqMetaData;
            } else if obj_end <= node.next as *mut u8 {
                return node_ptr as *mut AlloqMetaData;
            }
        }
        panic!("no available memory");
    }
}

// TODO: Impl next-fit.
// As it demands a internal mutation and the last allocation, its raise a lot of implementation
// questions:
// - Should the entire api change just for that?
// - ~~What should happen when the last allocation is invalid?~~ (just check on `remove`)
// - Store the heap end is a problem when it can be moved or expanded

pub struct BestFit;
impl AllocMethod for BestFit {
    fn fit(
        (first, end): (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> *mut AlloqMetaData {
        let mut best = ptr::null_mut();
        let mut dispersion = usize::MAX;
        let mut align = ptr::null();
        for node_ptr in first.iter() {
            let node = unsafe { *node_ptr };
            let obj_end = AlloqMetaData::end_of_allocation(node.end as *mut u8, layout);
            if obj_end <= node.next as *mut u8 {
                let disp = unsafe { (node.next as *mut u8).offset_from(obj_end) } as usize;
                // Get dispersion from `back` also
                if disp < dispersion {
                    best = node_ptr as *mut AlloqMetaData;
                    align = crate::align_up(node_ptr as usize, mem::align_of::<AlloqMetaData>())
                        as *const u8;
                    dispersion = disp;
                }
            }
        }
        if best.is_null() {
            end
        } else {
            assert!(!align.is_null(), "extern modification");
            best
        }
    }
}

pub struct Alloq<A: AllocMethod = FirstFit> {
    pub heap_start: *const u8,
    pub heap_end: *const u8,
    pub first: spin::Mutex<(*mut AlloqMetaData, *mut AlloqMetaData)>,
    pub _marker: PhantomData<A>,
}

impl<A: AllocMethod> Alloq<A> {}

unsafe impl<A: AllocMethod> Allocator for Alloq<A> {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        let mut lock = self.first.lock();
        let ptr = if lock.0.is_null() {
            let meta = unsafe {
                AlloqMetaData::allocate(
                    ptr::null_mut() as *mut AlloqMetaData,
                    self.heap_range(),
                    layout,
                )
            };
            unsafe {
                lock.0 = meta.1;
                lock.1 = meta.1;
                meta.0.end.offset(-(layout.size() as isize))
            }
        } else {
            unsafe {
                let back = A::fit((&mut *lock.0, &mut *lock.1), layout);
                let meta = AlloqMetaData::allocate(back, self.heap_range(), layout);
                lock.1 = meta.1;
                meta.0.end.offset(-(layout.size() as isize))
            }
        };
        let slice = unsafe { slice::from_raw_parts_mut(ptr as *mut u8, layout.size()) };
        NonNull::new(slice).ok_or(AllocError)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        let mut lock = self.first.lock();
        A::remove((&mut *lock.0, &mut *lock.1), ptr.as_ptr(), layout);
    }
}

impl<A: AllocMethod> Alloqator for Alloq<A> {
    type Metadata = AlloqMetaData;

    fn new(heap_range: core::ops::Range<*const u8>) -> Self
    where
        Self: Sized,
    {
        Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            first: (ptr::null_mut(), ptr::null_mut()).into(),
            _marker: PhantomData,
        }
    }

    fn reset(&self) {
        let mut lock = self.first.lock();
        lock.0 = ptr::null_mut();
        lock.1 = ptr::null_mut();
    }

    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }
    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }
}

pub mod first {
    use super::{Alloq as Al, FirstFit};

    pub type Alloq = Al<FirstFit>;

    #[cfg(test)]
    pub mod tests {
        use super::Alloq;

        include!("test.template.rs");
    }
}

pub mod best {
    use super::{Alloq as Al, BestFit};

    pub type Alloq = Al<BestFit>;

    #[cfg(test)]
    pub mod tests {
        use super::Alloq;

        include!("test.template.rs");
    }
}
