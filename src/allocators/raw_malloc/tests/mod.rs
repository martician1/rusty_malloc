#![allow(unused_imports)]

use crate::growers::arena_grower::ArenaGrower;
use crate::util::checked_add;

use self::format::{RecordEntryLayer, SimpleFormatter};

use super::*;

use tracing_subscriber::fmt::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

mod format;

#[test]
fn test_1() {
    // let __filter = EnvFilter::from_default_env()
    //     .add_directive("rusty_malloc::allocators=debug".parse().unwrap());
    // //.add_directive("rusty_malloc::allocators[__alloc]=info".parse().unwrap());

    // let subscriber = Registry::default()
    //     .with(RecordEntryLayer::default())
    //     .with(Layer::new().event_format(SimpleFormatter::default()));

    // tracing::subscriber::set_global_default(subscriber).unwrap();

    const BUF_SIZE: usize = 64 * 1024;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let mut objects: Vec<(*mut u8, Layout)> = vec![];
    for i in 0..8 {
        for j in i..8 {
            let l = Layout::from_size_align(1 << j, 1 << i).unwrap();
            objects.push(unsafe { (allocator.alloc(l), l) });
        }
    }

    for i in 0..objects.len() {
        let (ptr, layout) = objects[i];
        assert_eq!(ptr as usize % layout.align(), 0);
        if i != objects.len() - 1 {
            assert!(checked_add(ptr, layout.size()).unwrap() <= objects[i + 1].0);
        }
        unsafe { allocator.dealloc(ptr, layout) };
        unsafe { assert_eq!(allocator.alloc(layout), ptr) };
    }

    for i in (0..objects.len()).rev() {
        let (ptr, layout) = objects[i];
        unsafe { allocator.dealloc(ptr, layout) };
    }

    for i in 0..objects.len() {
        unsafe { assert_eq!(allocator.alloc(objects[i].1), objects[i].0) };
    }
}

#[test]
fn test_2() {
    const BUF_SIZE: usize = 32 * BLOCK_MIN_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout_1 = Layout::from_size_align(BLOCK_CONTENT_MIN_SIZE, 1).unwrap();
    let layout_2 = Layout::from_size_align(BLOCK_CONTENT_MIN_SIZE * 2, 1).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout_1);
        let p2 = allocator.alloc(layout_1);
        let p3 = allocator.alloc(layout_1);
        let p4 = allocator.alloc(layout_2);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        assert!(!p3.is_null());
        assert!(!p4.is_null());
        allocator.dealloc(p1, layout_1);
        allocator.dealloc(p2, layout_1);
        allocator.dealloc(p4, layout_2);
        assert_eq!(p4, allocator.alloc(layout_2));
    }
}

#[test]
fn test_3() {
    const BUF_SIZE: usize = 32 * BLOCK_MIN_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout_1 = Layout::from_size_align(BLOCK_CONTENT_MIN_SIZE, 1).unwrap();
    let layout_2 = Layout::from_size_align(BLOCK_CONTENT_MIN_SIZE * 2, 1).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout_1);
        let p2 = allocator.alloc(layout_1);
        let p3 = allocator.alloc(layout_1);
        let p4 = allocator.alloc(layout_2);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        assert!(!p3.is_null());
        assert!(!p4.is_null());
        allocator.dealloc(p1, layout_1);
        allocator.dealloc(p4, layout_2);
        allocator.dealloc(p2, layout_1);
        assert_eq!(p4, allocator.alloc(layout_2));
    }
}

#[test]
fn test_4() {
    const BUF_SIZE: usize = 32 * BLOCK_MIN_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout_1 = Layout::from_size_align(BLOCK_CONTENT_MIN_SIZE, 1).unwrap();
    let layout_2 = Layout::from_size_align(BLOCK_CONTENT_MIN_SIZE * 2, 1).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout_1);
        let p2 = allocator.alloc(layout_1);
        let p3 = allocator.alloc(layout_1);
        let p4 = allocator.alloc(layout_2);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        assert!(!p3.is_null());
        assert!(!p4.is_null());
        allocator.dealloc(p2, layout_1);
        allocator.dealloc(p4, layout_2);
        allocator.dealloc(p1, layout_1);
        assert_eq!(p1, allocator.alloc(layout_2));
    }
}

