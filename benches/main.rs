#![feature(allocator_api)]
#![feature(type_name_of_val)]

use std::{
    alloc::Layout,
    collections::hash_map::DefaultHasher,
    fs::{self, File},
    hash::{Hash, Hasher},
    hint::black_box,
    io::{BufWriter, Write},
    time::{self, Duration, Instant, SystemTime},
};

use alloq::{bump, debump, list::best, list::first, pool, statiq, Alloqator};

pub const HEAP_SIM_SIZE: usize = 1024 * 1024 * 512;
pub static mut HEAP_SIM: [u8; HEAP_SIM_SIZE] = [0u8; HEAP_SIM_SIZE];
pub const TEST_COUNT: usize = 10_usize.pow(3);

fn get_time(f: impl FnOnce()) -> Duration {
    let start = Instant::now();
    f();
    Instant::now() - start
}

fn test_and_clear(a: &impl Alloqator, f: impl FnOnce()) -> Duration {
    let time = get_time(f);
    time
}

macro_rules! unit_bench {
    ($dir:expr, $name:expr, $($alloq:expr),*) => {{
        let file = File::create(format!("{}/{}.csv", $dir, stringify!($name))).unwrap();
        let mut w = BufWriter::new(file);
        writeln!(w, "{}", stringify!($name)).unwrap();
        write!(w, "count, ").unwrap();
        $(write!(w, "{}, ", std::any::type_name_of_val($alloq)).unwrap();)*
        writeln!(w, "").unwrap();
        println!("benchmarking {}", stringify!($name));
        for n in (0..=TEST_COUNT).step_by(10) {
            write!(w, "{n}, ").unwrap();
            $(
                std::println!("testing on {}...", stringify!($alloq));
                let t = $name($alloq, n);
                write!(w, "{}, ", t.as_nanos()).unwrap();
            )*
            writeln!(w, "").unwrap();
        }
    }};
}

macro_rules! run_benches {
    ($dir:expr, $($alloq:expr),*) => {
        unit_bench!($dir, linear_allocation, $($alloq),*);
        unit_bench!($dir, linear_deallocation, $($alloq),*);
        unit_bench!($dir, reverse_deallocation, $($alloq),*);
        unit_bench!($dir, vector_pushing, $($alloq),*);
        unit_bench!($dir, vector_fragmentation, $($alloq),*);
        unit_bench!($dir, reset, $($alloq),*);
    };
}

fn main() {
    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .expect("can't get time")
        .hash(&mut hasher);
    let dir = format!("alloq-bench-{}", hasher.finish());
    fs::create_dir(&dir).expect("can't create a directory");

    println!("preparing alloqators");

    let bump = bump::Alloq::new(unsafe { HEAP_SIM.as_ptr_range() });
    let debump = debump::Alloq::new(unsafe { HEAP_SIM.as_ptr_range() });
    let pool =
        unsafe { pool::Alloq::with_chunk_size(HEAP_SIM.as_ptr_range(), HEAP_SIM_SIZE / 1024, 2) };
    let first = first::Alloq::new(unsafe { HEAP_SIM.as_ptr_range() });
    let best = best::Alloq::new(unsafe { HEAP_SIM.as_ptr_range() });
    let statiq = statiq::Alloq::new(unsafe { HEAP_SIM.as_ptr_range() });

    println!("running benchmarks");
    run_benches!(dir, &first, &best, &bump, &debump, &pool, &statiq);
    println!("benchmarks results saved on {dir}");
}

fn linear_allocation<A: Alloqator>(a: &A, n: usize) -> Duration {
    unsafe { a.reset() };
    let layout = Layout::from_size_align(32, 2).unwrap();
    let mut v = Vec::with_capacity(n);
    let t = test_and_clear(a, || {
        for _x in 0..n {
            v.push(a.alloq(layout));
        }
    });
    assert!(
        v.iter().all(|p| !p.is_null()),
        "linear_allocation assert error: {}:{n} can't allocate memory",
        std::any::type_name::<A>()
    );
    t
}

fn linear_deallocation<A: Alloqator>(a: &A, n: usize) -> Duration {
    unsafe { a.reset() };
    let layout = Layout::from_size_align(32, 2).unwrap();
    let ptrs: Vec<_> = (0..n).map(|_| a.alloq(layout)).collect();
    test_and_clear(a, || {
        for ptr in ptrs {
            unsafe { a.dealloq(ptr.clone(), layout) };
        }
    })
}

fn reverse_deallocation<A: Alloqator>(a: &A, n: usize) -> Duration {
    unsafe { a.reset() };
    let layout = Layout::from_size_align(32, 2).unwrap();
    let ptrs: Vec<_> = (0..n).map(|_| a.alloq(layout)).collect();
    test_and_clear(a, || {
        for ptr in ptrs.iter().rev() {
            unsafe { a.dealloq(ptr.clone(), layout) };
        }
    })
}

fn vector_pushing<A: Alloqator>(a: &A, n: usize) -> Duration {
    unsafe { a.reset() };
    let mut v = Vec::new_in(a);
    let t = test_and_clear(a, || {
        for x in 0..n {
            v.push(x);
        }
    });
    assert_eq!(
        v.iter().sum::<usize>(),
        black_box((0..n).sum::<usize>()),
        "vector_pushing assert error: {}:{n} can't handle reallocs",
        std::any::type_name::<A>()
    );
    t
}

fn reset<A: Alloqator>(a: &A, _n: usize) -> Duration {
    unsafe { a.reset() };
    test_and_clear(a, || unsafe { a.reset() })
}

fn vector_fragmentation<A: Alloqator>(a: &A, n: usize) -> Duration {
    unsafe { a.reset() };
    let mut v1 = Vec::new_in(a);
    let mut v2 = Vec::new_in(a);
    let mut v3 = Vec::new_in(a);
    let t = test_and_clear(a, || {
        for x in 0..(n as isize) {
            if x % 2 == 0 {
                v1.push(x);
            } else {
                v2.push(x);
            }
            v3.push(-x);
        }
    });
    assert!(
        v1.iter().all(|x| x % 2 == 0),
        "vector_fragmentation assert error (v1 contains odds numbers): {}:{n} can't handle multiple reallocs",
        std::any::type_name::<A>()
    );
    assert!(
        v2.iter().all(|x| x % 2 == 1),
        "vector_fragmentation assert error (v2 contains even numbers): {}:{n} can't handle multiple reallocs",
        std::any::type_name::<A>()
    );
    assert_eq!(
        v1.iter().chain(v2.iter()).sum::<isize>(),
        -v3.iter().sum::<isize>(),
        "vector_fragmentation assert error (sum of v3 is not equal to inverse of v1 + v2): {}:{n} can't handle multiple reallocs",
        std::any::type_name::<A>()
    );
    t
}
