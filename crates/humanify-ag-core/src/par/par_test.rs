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

// --- the ordered mode (rule 9, docs/concurrency.md) ------------------------

use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

/// Restores the throughput mode whatever the test body does.
struct OrderedGuard;
impl Drop for OrderedGuard {
    fn drop(&mut self) {
        super::set_ordered_mode(false);
    }
}

/// The mode is a PROCESS GLOBAL, and cargo's test harness runs tests on
/// parallel threads: without this lock, an ordered test's set-true window
/// overlaps another test's in-flight `map_ordered` and both observe the
/// wrong mode (the throughput probe read peak 1 under the gate — the
/// flake that found it). Every test that sets or probes the mode takes
/// the lock for its whole body (test-only: production sets it once at
/// startup, before any stage runs).
static MODE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Peak simultaneous executions of a body that holds its slot until it
/// observes `want` live (bounded spin, so a sequential run terminates).
fn probe(
    items: usize,
    body: impl Fn(&AtomicUsize, &AtomicUsize, usize) -> usize + Sync,
) -> (Vec<usize>, usize) {
    let live = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let run = |i: usize| body(&live, &peak, i);
    let out = super::map_ordered(&(0..items).collect::<Vec<_>>(), |&i| run(i));
    (out, peak.load(SeqCst))
}

fn hold_until(live: &AtomicUsize, peak: &AtomicUsize, want: usize) {
    let now = live.fetch_add(1, SeqCst) + 1;
    peak.fetch_max(now, SeqCst);
    // Yields, not a pure spin: under a busy box (a parallel cargo build, an
    // index merge) a spin-only wait can burn its budget before the pool
    // schedules the second worker. Every 1k spins, hand the core back.
    let mut spins = 0usize;
    while live.load(SeqCst) < want && spins < 500_000_000 {
        std::hint::spin_loop();
        spins += 1;
        if spins.is_multiple_of(1_000) {
            std::thread::yield_now();
        }
    }
    live.fetch_sub(1, SeqCst);
}

#[test]
fn ordered_mode_runs_map_ordered_one_item_at_a_time() {
    let _serial = MODE_TEST_LOCK.lock().expect("mode lock");
    super::set_ordered_mode(true);
    let _guard = OrderedGuard;
    let (out, peak) = probe(16, |live, peak, _| {
        hold_until(live, peak, 2);
        0
    });
    assert!(out.iter().all(|&o| o == 0));
    assert_eq!(peak, 1, "ordered map_ordered must run one item at a time");
}

#[test]
fn throughput_mode_overlaps_map_ordered_items() {
    let _serial = MODE_TEST_LOCK.lock().expect("mode lock");
    // The default: the pool overlaps items (this is what ordered mode
    // gives up). Two workers at once is the smallest honest observation.
    let (_, peak) = probe(64, |live, peak, _| {
        hold_until(live, peak, 2);
        0
    });
    assert!(peak >= 2, "the pool must overlap items in the default mode");
}

#[test]
fn ordered_mode_runs_beside_without_overlapping() {
    let _serial = MODE_TEST_LOCK.lock().expect("mode lock");
    super::set_ordered_mode(true);
    let _guard = OrderedGuard;
    let events = std::sync::Mutex::new(Vec::new());
    let (bg, here) = super::beside(
        || {
            events.lock().expect("events").push("bg");
            1
        },
        || {
            events.lock().expect("events").push("here");
            2
        },
    );
    assert_eq!((bg, here), (1, 2));
    assert_eq!(*events.into_inner().expect("events"), vec!["bg", "here"]);
}

#[test]
fn ordered_mode_runs_join_legs_one_after_the_other() {
    let _serial = MODE_TEST_LOCK.lock().expect("mode lock");
    super::set_ordered_mode(true);
    let _guard = OrderedGuard;
    let events = std::sync::Mutex::new(Vec::new());
    let (a, b) = super::join(
        || {
            events.lock().expect("events").push("a");
            1
        },
        || {
            events.lock().expect("events").push("b");
            2
        },
    );
    assert_eq!((a, b), (1, 2));
    assert_eq!(*events.into_inner().expect("events"), vec!["a", "b"]);
}
