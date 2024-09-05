# Rusty Malloc
[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-url]: https://crates.io/crates/rusty_malloc
[crates-badge]: https://img.shields.io/crates/v/rusty_malloc.svg
[docs-url]: https://docs.rs/rusty_malloc/0.1.0/rusty_malloc
[docs-badge]: https://docs.rs/rusty_malloc/badge.svg
[mit-url]: LICENSE
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg


A multithreaded yet simple memory allocator written in Rust.

This crate is a hobby project I did to get hands-on experience with unsafe Rust.
Nevertheless, I made an effort to write good documentation so that it can also serve as a learning resource.

### Usage
To use this crate you can add `rusty_malloc` as a dependency in your project's `Cargo.toml`.
```toml
[dependencies]
rusty_malloc = "0.2"
```
```Rust
use rusty_malloc::RustyMalloc;
use rusty_malloc::growers::BrkGrower;

#[global_allocator]
static ALLOCATOR: RustyMalloc<BrkGrower> = unsafe { RustyMalloc::with_grower(BrkGrower::new(4096)) };

fn main() {
    let v1: Vec<u32> = vec![1, 2, 3];
    println!("Brk is cool {:?}", v1);
}
```

To read more about the allocator's mode of operation, check out the [documentation][docs-url].
