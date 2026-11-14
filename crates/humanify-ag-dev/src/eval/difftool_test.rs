use super::*;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dev-difftool-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn changed_lines_count_both_sides_of_a_modified_line() {
    let d = dir("count");
    std::fs::write(d.join("a"), "x\ny\nz\n").unwrap();
    std::fs::write(d.join("b"), "x\nY\nz\nw\n").unwrap();
    let differ = Differ;
    assert_eq!(differ.changed_lines(&d.join("a"), &d.join("b")).unwrap(), 3);
    assert_eq!(
        differ
            .changed_line_texts(&d.join("a"), &d.join("b"))
            .unwrap(),
        ["y", "Y", "w"]
    );
    assert_eq!(differ.changed_lines(&d.join("a"), &d.join("a")).unwrap(), 0);
}

#[test]
fn crlf_is_normalised_and_a_missing_file_is_an_error() {
    let d = dir("crlf");
    std::fs::write(d.join("a"), "x\r\ny\r\n").unwrap();
    std::fs::write(d.join("b"), "x\ny\n").unwrap();
    let differ = Differ;
    assert_eq!(differ.changed_lines(&d.join("a"), &d.join("b")).unwrap(), 0);
    assert!(
        differ
            .changed_lines(&d.join("a"), &d.join("missing"))
            .is_err()
    );
}

#[test]
fn a_long_file_is_charged_its_minimal_edit_not_the_speed_heuristics() {
    // GNU diff's default gives up looking for the shortest edit on long
    // inputs (exp222: 229,478 lines against a minimal 60,876 on one
    // 365k-line file). This pair reads 17,600 by default; the minimum is
    // 14,000.
    let d = dir("minimal");
    let (a, b, minimal) = crate::eval::test_support::overcharged_pair(16_000);
    assert_eq!(minimal, 14_000);
    std::fs::write(d.join("a"), a).unwrap();
    std::fs::write(d.join("b"), b).unwrap();
    let differ = Differ;
    assert_eq!(
        differ.changed_lines(&d.join("a"), &d.join("b")).unwrap(),
        minimal
    );
    assert_eq!(
        differ
            .changed_line_texts(&d.join("a"), &d.join("b"))
            .unwrap()
            .len(),
        minimal
    );
}
