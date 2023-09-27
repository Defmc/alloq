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
        let aligned_meta =
            crate::align_up(range.start as usize, mem::align_of::<Self>()) as *mut u8;
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
        let aligned = crate::align_up(ptr as usize, mem::align_of::<Self>()) as *mut Self;
        *aligned = *self;
        aligned as *mut u8
    }

    pub fn disconnect(&mut self) {
        if !self.back.is_null() {
            unsafe {
                (*self.back).next = self.next;
            }
        }
        if !self.next.is_null() {
            unsafe {
                (*self.next).back = self.back;
            }
        }
    }

    pub unsafe fn connect_unchecked(back: &mut Self, next: &mut Self) {
        back.next = next as *mut Self;
        next.back = back as *mut Self;
    }
}

pub trait AllocMethod {
    fn fit(
        first_and_end: (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> Range<*const u8>;
    fn remove(first_and_end: (&mut AlloqMetaData, &mut AlloqMetaData), ptr: *mut u8);
}

pub struct FirstFit;
impl AllocMethod for FirstFit {
    fn fit(
        (first, end): (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> Range<*const u8> {
        todo!()
    }
    fn remove((first, end): (&mut AlloqMetaData, &mut AlloqMetaData), ptr: *mut u8) {
        todo!()
    }
}

pub struct NextFit;
impl AllocMethod for NextFit {
    fn fit(
        (first, end): (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> Range<*const u8> {
        todo!()
    }
    fn remove((first, end): (&mut AlloqMetaData, &mut AlloqMetaData), ptr: *mut u8) {
        todo!()
    }
}

pub struct BestFit;
impl AllocMethod for BestFit {
    fn fit(
        (first, end): (&mut AlloqMetaData, &mut AlloqMetaData),
        layout: Layout,
    ) -> Range<*const u8> {
        todo!()
    }
    fn remove((first, end): (&mut AlloqMetaData, &mut AlloqMetaData), ptr: *mut u8) {
        todo!()
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
                let fit = A::fit((&mut *lock.0, &mut *lock.1), layout);
                let meta = AlloqMetaData::allocate(lock.0, fit, layout);
                lock.1 = meta.1;
                meta.0.end.offset(-(layout.size() as isize))
            }
        };
        let slice = unsafe { slice::from_raw_parts_mut(ptr as *mut u8, layout.size()) };
        NonNull::new(slice).ok_or(AllocError)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, _: core::alloc::Layout) {
        let mut lock = self.first.lock();
        A::remove((&mut *lock.0, &mut *lock.1), ptr.as_ptr());
        // TODO: Support remove from `first` and `end`
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

    fn heap_start(&self) -> *const u8 {
        self.heap_start
    }
    fn heap_end(&self) -> *const u8 {
        self.heap_end
    }
}
