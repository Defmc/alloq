#![feature(allocator_api)]
#![feature(type_name_of_val)]

use std::{
    alloc::{Allocator, Layout},
    any,
    ptr::{self},
    time::Instant,
};

use alloq::{
    bump::Alloq as Bump, debump::Alloq as DeBump,
    /* list::Alloq as List, */ pool::Alloq as Pool, Alloqator,
};

const HEAP_SIZE: usize = 1024 * 1024 * 1024;

const TEST_COUNT: usize = 500;

static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

#[inline(always)]
fn get_time(f: impl FnOnce()) -> u128 {
    let start = Instant::now();
    f();
    let end = Instant::now();
    (end - start).as_nanos()
}

macro_rules! run_test {
    ($name:expr, $f:tt, $($alloc:tt),*) => {{
        println!("{} (ns)", $name);
        println!("count, bump, debump, pool, list");
        for n in (0..TEST_COUNT).step_by(10) {
            print!("{n}, ");
            $($f(&mut $alloc, n);)*
            println!("");
        }
    }};
}

fn main() {
    println!("benchmarking");

    let mut bump = Bump::new(unsafe { HEAP.as_ptr_range() });
    let mut debump = DeBump::new(unsafe { HEAP.as_ptr_range() });
    let mut pool = unsafe { Pool::with_chunk_size(HEAP.as_ptr_range(), TEST_COUNT * 32, 2) };
    //    let mut list = List::new(unsafe { HEAP.as_ptr_range() });

    run_test!("linear_allocation", linear_allocation, bump, debump, pool);
    run_test!(
        "linear_deallocation",
        linear_deallocation,
        bump,
        debump,
        pool
    );
    run_test!(
        "stack_like_deallocation",
        stack_like_deallocation,
        bump,
        debump,
        pool
    );
    run_test!(
        "vector_pushing",
        stack_like_deallocation,
        bump,
        debump,
        pool
    );
    run_test!("reset", stack_like_deallocation, bump, debump, pool);
}

fn linear_allocation(a: &mut (impl Alloqator + Allocator), n: usize) {
    let layout = Layout::from_size_align(32, 2).unwrap();
    print!(
        "{}, ",
        get_time(|| {
            for _ in 0..TEST_COUNT {
                unsafe { a.alloc(layout) };
            }
        })
    );
    a.reset();
}

fn linear_deallocation(a: &mut (impl Alloqator + Allocator), n: usize) {
    let layout = Layout::from_size_align(32, 2).unwrap();
    let mut v = Vec::with_capacity(n);
    for e in v.iter_mut() {
        *e = unsafe { a.alloc(layout) };
    }
    print!(
        "{}, ",
        get_time(|| {
            for e in v.iter() {
                unsafe { a.dealloc(*e, layout) };
            }
        })
    );
    a.reset();
}

fn stack_like_deallocation(a: &mut (impl Alloqator + Allocator), n: usize) {
    let layout = Layout::from_size_align(32, 2).unwrap();
    let mut v = Vec::with_capacity(n);
    for e in v.iter_mut() {
        *e = unsafe { a.alloc(layout) };
    }
    print!(
        "{}, ",
        get_time(|| {
            for e in v.iter().rev() {
                unsafe { a.dealloc(*e, layout) };
            }
        })
    );
    a.reset();
}

fn vector_pushing(a: &mut (impl Alloqator + Allocator), n: usize) {
    let mut v = Vec::new_in(&*a);
    print!(
        "{}, ",
        get_time(|| {
            for x in 0..n {
                v.push(x);
            }
        })
    );
    a.reset();
}

fn reset(a: &mut (impl Alloqator + Allocator), n: usize) {
    print!("{}, ", get_time(|| { a.reset() }));
}
