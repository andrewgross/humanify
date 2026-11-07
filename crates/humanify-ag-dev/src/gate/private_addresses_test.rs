use super::*;

/// Assembled, so this file does not itself carry an address.
fn dot(parts: &[u32]) -> String {
    parts
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

#[test]
fn the_matcher_catches_each_private_range_and_nothing_public() {
    for a in [
        dot(&[192, 168, 1, 234]),
        dot(&[10, 0, 0, 1]),
        dot(&[172, 16, 5, 4]),
        dot(&[172, 31, 255, 255]),
    ] {
        assert_eq!(
            private_addresses(&format!("http://{a}:8000/v1")),
            std::slice::from_ref(&a)
        );
        assert_eq!(
            private_addresses(&format!("at {a}.")),
            std::slice::from_ref(&a)
        );
    }
    for s in [
        dot(&[172, 15, 0, 1]),
        dot(&[172, 32, 0, 1]),
        dot(&[8, 8, 8, 8]),
        dot(&[127, 0, 0, 1]),
        format!("v{}", dot(&[1, 10, 0, 0, 1])),
        dot(&[10, 0, 0, 1, 5]),
        dot(&[10, 0, 0, 256]),
        "<llm-host>:8000".to_string(),
    ] {
        assert!(private_addresses(&s).is_empty(), "{s}");
    }
    // Two in one text, both found, in order.
    let two = format!("{} and {}", dot(&[10, 1, 2, 3]), dot(&[192, 168, 0, 9]));
    assert_eq!(
        private_addresses(&two),
        [dot(&[10, 1, 2, 3]), dot(&[192, 168, 0, 9])]
    );
}

#[test]
fn a_planted_address_in_a_tracked_file_is_reported_by_name() {
    let files = vec![
        ("a.txt".to_string(), b"nothing here".to_vec()),
        (
            "b.md".to_string(),
            format!("host {}", dot(&[192, 168, 1, 2])).into_bytes(),
        ),
    ];
    let hits = scan(&files, &[]);
    assert_eq!(hits, [format!("b.md: {}", dot(&[192, 168, 1, 2]))]);
    assert!(scan(&files, &[("b.md", "a test fixture")]).is_empty());
}

#[test]
fn no_tracked_file_contains_one() {
    let repo = crate::paths::repo_root();
    let hits = scan(&tracked(&repo).unwrap(), ALLOWED);
    assert!(hits.is_empty(), "{}", report(&hits));
}
