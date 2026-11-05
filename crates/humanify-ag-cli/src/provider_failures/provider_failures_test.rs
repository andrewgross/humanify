//! The run's failed-call tally (concurrency audit R2 + recommendation 1c):
//! every namer's failures in one place, one WARNING for all of them, and
//! the `--stats-json` block the eval refuses a card on.

use super::ProviderFailures;

fn some() -> ProviderFailures {
    ProviderFailures {
        naming: 7,
        modules: 2,
        vendor: 1,
        split: 3,
        tree_reviser: 1,
    }
}

#[test]
fn the_total_covers_every_namer() {
    assert_eq!(some().total(), 14);
    assert_eq!(ProviderFailures::default().total(), 0);
}

#[test]
fn the_stats_block_carries_the_total_and_each_namer() {
    let s = some().stats();
    assert_eq!(
        (
            s.total,
            s.naming,
            s.modules,
            s.vendor,
            s.split,
            s.tree_reviser
        ),
        (14.0, 7.0, 2.0, 1.0, 3.0, 1.0)
    );
    // Always written, all-zero included: absent means an older binary.
    assert_eq!(ProviderFailures::default().stats().total, 0.0);
}

#[test]
fn one_warning_names_every_namer_that_failed() {
    let w = some().warning().expect("a WARNING");
    assert!(
        w.starts_with("WARNING: 14 LLM call(s) failed after every retry (provider errors:"),
        "{w}"
    );
    for part in [
        "naming 7",
        "modules 2",
        "vendor 1",
        "split 3",
        "tree reviser 1",
    ] {
        assert!(w.contains(part), "{part} in {w}");
    }
    assert!(w.contains("names are missing"), "{w}");
}

#[test]
fn the_warning_leaves_out_namers_that_did_not_fail() {
    let w = ProviderFailures {
        vendor: 2,
        ..ProviderFailures::default()
    }
    .warning()
    .expect("a WARNING");
    assert!(w.contains("(provider errors: vendor 2)"), "{w}");
}

#[test]
fn a_clean_run_has_no_warning() {
    assert_eq!(ProviderFailures::default().warning(), None);
}
