use core::{
    alloc::{AllocError, Allocator, Layout},
    marker::PhantomData,
    mem,
    ops::{DerefMut, Range},
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
        let range_end = if list.is_null() || (*list).next.is_null() {
            range.end
        } else {
            (*list).next.cast::<u8>().cast_const()
        };
        let aligned_meta =
            crate::align_up(range_start as usize, mem::align_of::<Self>()) as *mut u8;
        let aligned_val = crate::align_up(
            aligned_meta.offset(mem::size_of::<Self>() as isize) as usize,
            layout.align(),
        ) as *mut u8;
        let s = Self {
            end: aligned_val.offset(layout.size() as isize),
            next: ptr::null_mut(),
            back: list,
        };
        assert!(
            aligned_val.offset(layout.size() as isize).cast_const() <= range_end,
            "no available memory, end is {range_end:?}"
        );
        let s_ptr = s.write(aligned_meta);
        if !list.is_null() {
            let list_obj = list.as_mut().unwrap();
            let s_obj = s_ptr.as_mut().unwrap();
            if !list_obj.next.is_null() {
                Self::connect_unchecked(s_obj, list_obj.next.as_mut().unwrap());
            }
            Self::connect_unchecked(list_obj, s_obj);
        }
        (s, s_ptr)
    }

    pub unsafe fn write(&self, ptr: *mut u8) -> *mut Self {
        let ptr = ptr.cast::<Self>();
        *ptr = *self;
        ptr
    }

    pub fn disconnect(&mut self) {
        let next = self.next;
        let back = self.back;
        if !self.back.is_null() {
            unsafe { (*back).next = next };
        }
        if !self.next.is_null() {
            unsafe { (*next).back = back };
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
        first_and_end: &mut (*mut AlloqMetaData, *mut AlloqMetaData),
        ptr: *mut u8,
        layout: Layout,
    ) {
        let ptr_end = unsafe { ptr.offset(layout.size() as isize) };
        unsafe {
            if first_and_end.1.as_ref().unwrap().end == ptr_end {
                first_and_end.1 = first_and_end.1.as_ref().unwrap().back;
                return;
            }
        }
        let node = unsafe {
            first_and_end
                .0
                .as_ref()
                .unwrap()
                .iter()
                .find(|&n| (*n).end == ptr_end)
        }
        .expect("use after free");
        unsafe { *node.cast_mut() }.disconnect();
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
            let obj_end = AlloqMetaData::end_of_allocation(node.end.cast_mut(), layout);
            if node.next.is_null() {
                // TODO: Check if don't overflow heap
                return node_ptr.cast_mut();
            } else if obj_end <= node.next.cast() {
                return node_ptr.cast_mut();
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
        for node_ptr in first.iter() {
            let node = unsafe { *node_ptr };
            let obj_end = AlloqMetaData::end_of_allocation(node.end as *mut u8, layout);
            if obj_end <= node.next.cast() {
                let disp = unsafe { (node.next.cast::<u8>()).offset_from(obj_end) } as usize;
                // TODO: Get dispersion from `back` also
                if disp < dispersion {
                    best = node_ptr as *mut AlloqMetaData;
                    dispersion = disp;
                }
            }
        }
        if best.is_null() {
            end as *mut AlloqMetaData
        } else {
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
        let ptr = unsafe {
            let back = A::fit((lock.0.as_mut().unwrap(), lock.1.as_mut().unwrap()), layout);
            let meta = AlloqMetaData::allocate(back, self.heap_range(), layout);
            if meta.1 > lock.1 {
                lock.1 = meta.1;
            }
            meta.0.end.offset(-(layout.size() as isize))
        };
        let slice = unsafe { slice::from_raw_parts_mut(ptr.cast_mut(), layout.size()) };
        NonNull::new(slice).ok_or(AllocError)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        let mut lock = self.first.lock();
        A::remove(&mut (lock.0, lock.1), ptr.as_ptr(), layout);
    }
}

impl<A: AllocMethod> Alloqator for Alloq<A> {
    type Metadata = AlloqMetaData;

    fn new(heap_range: core::ops::Range<*const u8>) -> Self
    where
        Self: Sized,
    {
        let offset = unsafe {
            AlloqMetaData::allocate(ptr::null_mut(), heap_range.clone(), Layout::new::<u8>())
        };
        let s = Self {
            heap_start: heap_range.start,
            heap_end: heap_range.end,
            first: (offset.1, offset.1).into(),
            _marker: PhantomData,
        };
        s
    }

    unsafe fn reset(&self) {
        let mut lock = self.first.lock();
        let offset = unsafe {
            AlloqMetaData::allocate(ptr::null_mut(), self.heap_range(), Layout::new::<u8>())
        };
        lock.0 = offset.1;
        lock.1 = offset.1;
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
