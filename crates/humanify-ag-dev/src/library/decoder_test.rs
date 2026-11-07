//! Ported from test/library-truth-decoder.test.ts (which ran decoder.py):
//! `a("😀");b();c()` — the emoji is 2 UTF-16 units and 4 UTF-8 bytes;
//! col 0 -> x.js, col 8 (`b`) -> y.js, col 11 (`;`) -> nothing, col 12
//! (`c`) -> x.js.

use super::*;

const TEXT: &str = "a(\"\u{1F600}\");b();c()";
const MAPPINGS: &str = "AAAA,QCAA,G,CDAA";

fn offsets(utf16: bool, unmapped: bool) -> Vec<(i64, Option<i64>)> {
    segments(TEXT, MAPPINGS, None, utf16, unmapped)
        .unwrap()
        .into_iter()
        .map(|s| (s.0, s.1))
        .collect()
}

#[test]
fn utf16_columns_and_unmapped_segments() {
    assert_eq!(
        offsets(true, true),
        vec![(0, Some(0)), (10, Some(1)), (13, None), (14, Some(0))]
    );
}

#[test]
fn both_rules_off_is_scorer_4() {
    assert_eq!(
        offsets(false, false),
        vec![(0, Some(0)), (11, Some(1)), (15, Some(0))]
    );
}

#[test]
fn a_column_inside_a_surrogate_pair_counts_three_bytes_per_half() {
    // col 4 splits the emoji's pair: Python reads `a("` + high half (3 + 3
    // bytes), then from col 4 the low half (3 bytes) and the quote.
    let segs = segments(TEXT, "AAAA,IAAA,EAAA", None, true, true).unwrap();
    let offs: Vec<i64> = segs.iter().map(|s| s.0).collect();
    assert_eq!(offs, vec![0, 6, 10]);
}

#[test]
fn vlq_reads_signed_fields() {
    assert_eq!(vlq("AAgBC").unwrap(), vec![0, 0, 16, 1]);
    assert_eq!(vlq("D").unwrap(), vec![-1]);
    assert!(vlq("!").is_err());
}
