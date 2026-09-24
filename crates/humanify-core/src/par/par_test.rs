//! The order contract of the parallel helpers: results come back in INPUT
//! order no matter which item finishes first, and the sequential producer
//! runs on the calling thread in index order.

use std::cell::RefCell;

use super::{beside, map_ordered, produce_then_map};

/// Uneven work so a parallel map finishes items out of order.
fn uneven(i: usize) -> usize {
    let spins = (97 * i) % 50_000;
    let mut acc = i;
    for k in 0..spins {
        acc = acc.wrapping_mul(31).wrapping_add(k);
    }
    std::hint::black_box(acc);
    i * 2
}

#[test]
fn map_ordered_returns_results_in_input_order() {
    let items: Vec<usize> = (0..5_000).collect();
    let out = map_ordered(&items, |&i| uneven(i));
    let want: Vec<usize> = items.iter().map(|i| i * 2).collect();
    assert_eq!(out, want);
}

#[test]
fn produce_then_map_keeps_index_order_across_chunks() {
    // 1,003 items in chunks of 64: the last chunk is partial.
    let out = produce_then_map(1_003, 64, |i| i, uneven);
    let want: Vec<usize> = (0..1_003).map(|i| i * 2).collect();
    assert_eq!(out, want);
}

#[test]
fn produce_then_map_calls_the_producer_sequentially_in_index_order() {
    // A RefCell is !Sync — the producer may hold non-thread-safe borrows
    // (the oxc AST), so it must run on the calling thread, in order.
    let seen = RefCell::new(Vec::new());
    let out = produce_then_map(
        300,
        7,
        |i| {
            seen.borrow_mut().push(i);
            i + 1
        },
        |p| p * 10,
    );
    assert_eq!(*seen.borrow(), (0..300).collect::<Vec<_>>());
    assert_eq!(out, (0..300).map(|i| (i + 1) * 10).collect::<Vec<_>>());
}

#[test]
fn produce_then_map_handles_empty_input() {
    let out: Vec<usize> = produce_then_map(0, 16, |i| i, |p| p);
    assert!(out.is_empty());
}

#[test]
fn beside_returns_both_results_and_runs_here_on_the_calling_thread() {
    // `here` holds a !Send value (Rc): it must run on this thread.
    let local = std::rc::Rc::new(5);
    let caller = std::thread::current().id();
    let (bg, (here_value, here_thread)) = beside(
        || (0..1_000u64).sum::<u64>(),
        || (*local * 2, std::thread::current().id()),
    );
    assert_eq!(bg, 499_500);
    assert_eq!(here_value, 10);
    assert_eq!(here_thread, caller);
}
