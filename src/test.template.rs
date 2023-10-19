extern crate alloc;
extern crate std;
use crate::Alloqator;
use alloc::{boxed::Box, vec::Vec};
use core::{alloc::Layout, mem::MaybeUninit, ptr::null_mut};
use std::thread;

#[test]
fn simple_alloc() {
    let heap = [0u8; 512 * 8];
    let alloqer = Alloq::new(heap.as_ptr_range());
    let layout = Layout::from_size_align(32, 2).unwrap();
    let ptr = unsafe { alloqer.alloc(layout) };
    unsafe { alloqer.dealloc(ptr, layout) };
}

#[test]
fn linear_allocs() {
    let heap = [0u8; 512 * 512];
    let alloqer = Alloq::new(heap.as_ptr_range());
    let layout = Layout::from_size_align(32, 2).unwrap();
    let mut chunks_allocated = [null_mut(); 4];
    for chunk in chunks_allocated.iter_mut() {
        *chunk = unsafe { alloqer.alloc(layout) };
    }

    for &mut chunk in chunks_allocated.iter_mut() {
        unsafe { alloqer.dealloc(chunk, layout) }
    }
}

#[test]
fn stack_allocs() {
    let heap = [0u8; 512 * 512];
    let alloqer = Alloq::new(heap.as_ptr_range());
    let layout = Layout::from_size_align(32, 2).unwrap();
    let mut chunks_allocated = [null_mut(); 4];
    for chunk in chunks_allocated.iter_mut() {
        *chunk = unsafe { alloqer.alloc(layout) };
    }

    for &mut chunk in chunks_allocated.iter_mut().rev() {
        unsafe { alloqer.dealloc(chunk, layout) }
    }
}

#[test]
fn multithread_allocs() {
    static mut ALLOQER: MaybeUninit<Alloq> = MaybeUninit::uninit();
    let heap = [0u8; crate::get_size_hint_in::<i32, Alloq>(200)];
    unsafe {
        ALLOQER = MaybeUninit::new(Alloq::new(heap.as_ptr_range()));
    }
    let layout = Layout::new::<i32>();
    let thread = thread::spawn(|| {
        let layout = Layout::new::<i32>();
        for _ in 0..100 {
            let ptr = unsafe { ALLOQER.assume_init_mut().alloc(layout) };
            unsafe { ALLOQER.assume_init_mut().dealloc(ptr, layout) };
        }
    });
    for _ in 0..100 {
        let ptr = unsafe { ALLOQER.assume_init_mut().alloc(layout) };
        unsafe { ALLOQER.assume_init_mut().dealloc(ptr, layout) };
    }
    thread.join().unwrap();
}

#[test]
fn vec_grow() {
    let heap_stackish = [0u8; 512];
    let alloqer = Alloq::new(heap_stackish.as_ptr_range());
    let mut v = Vec::new_in(&alloqer);
    for x in 0..10 {
        v.push(x);
    }
    assert_eq!(v.iter().sum::<i32>(), 45i32);
}

#[test]
fn boxed() {
    let heap_stackish = [0u8; 512];
    let alloqer = Alloq::new(heap_stackish.as_ptr_range());
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
    let heap_stackish = [0u8; 1024 * 1024];
    let alloqer = Alloq::new(heap_stackish.as_ptr_range());
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
    let heap_stackish = [0u8; crate::get_size_hint_in::<S, Alloq>(10) * 2];
    let alloqer = Alloq::new(heap_stackish.as_ptr_range());
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
    let heap_stackish = [0u8; crate::get_size_hint_in::<[u16; 32], Alloq>(VECTOR_SIZE) * 2];
    let alloqer = Alloq::new(heap_stackish.as_ptr_range());
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
    let heap_stackish = [0u8; crate::get_size_hint_in::<(), Alloq>(VECTOR_SIZE)];
    let alloqer = Alloq::new(heap_stackish.as_ptr_range());
    let mut v = Vec::with_capacity_in(VECTOR_SIZE, &alloqer);
    for _ in 0..VECTOR_SIZE {
        v.push(());
    }
    let b = Box::new_in((), &alloqer);
    assert_eq!(*b, ());
    assert_eq!(v.len(), VECTOR_SIZE);
}