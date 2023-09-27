## Impl of several allocators in `no_std` Rust
Just add the algorithm you want to enable on `Cargo.toml`. E.g for `bump`-allocator:
```toml
[dependencies]
# [...]
alloq = {version = "*", features = ["bump"]}
```

Nice, but you can have multiple allocators in your project. So toggle the import. E.g from `list`-allocator to `bump`-allocator:
```rs 
// use alloq::list::Alloq;
use alloq::bump::Alloq;
```
And everything should work.
