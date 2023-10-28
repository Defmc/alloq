## Impl of several allocators in `no_std` Rust
Just add the algorithm you want to enable on `Cargo.toml`. E.g for `bump`-allocator:
```toml
[dependencies]
# [...]
alloq = {version = "*", features = ["bump"]}
```

Nice, but you can have multiple allocators in your project. So toggle the import. E.g from free list allocator with best-fit allocator to `bump`-allocator:
```rs 
// use alloq::list::best::Alloq;
use alloq::bump::Alloq;
```
And everything should work.

## Benchmark
Run `cargo bench` to generate the benchmark results. The command should have created a folder like `alloq-bench-1091070246479467809` (these numbers doesn't matter, it's just for avoid folder conflicts between benchmarks). Open it and copy `bench.gp` gnuplot script template, run it and open with a image viewer like `feh`:
```sh
cd alloq-bench-* && cp ../bench.gp bench.gp && gnuplot bench.gp && feh gp_out.png
```
