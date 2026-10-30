//! Statement provenance (exp166): every statement the formatter prints
//! names the INPUT statement its contents came from — including the
//! statements the beautify synthesizes by splitting one input statement
//! into several (`var a = 1, b = 2;`, `x(), y();`). The naming era maps a
//! raw claimed region onto its own statements through this, so an off-by-
//! the-splits ordinal can never move the wrong module.

use super::{FormatOptions, format_file, format_file_with_origins};

/// The input statement (by start offset) each top-level output statement
/// came from, read through the provenance.
fn top_level_origins(code: &str) -> Vec<(String, usize)> {
    let formatted = format_file_with_origins(code, &FormatOptions::default(), &[]).unwrap();
    let alloc = oxc_allocator::Allocator::default();
    let out = crate::ingest::Ingest::parse_module(&alloc, &formatted.text);
    let input_alloc = oxc_allocator::Allocator::default();
    let input = crate::ingest::Ingest::parse_module(&input_alloc, code);
    let input_starts: Vec<u32> = input
        .program
        .body
        .iter()
        .map(|s| oxc_span::GetSpan::span(s).start)
        .collect();
    out.program
        .body
        .iter()
        .map(|s| {
            let span = oxc_span::GetSpan::span(s);
            let origin = formatted
                .statement_origins
                .iter()
                .find(|(o, _)| *o == span.start)
                .map(|(_, i)| *i)
                .expect("every top-level statement has a provenance mark");
            let owner = input_starts.partition_point(|&st| st <= origin) - 1;
            (
                formatted.text[span.start as usize..span.end as usize].to_string(),
                owner,
            )
        })
        .collect()
}

#[test]
fn a_split_statement_names_the_input_statement_it_came_from() {
    let code = "var a=1,b=2;x(),y();function f(){return 1}c&&d();";
    let got = top_level_origins(code);
    let owners: Vec<usize> = got.iter().map(|(_, o)| *o).collect();
    // The formatter splits the first two input statements; every piece
    // keeps its input statement, and the count the ordinal arithmetic
    // assumed (one output statement per input statement) does NOT hold.
    assert!(got.len() > 4, "the formatter split nothing: {got:?}");
    let mut sorted = owners.clone();
    sorted.sort_unstable();
    assert_eq!(owners, sorted, "provenance is monotone: {got:?}");
    assert_eq!(owners.first(), Some(&0));
    assert_eq!(owners.last(), Some(&3), "{got:?}");
    for (text, owner) in &got {
        let want = match text.as_str() {
            t if t.contains('a') && t.starts_with("var") => 0,
            t if t.contains('b') && t.starts_with("var") => 0,
            t if t.starts_with('x') || t.starts_with('y') => 1,
            t if t.starts_with("function") => 2,
            _ => 3,
        };
        assert_eq!(
            *owner, want,
            "{text:?} came from input statement {owner}: {got:?}"
        );
    }
}

#[test]
fn the_provenance_never_changes_the_formatted_bytes() {
    let code = "var a=1,b=2;x(),y();function f(){var q=1,r=2;return q+r}c?d():e();!0;";
    let plain = format_file(code, &FormatOptions::default(), &[]).unwrap();
    let marked = format_file_with_origins(code, &FormatOptions::default(), &[]).unwrap();
    assert_eq!(plain.text, marked.text);
    assert!(plain.statement_origins.is_empty());
    assert!(!marked.statement_origins.is_empty());
}

#[test]
fn a_license_header_shifts_the_marks_with_the_text() {
    let code = "/*! @license MIT */\nvar a=1,b=2;";
    let got = top_level_origins(code);
    assert!(got.iter().all(|(_, o)| *o == 0), "{got:?}");
}
