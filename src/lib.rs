//! A multithreaded yet simple memory allocator written in Rust.
//!
//! This crate is a hobby project I did to get hands-on experience with unsafe Rust.
//! Nevertheless, I made an effort to write good documentation so that it can also serve as a learning resource.
//!
//! # Usage
//! To use this crate you can add `rusty_malloc` as a dependency in your project's `Cargo.toml`.
//! ```toml
//! [dependencies]
//! rusty_malloc = "0.2"
//! ```
//!
//! ```
//! use rusty_malloc::RustyMalloc;
//! use rusty_malloc::growers::BrkGrower;
//!
//! #[global_allocator]
//! static ALLOCATOR: RustyMalloc<BrkGrower> = unsafe { RustyMalloc::with_grower(BrkGrower::new(4096)) };
//!
//! fn main() {
//!     let v1: Vec<u32> = vec![1, 2, 3];
//!     println!("Brk is cool {:?}", v1);
//! }
//! ```
//!
//! # Allocators
//! Two allocators are exported by this crate - [`RawMalloc`]
//! and [`RustyMalloc`]. Both of them can be used as either global or local allocators.
//! Use [`RawMalloc`] if you are looking for a single-threaded allocator,
//! [`RustyMalloc`] is just a `Mutex` wrapper over it to allow for multithreading.
//!
//! # Mode of operation
//! The allocator uses a straightforward [freelist](#freelist) algorithm:
//! - When an allocation is requested a search for a suitable free block is
//!   started (this is done by traversing the freelist).
//!   The search is greedy meaning that the chosen block is
//!   always the first found and might not be the best fit. A merging algorithm is
//!   also applied to combine adjacent free blocks and increase their content capacity.
//! - If no block is found a request is dispatched to the allocators underlying
//!   [grower](#growers) to give out more memory so that the object can be stored.
//! - Lastly, on deallocation the allocator transforms the to-be-freed block into a freelist
//!   node and prepends it to the freelist.
//!
//! Bellow is a list of the abstractions used by the allocators for operating on the heap:
//!
//! ## Blocks
//! At a purely conceptional level the heap is divided into blocks.
//! Each block has a [header](#headers) followed by either
//! an [allocation](#objects) or a [freelist node](#freelist).
//! To easily distinguish between blocks we will say that
//! a block containing an allocation is occupied
//! whereas a block containing a freelist node is free.
//!
//! ## Headers
//! At the beginning of each block there is a header holding all the essential metadata for that
//! block, i.e. the size of the block contents or whether the block is free or occupied.
//!
//! ## Objects
//! An objects or also an allocation is a memory region on the heap that was given-out by the allocator.
//! It has a fixed nonzero size which is stored in it's preceding header.
//!
//! ## Freelist
//! The freelist is a linked-list data structure embedded within all of the free blocks on the heap.
//! It allows the allocator to reclaim and reuse freed memory, and thereby reduce the memory footprint
//! of the program. When an allocation is freed, the allocator transforms it into a freelist node,
//! which is then prepended to the freelist.
//!
//! ## Growers
//! A grower is the allocators' internal storage buffer.
//! The [`RustyMalloc`] and [`RawMalloc`] allocators are generic over their growers
//! which means that anything that implements [`Grower`]
//! (aka anything that acts a contiguous buffer which can grow)
//! can be used as their buffer. For example, you could build your own stack allocator
//! by implementing the [`Grower`] trait for a `StackBuffer` struct
//! and passing that struct as a parameter to [`RustyMalloc`] to manage it.
//!
//! # Takeaways
//! As a project wrap-up I decided to bench the allocator to see whether it was
//! at all comparable to the default [`System`] allocator that Rust uses.
//! At first I got excited since the test results showed that my allocator did well and
//! even slightly outperformed the default one, however the moment I distributed the test load
//! to multiple CPUs performance started to significantly degrade.
//!
//! I did a bit of profiling and it turned out that for 16 threads over 60% of execution time was spent
//! waiting to acquire a lock for [`RustyMalloc`]'s underlying [`RawMalloc`] allocator. Yikes!
//! It then clicked with me how wrong it was for all threads to share the same heap region,
//! of course there will be a huge amount of contention! It would be much smarter to let threads handle
//! their own memory regions and never have to interfere.
//! This however would be hard to achieve with the crate's current allocator design,
//! since the whole notion of the heap being a contiguous growable buffer would have to be abandoned
//! in favor of conceiving it as discrete chunks spread all across the address space.
//! The reason why I initially didn't consider this more flexible design is that
//! at the beginning I just wanted to write a `brk`-managed allocator,
//! back then I did not see the value of using a scattered heap and just thought of it as
//! unnecessary complication which would make the whole allocator more cumbersome to implement.
//!
//! In short, I did not consider scalability and it bit me hard!
//! So, for all future allocator writers reading this, I hope you learn from my missteps.
//! Use this as a guide to help you avoid the pitfalls I encountered and make your journey
//! a bit easier when building your own allocators.
//!
//! [`RawMalloc`]: allocators::RawMalloc
//! [`RustyMalloc`]: allocators::RustyMalloc
//! [`Grower`]: growers::Grower
//! [`System`]: std::alloc::System
#![feature(allocator_api)]

pub use crate::allocators::RawMalloc;
pub use crate::allocators::RustyMalloc;

pub mod allocators;
mod freelist;
pub mod growers;
mod header;
mod util;
