//! Order-preserving parallel maps — the ONE place the core fans work out
//! to the rayon pool.
//!
//! The rule the helpers encode: only a PURE per-item map is parallel, and
//! its results come back in INPUT order, so the caller sees exactly the
//! Vec a sequential `.iter().map().collect()` would build. (The TS-parity
//! reason this rule was born with — "porting lesson 4: iteration order is
//! a decision input" — died at the cutover; what backs it today is
//! sequential equivalence: a parallel stage in input order is
//! byte-indistinguishable from its sequential loop, which is what the
//! neutrality gate proves. docs/audits/ordering-2026-10.md, inventory (a).)
//! Decision code — the cascade's tier loop, ambiguous-map insertion,
//! propagation, demote/revoke, pool assignment — stays sequential and
//! never calls here.
//!
//! The ordered mode (rule 9, docs/concurrency.md) is a TEST-MODE property:
//! [`set_ordered_mode`] makes every helper run its work sequentially, one
//! item at a time, with identical results. It is set ONCE at startup,
//! before any stage runs; a regular run never pays for it.

use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;

/// The ordered mode's flag: set once at startup (run.order = "ordered"),
/// read-only during the run. Both branches of every helper produce
/// identical results, so nothing that decides OUTPUT reads this — it
/// schedules work only (pattern `order-mode`, docs/concurrency.md).
static ORDERED: AtomicBool = AtomicBool::new(false);

/// Turn the ordered mode on or off. Called once at startup, before any
/// parallel work; flipping it mid-run is unsupported (tests set and
/// restore it around a whole helper call).
pub fn set_ordered_mode(ordered: bool) {
    ORDERED.store(ordered, Ordering::SeqCst);
}

/// Whether the ordered mode is on.
pub fn ordered_mode() -> bool {
    ORDERED.load(Ordering::SeqCst)
}

/// `items.iter().map(f).collect()`, with `f` run on the rayon pool.
pub fn map_ordered<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync + Send) -> Vec<R> {
    if ordered_mode() {
        return items.iter().map(f).collect();
    }
    // An indexed parallel iterator's collect writes each result into its
    // input slot — the order is by construction, not by completion.
    items.par_iter().map(f).collect()
}

/// Sequential producer, parallel consumer: `produce(i)` runs on the CALLING
/// thread for `i` in `0..n`, in index order — so it may borrow data that is
/// not thread-safe (the oxc AST carries `Cell`s) — and `consume` maps each
/// produced item on the rayon pool. Work proceeds in chunks of `chunk`
/// items so at most one chunk of produced items is alive at a time.
/// Results are in index order.
pub fn produce_then_map<P: Send, R: Send>(
    n: usize,
    chunk: usize,
    produce: impl FnMut(usize) -> P,
    consume: impl Fn(P) -> R + Sync + Send,
) -> Vec<R> {
    if ordered_mode() {
        let mut produce = produce;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(consume(produce(i)));
        }
        return out;
    }
    let mut produce = produce;
    let chunk = chunk.max(1);
    let mut out = Vec::with_capacity(n);
    let mut start = 0;
    while start < n {
        let end = (start + chunk).min(n);
        let batch: Vec<P> = (start..end).map(&mut produce).collect();
        let mapped: Vec<R> = batch.into_par_iter().map(&consume).collect();
        out.extend(mapped);
        start = end;
    }
    out
}

/// Run two independent pure computations concurrently; the pair comes
/// back as `(a(), b())`.
pub fn join<A: Send, B: Send>(
    a: impl FnOnce() -> A + Send,
    b: impl FnOnce() -> B + Send,
) -> (A, B) {
    if ordered_mode() {
        return (a(), b());
    }
    rayon::join(a, b)
}

/// Run `background` on a scoped thread while `here` runs on the CALLING
/// thread — so `here` may hold data that is not thread-safe (the oxc AST)
/// while a pure computation proceeds beside it. Returns
/// `(background(), here())`.
pub fn beside<A: Send, B>(
    background: impl FnOnce() -> A + Send,
    here: impl FnOnce() -> B,
) -> (A, B) {
    if ordered_mode() {
        // One at a time: the background job, then the caller's. The two
        // are independent, so the order between them cannot matter.
        return (background(), here());
    }
    std::thread::scope(|scope| {
        let handle = scope.spawn(background);
        let here_result = here();
        let background_result = match handle.join() {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        (background_result, here_result)
    })
}

#[cfg(test)]
mod par_test;
