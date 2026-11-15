use super::*;
use crate::census::rust_fns::Source;

fn src(rel: &str, text: &str) -> Source {
    Source {
        rel: rel.to_string(),
        text: text.to_string(),
    }
}

/// A tree holding `files`, with `rust` scanned for fn names.
fn ctx(files: &[&str], rust: &[Source]) -> Context {
    Context::new(
        files.iter().map(|s| s.to_string()),
        rust,
        vec![
            "README.md".into(),
            "docs/**/*.md".into(),
            "**/README.md".into(),
        ],
    )
}

fn rules(found: &[Finding]) -> Vec<(&'static str, usize)> {
    found.iter().map(|f| (f.rule.id(), f.line)).collect()
}

#[test]
fn comments_are_read_and_strings_are_not() {
    let text = "// one\nlet s = \"// not a comment\";\nlet c = '\"'; /* two\nthree */\n/// four\nfn f<'a>() {} // five\n";
    let got = crate::census::concurrency::scan::comments(text);
    let lines: Vec<(usize, &str)> = got.iter().map(|(l, t)| (*l, t.as_str())).collect();
    assert_eq!(
        lines,
        [
            (1, "// one"),
            (3, "/* two\nthree */"),
            (5, "/// four"),
            (6, "// five"),
        ]
    );
}

#[test]
fn provenance_flags_experiment_ids_findings_and_dates() {
    let rust = [src(
        "crates/a/src/x.rs",
        "// plain words\n// from exp123\n// finding #45 said so\n// on 2026-10-03\n// expected 3 items, exp12 is too short\nlet s = \"exp999\";\n",
    )];
    let c = ctx(&["crates/a/src/x.rs"], &rust);
    let found = rust_findings(&rust[0], &c);
    assert_eq!(rules(&found), [("C1", 2), ("C1", 3), ("C1", 4)]);
}

#[test]
fn a_reference_to_a_file_that_is_gone_is_flagged() {
    let rust = [src(
        "crates/a/src/x.rs",
        "// see docs/kept.md and docs/gone.md\n// sibling y.rs, missing z.rs\n// host /work/data/run.json and https://example.com/a/b.md\n// history: `git show abc123:docs/gone.md`\n// writes out.json and .humanify/meta.json\n// reads docs/gone.json\n",
    )];
    let c = ctx(
        &["crates/a/src/x.rs", "crates/a/src/y.rs", "docs/kept.md"],
        &rust,
    );
    let found = rust_findings(&rust[0], &c);
    assert_eq!(rules(&found), [("C2", 1), ("C2", 2), ("C2", 6)]);
    assert!(found[0].text.contains("docs/gone.md"), "{:?}", found[0]);
    assert!(found[1].text.contains("z.rs"), "{:?}", found[1]);
}

#[test]
fn a_directive_needs_a_tag_naming_a_real_check() {
    let rust = [src(
        "crates/a/src/x.rs",
        "// this is never empty\n// it must hold (enforced: a_real_test)\n// do not call twice (enforced: rust:clippy)\n// never zero (enforced: clippy::needless_return)\n// must stay (enforced: no_such_check)\n// Do Not reorder\n// a description, nothing more\n#[test]\nfn a_real_test() {}\n",
    )];
    let c = ctx(&["crates/a/src/x.rs"], &rust);
    let found = rust_findings(&rust[0], &c);
    assert_eq!(rules(&found), [("C3", 1), ("C3", 5), ("C3", 6)]);
}

#[test]
fn markdown_links_and_doc_references_resolve() {
    let md = src(
        "docs/a.md",
        "[ok](b.md) [anchor](#x) [web](https://x.y/z.md) [gone](c.md#h)\nsee docs/b.md and docs/c.md; `git show abc:docs/c.md`\n",
    );
    let c = ctx(&["docs/a.md", "docs/b.md"], &[]);
    let found = markdown_findings(&md, &c);
    assert_eq!(rules(&found), [("C4", 1), ("C4", 2)]);
}

#[test]
fn only_the_allowlisted_markdown_files_may_exist() {
    let c = ctx(&[], &[]);
    let found = markdown_file_findings(
        &[
            "README.md".to_string(),
            "docs/x/y.md".to_string(),
            "test/e2e/fixtures/a/README.md".to_string(),
            "notes.md".to_string(),
            "experiments/1-a/RESULTS.md".to_string(),
        ],
        &c,
    );
    let files: Vec<&str> = found.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(files, ["notes.md", "experiments/1-a/RESULTS.md"]);
    assert!(found.iter().all(|f| f.rule == Rule::Markdown));
}

#[test]
fn globs_match_whole_segments() {
    assert!(glob_match("docs/**/*.md", "docs/a.md"));
    assert!(glob_match("docs/**/*.md", "docs/x/y/a.md"));
    assert!(glob_match("**/README.md", "README.md"));
    assert!(glob_match("**/README.md", "a/b/README.md"));
    assert!(glob_match(
        "experiments/*/README.md",
        "experiments/1-a/README.md"
    ));
    assert!(!glob_match(
        "experiments/*/README.md",
        "experiments/1-a/b/README.md"
    ));
    assert!(!glob_match("docs/*.md", "docs/x/a.md"));
    assert!(!glob_match("*.md", "a/b.md"));
}

/// A finding is new when the merge-base had no finding of the same rule on
/// the same text in the same file; moving a line does not make it new.
#[test]
fn only_findings_absent_at_the_base_are_new() {
    let rust_now = [src(
        "crates/a/src/x.rs",
        "// added line\n// from exp100\n// from exp100\n// from exp200\n",
    )];
    let rust_base = [src("crates/a/src/x.rs", "// from exp200\n// from exp100\n")];
    let c = ctx(&["crates/a/src/x.rs"], &rust_now);
    let now = rust_findings(&rust_now[0], &c);
    let base = rust_findings(&rust_base[0], &c);
    let new = new_findings(&now, &base);
    assert_eq!(new.len(), 1, "{new:?}");
    assert_eq!(new[0].text, "// from exp100");
}

#[test]
fn the_report_counts_every_rule_and_asks_for_review_only_on_new_findings() {
    let rust = [src("crates/a/src/x.rs", "// exp100\n")];
    let c = ctx(&["crates/a/src/x.rs"], &rust);
    let all = rust_findings(&rust[0], &c);
    let (code, text) = report(&all, &all, "abc", false);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("  C1         1     0  provenance"), "{text}");
    assert!(text.contains("COMMENT CENSUS GREEN"), "{text}");
    let (code, text) = report(&all, &[], "abc", false);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("crates/a/src/x.rs:1 [C1]"), "{text}");
    assert!(text.contains("COMMENT CENSUS: REVIEW"), "{text}");
}