#[test]
fn test_5() {
    const BUF_SIZE: usize = 8 * BLOCK_MIN_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout = Layout::from_size_align(BUF_SIZE - HEADER_SIZE, 1).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout);
        assert!(!p1.is_null());
        // It's ok for the size to be 0 since it should get augmented.
        assert!(allocator
            .alloc(Layout::from_size_align(0, 1).unwrap())
            .is_null());
    }
}

#[test]
fn test_6() {
    const BUF_SIZE: usize = 128 * HEADER_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout_1 = Layout::from_size_align(HEADER_SIZE * 4, HEADER_ALIGN).unwrap();
    let layout_2 = Layout::from_size_align(HEADER_SIZE * 10, HEADER_ALIGN).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout_1);
        let p2 = allocator.alloc(layout_2);
        let p3 = allocator.alloc(layout_2);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        assert!(!p3.is_null());
        allocator.dealloc(p2, layout_2);
        allocator.dealloc(p3, layout_2);

        assert_eq!(p1, allocator.realloc(p1, layout_1, layout_2.size()));
        assert_eq!(p1, allocator.realloc(p1, layout_2, layout_1.size()));

        let p4 = allocator.alloc(layout_1);
        assert_eq!(p2, p4);

        assert_eq!(
            p4.add(5 * HEADER_SIZE),
            allocator.realloc(p1, layout_1, layout_2.size())
        );
    }
}

#[test]
fn test_7() {
    const BUF_SIZE: usize = 64 * HEADER_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout = Layout::from_size_align(HEADER_SIZE * 10, HEADER_ALIGN).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout);
        assert!(!p1.is_null());
        allocator.dealloc(p1, layout);
        let p2 = allocator.alloc(layout);
        let p3 = allocator.alloc(layout);

        assert_ne!(p2, p3);
    }
}

#[test]
fn test_8() {
    const BUF_SIZE: usize = 1024 * HEADER_SIZE;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout = Layout::from_size_align(HEADER_SIZE * 32, HEADER_SIZE * 32).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout);
        let p2 = allocator.alloc(layout);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        assert_eq!(p1.add(layout.size() * 2), p2);
        let p3 = allocator.realloc(p2, layout, layout.size() * 2);
        let p4 = allocator.alloc(Layout::from_size_align(layout.size() * 2, HEADER_ALIGN).unwrap());
        assert_eq!(p1.add(layout.size() + HEADER_SIZE), p4);
        assert_eq!(p4.add(layout.size() * 3 - HEADER_SIZE), p3);
    }
}

#[test]
fn test_9() {
    const BUF_SIZE: usize = BLOCK_MIN_SIZE * 32;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout = Layout::from_size_align(HEADER_SIZE * 8, HEADER_ALIGN).unwrap();
    unsafe {
        let p1 = allocator.alloc(layout);
        assert!(!p1.is_null());
        let p2 = allocator.realloc(p1, layout, 1);
        assert_eq!(p1, p2);
        let p2_header: *mut Header = p2.sub(HEADER_SIZE).cast();
        assert_eq!((*p2_header).content_size(), BLOCK_CONTENT_MIN_SIZE);
    }
}

#[test]
fn test_10() {
    const BUF_SIZE: usize = BLOCK_MIN_SIZE * 8;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let p1 = allocator
        .allocate(Layout::from_size_align(0, 1).unwrap())
        .unwrap()
        .as_ptr() as *mut u8;
    let p2 = allocator
        .allocate(Layout::from_size_align(0, 1).unwrap())
        .unwrap()
        .as_ptr() as *mut u8;
    assert_ne!(p1, p2);
}

#[test]
fn test_11() {
    const BUF_SIZE: usize = BLOCK_MIN_SIZE * 8;
    let mut buf = [0_u8; BUF_SIZE];
    let grower = ArenaGrower::new((&mut buf) as *mut _, BUF_SIZE);
    let allocator = unsafe { RawMalloc::with_grower(grower) };

    let layout_1 = Layout::from_size_align(20, 4).unwrap();
    let layout_2 = Layout::from_size_align(60, 4).unwrap();
    let p1 = allocator.allocate(layout_2).unwrap().as_ptr() as *mut u8;
    let p2 = unsafe {
        Allocator::shrink(&allocator, NonNull::new(p1).unwrap(), layout_2, layout_1)
            .unwrap()
            .as_ptr() as *mut u8
    };
    assert_eq!(p1, p2);
    let p3 = unsafe {
        Allocator::grow(&allocator, NonNull::new(p2).unwrap(), layout_1, layout_2)
            .unwrap()
            .as_ptr() as *mut u8
    };
    assert_eq!(p1, p3);
}
