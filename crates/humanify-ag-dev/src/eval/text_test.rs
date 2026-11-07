use super::*;

#[test]
fn words_follow_the_js_word_regex() {
    let w: Vec<&str> = words("a1px 1px $x_y.z9 é")
        .into_iter()
        .map(|(_, w)| w)
        .collect();
    assert_eq!(w, ["a1px", "px", "$x_y", "z9"]);
}

#[test]
fn masked_head_blanks_every_word_of_the_first_line() {
    assert_eq!(
        masked_head("var foo = bar(1, \"x\");\nmore"),
        "_ _ = _(1, \"_\");"
    );
}

#[test]
fn edited_line_counts_are_set_based() {
    assert_eq!(edited_line_counts("a\nb\nc", "a\nc\nd\nd"), (1, 2));
}

#[test]
fn lcs_and_churn() {
    let a = ["x", "y", "z"];
    let b = ["x", "z", "w"];
    assert_eq!(lcs_len(&a, &b), 2);
    assert_eq!(line_churn(&a, &b), 2);
    let keep = on_lcs(&["p", "q", "r"], &["q", "p", "r"]);
    assert_eq!(keep.len(), 2);
    assert!(keep.contains(&2));
}

#[test]
fn token_set_and_overlap() {
    let a = token_set("function fooBar(ab, cde) { return cde; }");
    assert!(a.contains("fooBar") && a.contains("cde") && !a.contains("ab"));
    let b = token_set("function fooBar(cde) {}");
    assert!((overlap(&a, &b) - 3.0 / 4.0).abs() < 1e-9);
}
