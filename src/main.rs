#![feature(allocator_api)]

use alloq::Alloq;

fn main() {
    let heap_stackish = [0u8; 512];
    let alloqer = Alloq::from_ptr(heap_stackish.as_ptr_range());
    println!("memory chunk:\n{heap_stackish:?}");
    let mut v = Vec::with_capacity_in(10, &alloqer);
    println!("memory chunk after alloc an `vec`:\n{heap_stackish:?}");
    for x in 0u8..10u8 {
        v.push(x);
    }
    println!("memory chunk with value edited:\n{heap_stackish:?}");
    v.push(255);
    println!("memory chunk after a push:\n{heap_stackish:?}");
    println!("memory chunk final:\n{heap_stackish:?}");

    unsafe { alloqer.log_allocations() };
    println!("closing");
}
