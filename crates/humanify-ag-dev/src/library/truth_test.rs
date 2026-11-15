use super::*;

/// The bytes each segment holds inside a span: a segment starting before
/// the span counts from its start, the last one runs to the span's end,
/// and segments in insertion order sum per source.
#[test]
fn segment_bytes_clip_each_segment_to_the_span() {
    let segs = [(0, 5), (10, 3), (20, 5)];
    assert_eq!(segment_bytes(&segs, 5, 25), vec![(0, 5), (1, 10), (2, 5)]);
    assert_eq!(segment_bytes(&segs, 12, 15), vec![(1, 3)]);
    assert_eq!(segment_bytes(&segs, 30, 30), vec![]);
    let by = source_bytes(&segs, 0, 30);
    assert_eq!(by.items().collect::<Vec<_>>(), vec![(&5, 20), (&3, 10)]);
}
