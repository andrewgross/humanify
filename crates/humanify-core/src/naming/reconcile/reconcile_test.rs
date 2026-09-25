//! Reconcile pieces the unit-test fixtures (passes_test.rs) do not pin:
//! Babel's line terminators vs `diff`'s, the hunk-header grammar, the
//! corpus gate's arithmetic, and the render forms the oracle pairs found.

use oxc_allocator::Allocator;

use super::hunks::{parse_normal_diff, prior_too_dissimilar};
use crate::babel_view::BabelLines;
use crate::ingest::Ingest;
use crate::naming::waves::render::{private_rename_edits, render_program, render_program_with};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Anchor;
use crate::twins::gates::PrivateRenameSet;

#[test]
fn babel_lines_break_on_every_js_line_terminator() {
    let text = "a\r\nb\rc\u{2028}d\ne";
    let lines = BabelLines::new(text);
    let at = |needle: &str| lines.loc(text.find(needle).unwrap() as u32);
    assert_eq!(at("b"), (2, 0));
    assert_eq!(at("c"), (3, 0));
    assert_eq!(at("d"), (4, 0));
    assert_eq!(at("e"), (5, 0));
}

#[test]
fn hunk_headers_follow_the_ts_grammar() {
    let hunks = parse_normal_diff("3,4c3,4\n< a\n< b\n---\n> c\n>\n7a8\n> x\n9d9\n< y\nnot 1x2\n");
    assert_eq!(hunks.len(), 3);
    assert_eq!(hunks[0].new_lines, vec!["c".to_string(), String::new()]);
    assert_eq!(
        (hunks[1].op, hunks[1].prior_start, hunks[1].new_start),
        (b'a', 7, 8)
    );
    assert_eq!(hunks[2].prior_lines, vec!["y".to_string()]);
}

#[test]
fn corpus_gate_needs_eight_lines_and_half_unchanged() {
    let hunks =
        parse_normal_diff("1,5c1,5\n< a\n< b\n< c\n< d\n< e\n---\n> a\n> b\n> c\n> d\n> e\n");
    assert!(
        !prior_too_dissimilar(&hunks, Some(7)),
        "under 8 lines: never judged"
    );
    assert!(prior_too_dissimilar(&hunks, Some(9)), "4/9 unchanged < 0.5");
    assert!(
        !prior_too_dissimilar(&hunks, Some(10)),
        "5/10 unchanged = 0.5"
    );
    assert!(!prior_too_dissimilar(&hunks, None));
}

/// Found by the gate (2.1.85→86 generated.js): the statement twins'
/// private-name rewrites are not scope renames — the naming-era render must
/// apply them (`#f` → `#A`), in set order, only where the old name holds.
#[test]
fn private_rename_sets_apply_in_order() {
    let text = "class C { #f; #g; m() { return this.#f + this.#g; } }";
    let span = |from: usize, needle: &str| {
        let s = text[from..].find(needle).unwrap() + from;
        oxc_span::Span::new(s as u32, (s + needle.len()) as u32)
    };
    let f1 = span(0, "#f");
    let f2 = span(30, "#f");
    let g1 = span(0, "#g");
    let sets = vec![
        PrivateRenameSet {
            old_name: "f".into(),
            new_name: "A".into(),
            node_spans: vec![f1, f2],
        },
        // `#g` → `#f` does not re-fire on the nodes the first set renamed.
        PrivateRenameSet {
            old_name: "g".into(),
            new_name: "f".into(),
            node_spans: vec![g1],
        },
    ];
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let state = RenameState::new(ingest.semantic(), Anchor::Fresh);
    let edits = private_rename_edits(text, &sets);
    assert_eq!(
        render_program_with(ingest.semantic(), &state, &edits),
        "class C { #A; #f; m() { return this.#A + this.#g; } }"
    );
}

/// Found by the sweep fixtures: a renamed local of a shorthand
/// `export { x }` prints `local as x`; of `import { x }`, `x as local`.
#[test]
fn shorthand_specifiers_keep_the_module_name() {
    let text = "import { a } from \"m\";\nvar b = a;\nexport { b };";
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let program = state.view().program_scope();
    for (old, new) in [("a", "alpha"), ("b", "beta")] {
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: program,
                old_name: old,
                new_name: new,
                expected: None,
            },
            TrailSpec::Untrailed { why: "test" },
        );
        assert!(attempt.applied, "{old} -> {new}");
    }
    assert_eq!(
        render_program(semantic, &state),
        "import { a as alpha } from \"m\";\nvar beta = alpha;\nexport { beta as b };"
    );
}
