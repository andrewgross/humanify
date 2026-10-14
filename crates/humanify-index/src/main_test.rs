//! The builder's allocator (exp111). A build runs four workers, and each
//! fingerprints bundles of up to tens of MB (an oxc AST, unit prints, a
//! decoded sourcemap: ~1.5 GB at peak for the largest). Under glibc malloc
//! every worker's arena kept the high-water mark of the biggest version it
//! had met, so a long build's RSS ratcheted up with the versions it built
//! (exp108: 31–35 GB per process after ~800 versions, one OOM-killed).
//! mimalloc returns that memory between versions; the RSS curves are in
//! experiments/111-builder-memory/README.md.

/// mimalloc's committed bytes right now, from its own statistics.
fn mimalloc_committed() -> i64 {
    let json = mimalloc::MiMalloc::stats_json().expect("mimalloc stats");
    let stats: serde_json::Value =
        serde_json::from_str(json.to_str().expect("utf-8 stats")).expect("stats are JSON");
    stats["committed"]["current"]
        .as_i64()
        .expect("committed.current in mimalloc's stats")
}

/// The builder's heap IS mimalloc: a large allocation made through the
/// global allocator shows up in mimalloc's committed memory (under the
/// system allocator mimalloc never sees it).
#[test]
fn the_builder_allocates_through_mimalloc() {
    const SIZE: usize = 64 << 20;
    let before = mimalloc_committed();
    let block = vec![1u8; SIZE];
    let during = mimalloc_committed();
    assert_eq!(block.iter().map(|&b| usize::from(b)).sum::<usize>(), SIZE);
    assert!(
        during - before >= SIZE as i64,
        "a {SIZE}-byte allocation moved mimalloc's committed memory by {} bytes",
        during - before
    );
}
