#![feature(allocator_api)]
#![feature(type_name_of_val)]

use std::{
    alloc::{Allocator, Layout},
    any,
    ptr::{self},
    time::{Duration, Instant},
};

use alloq::{
    bump::Alloq as Bump, debump::Alloq as DeBump, list::Alloq as List, pool::Alloq as Pool,
    Alloqator,
};

const HEAP_SIZE: usize = 1024 * 1024 * 1024;

const TEST_COUNT: usize = 100;

static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[inline(always)]
fn get_time(f: impl FnOnce()) -> u128 {
    let start = Instant::now();
    f();
    let end = Instant::now();
    (end - start).as_nanos()
}

fn main() {
    println!("benchmarking");

    let mut bump = Bump::new(unsafe { HEAP.as_ptr_range() });
    let mut debump = DeBump::new(unsafe { HEAP.as_ptr_range() });
    let mut pool = unsafe { Pool::with_chunk_size(HEAP.as_ptr_range(), TEST_COUNT * 32, 2) };
    let mut list = List::new(unsafe { HEAP.as_ptr_range() });

    println!("alloq, count, linear_allocation (ns), linear_deallocation (ns), stack_like_deallocation (ns), vector_pushing (ns)");

    for n in 0..TEST_COUNT {
        test_alloq(&mut bump, n);
        test_alloq(&mut debump, n);
        test_alloq(&mut pool, n);
        //test_alloq(&mut bump, n);
    }
}

fn test_alloq(a: &mut (impl Alloqator + Allocator), n: usize) {
    print!("{}, {n}, ", any::type_name_of_val(a));
    let mut ptrs: Vec<_> = (0..n).map(|_| ptr::null_mut()).collect();
    let layout = Layout::from_size_align(8, 2).unwrap();

    print!(
        "{}, ",
        get_time(|| {
            for ptr in ptrs.iter_mut() {
                *ptr = unsafe { a.alloc(layout) };
            }
        })
    );

    print!(
        "{}, ",
        get_time(|| {
            for ptr in &ptrs {
                unsafe { a.dealloc(*ptr, layout) };
            }
        })
    );

    for ptr in ptrs.iter_mut() {
        *ptr = unsafe { a.alloc(layout) };
    }
    print!(
        "{}, ",
        get_time(|| {
            for ptr in ptrs.iter().rev() {
                unsafe { a.dealloc(*ptr, layout) };
            }
        })
    );

    print!(
        "{}, ",
        get_time(|| {
            let mut v = Vec::new_in(&*a);
            for x in 0..n {
                v.push(x);
            }
        })
    );

    a.reset();

    println!("");
}
