A multithreaded yet simple memory allocator written in Rust.

This crate is a hobby project I did to get hands-on experience with unsafe Rust.
Nevertheless, I made an effort to write good documentation so that it can also serve as a learning resource.

# Usage
To use this crate you can add `rusty_malloc` as a dependency in your project's `Cargo.toml`.
```toml
[dependencies]
rusty_malloc = "0.1"
```

```
use rusty_malloc::RustyMalloc;
use rusty_malloc::growers::BrkGrower;

#[global_allocator]
static ALLOCATOR: RustyMalloc<BrkGrower> = RustyMalloc::with_grower(BrkGrower::new());

fn main() {
    let v1: Vec<u32> = vec![1, 2, 3];
    println!("Brk is kinda cool {:?}", v1);
}
```

To read more about the allocators mode of operation, check out the docs.rs documentation.
