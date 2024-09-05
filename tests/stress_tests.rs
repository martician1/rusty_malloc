#![feature(allocator_api)]

use std::thread;

use rand::random;

use rusty_malloc::growers::BrkGrower;
use rusty_malloc::RustyMalloc;

#[global_allocator]
static ALLOCATOR: RustyMalloc<BrkGrower> =
    unsafe { RustyMalloc::with_grower(BrkGrower::new(4096 * 64)) };

#[test]
fn stress_test_1() {
    let thread_count = 16;
    let mut handles = vec![];

    for _ in 0..thread_count {
        handles.push(thread::spawn(|| {
            let mut sums = vec![];
            // allocate-deallocate loop
            for _ in 0..10_000 {
                let mut v = vec![];
                for _ in 0..1025 {
                    v.push(random::<u32>());
                }
                let sum = v
                    .iter()
                    .filter(|&&x| x > random::<u32>())
                    .fold(0_u32, |sum, &x| sum.wrapping_add(x));
                sums.push(sum);
            }
            sums.sort_unstable();
            sums.windows(2).filter(|w| w[0] == w[1]).count()
        }));
    }

    let mut acc = 0;
    for handle in handles {
        acc += handle.join().expect("Thread panicked.") as u64;
    }
    assert_ne!(acc, u64::MAX);
}

#[test]
fn stress_test_2() {
    let thread_count = 16;
    let mut handles = Vec::with_capacity_in(1, &ALLOCATOR);

    for _ in 0..thread_count {
        handles.push(thread::spawn(|| {
            let mut nums = Vec::with_capacity_in(1, &ALLOCATOR);
            // allocate-deallocate loop
            for _ in 0..1000 {
                let mut v = Vec::with_capacity_in(1, &ALLOCATOR);
                for _ in 0..1025 {
                    v.push(random::<u32>());
                }
                let tmp: Vec<_, _> = v.into_iter().filter(|&x| x > random::<u32>()).collect();
                nums.push(tmp);
            }
            nums.into_iter()
                .fold(vec![], |mut acc, e| {
                    acc.extend(e);
                    acc
                })
                .into_iter()
                .max()
                .unwrap_or(42)
        }));
    }

    let mut acc = 0;
    for handle in handles {
        acc += handle.join().expect("Thread panicked.") as u64;
    }
    assert_ne!(acc, u64::MAX);
}
