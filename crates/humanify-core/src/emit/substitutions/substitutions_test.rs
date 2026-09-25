//! `applySubstitutions` semantics (src/split/substitutions.ts has no test
//! file of its own; these pin its documented behaviour).

use super::{Substitution, apply_substitutions};

fn sub(line: usize, col: usize, from: &str, to: &str) -> Substitution {
    Substitution {
        line,
        col,
        from: from.into(),
        to: to.into(),
    }
}

fn lines(s: &str) -> Vec<String> {
    s.split('\n').map(str::to_string).collect()
}

#[test]
fn splices_right_to_left_within_a_line() {
    let out = apply_substitutions(
        &lines("a + a;\nb;"),
        &[sub(1, 0, "a", "alpha"), sub(1, 4, "a", "alpha")],
    );
    assert_eq!(out, "alpha + alpha;\nb;");
}

#[test]
fn skips_a_second_substitution_at_the_same_position() {
    let out = apply_substitutions(
        &lines("count;"),
        &[
            sub(1, 0, "count", "total"),
            sub(1, 0, "count", "{ count: total }"),
        ],
    );
    assert_eq!(out, "total;");
}

#[test]
fn columns_are_utf16_code_units() {
    // "é" is one UTF-16 unit but two UTF-8 bytes; "😀" is two units.
    let out = apply_substitutions(&lines("é😀 x;"), &[sub(1, 4, "x", "why")]);
    assert_eq!(out, "é😀 why;");
}

#[test]
fn untouched_lines_are_kept_verbatim() {
    let out = apply_substitutions(&lines("one\ntwo\nthree"), &[sub(2, 0, "two", "2")]);
    assert_eq!(out, "one\n2\nthree");
}
