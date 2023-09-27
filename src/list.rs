use core::{
    alloc::{Allocator, Layout},
    marker::PhantomData,
    mem,
    ops::Range,
    ptr,
};

use crate::Alloqator;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlloqMetaData {
    pub end: *const u8,
    pub next: *mut Self,
    pub back: *mut Self,
}

impl AlloqMetaData {
    pub unsafe fn allocate(list: *mut Self, range: Range<*const u8>, layout: Layout) -> Self {
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
        s.write(aligned_meta);
        s
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
    fn fit(first_meta: &mut AlloqMetaData, layout: Layout) -> Range<*const u8>;
    fn remove(first_meta: &mut AlloqMetaData, ptr: *mut u8);
}

pub struct FirstFit;
impl AllocMethod for FirstFit {
    fn fit(first_meta: &mut AlloqMetaData, layout: Layout) -> Range<*const u8> {
        todo!()
    }
    fn remove(first_meta: &mut AlloqMetaData, ptr: *mut u8) {
        todo!()
    }
}

pub struct NextFit;
impl AllocMethod for NextFit {
    fn fit(first_meta: &mut AlloqMetaData, layout: Layout) -> Range<*const u8> {
        todo!()
    }
    fn remove(first_meta: &mut AlloqMetaData, ptr: *mut u8) {
        todo!()
    }
}

pub struct BestFit;
impl AllocMethod for BestFit {
    fn fit(first_meta: &mut AlloqMetaData, layout: Layout) -> Range<*const u8> {
        todo!()
    }
    fn remove(first_meta: &mut AlloqMetaData, ptr: *mut u8) {
        todo!()
    }
}

pub struct Alloq<A: AllocMethod = FirstFit> {
    pub heap_start: *const u8,
    pub heap_end: *const u8,
    pub first: spin::Mutex<Option<AlloqMetaData>>,
    pub _marker: PhantomData<A>,
}

impl<A: AllocMethod> Alloq<A> {}

unsafe impl<A: AllocMethod> Allocator for Alloq<A> {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, core::alloc::AllocError> {
        let mut lock = self.first.lock();
        if lock.is_none() {
            let meta = unsafe {
                AlloqMetaData::allocate(
                    ptr::null_mut() as *mut AlloqMetaData,
                    self.heap_range(),
                    layout,
                )
            };
            *lock = Some(meta);
        } else {
            let fit = A::fit(lock.as_mut().unwrap(), layout);
            unsafe { AlloqMetaData::allocate(lock.as_mut().unwrap(), fit, layout) };
        }
        todo!()
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, _: core::alloc::Layout) {
        let mut lock = self.first.lock();
        A::remove(lock.as_mut().unwrap(), ptr.as_ptr());
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
            first: None.into(),
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
