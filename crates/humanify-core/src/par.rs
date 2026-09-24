//! Order-preserving parallel maps — the ONE place the core fans work out
//! to the rayon pool (docs/rust-port/04-performance-model.md).
//!
//! The rule the helpers encode (porting lesson 4: iteration order is a
//! decision input): only a PURE per-item map is parallel, and its results
//! come back in INPUT order, so the caller sees exactly the Vec a
//! sequential `.iter().map().collect()` would build. Decision code — the
//! cascade's tier loop, ambiguous-map insertion, propagation,
//! demote/revoke, pool assignment — stays sequential and never calls here.

use rayon::prelude::*;

/// `items.iter().map(f).collect()`, with `f` run on the rayon pool.
pub fn map_ordered<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync + Send) -> Vec<R> {
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
