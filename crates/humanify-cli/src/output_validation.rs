//! Output validation's REPORTING surface (TS: `src/output-validation.ts`).
//!
//! What is ported here, byte-exact against the TS functions
//! (test/parity/wpb4-vectors.json):
//! - the failure records (`OutputParseFailure`, `OutputSemanticFailure`);
//! - `describeParseError`'s location extraction and `buildExcerpt`'s code
//!   frame;
//! - `compareSemantics`' verdict and message over two measurements;
//! - `describeStructuralDivergence`'s text over two token streams — every
//!   detail line INDENTED, because the harness keeps an `ERROR:` headline's
//!   indented continuation and drops everything after an unindented line.
//!
//! What is NOT here yet (decision-bearing, and its only input is RENAMED
//! output, which the Rust pipeline cannot produce until WP3.x): measuring a
//! program's free names and binding count the way Babel's scope tracker
//! counts them, and the rename-invariant signature. Those land with the
//! rename stage that needs them; until then the driver fails loud at
//! stage 9 before any output exists to validate. The re-parse VALIDITY
//! check is here (`parse_failure_of`, oxc) — its verdict is the same
//! question, but its MESSAGE is oxc's, not Babel's (declared; the TS
//! message text is Babel's own, e.g. "unknown: Unexpected token (1:8)").

/// Details of a generated-output parse failure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputParseFailure {
    /// First line of the parser error message.
    pub message: String,
    /// 1-based line of the failure, when known.
    pub line: Option<u32>,
    /// 0-based column of the failure, when known.
    pub column: Option<u32>,
    /// Source lines around the failure, with the failing line marked.
    pub excerpt: Option<String>,
}

/// A violated rename invariant.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputSemanticFailure {
    pub message: String,
    pub added_free_names: Option<Vec<String>>,
    pub removed_free_names: Option<Vec<String>>,
    pub binding_count_before: Option<u64>,
    pub binding_count_after: Option<u64>,
}

/// `FreeNameMeasure`: the names a program observes as free, and its total
/// binding count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FreeNameMeasure {
    pub free_names: Vec<String>,
    pub total_binding_count: u64,
}

const EXCERPT_CONTEXT_LINES: u32 = 2;
const MIN_LINE_NUMBER_WIDTH: usize = 2;
const FAILURE_NAME_SAMPLE: usize = 5;
/// Tokens of context shown either side of the first divergence.
const DIVERGENCE_CONTEXT: usize = 6;

/// A thrown parser error as `describeParseError` sees it: its message
/// (`error.message ?? String(err)`) and its `loc`, when it has one.
#[derive(Clone, Debug, Default)]
pub struct ParserError {
    pub message: String,
    pub loc_line: Option<u32>,
    pub loc_column: Option<u32>,
}

/// `describeParseError(err, code)`.
pub fn describe_parse_error(err: &ParserError, code: &str) -> OutputParseFailure {
    let message = err
        .message
        .split('\n')
        .next()
        .unwrap_or_default()
        .to_string();
    let (line, column) = extract_location(err);
    OutputParseFailure {
        message,
        line,
        column,
        excerpt: line.map(|l| build_excerpt(code, l)),
    }
}

/// `extractLocation`: the error's own `loc.line` wins; else Babel's
/// trailing `(line:column)` anywhere in the message.
fn extract_location(err: &ParserError) -> (Option<u32>, Option<u32>) {
    if let Some(line) = err.loc_line {
        return (Some(line), err.loc_column);
    }
    // /\((\d+):(\d+)\)/ — the first match.
    let m = err.message.as_bytes();
    let mut i = 0;
    while i < m.len() {
        if m[i] == b'('
            && let Some((l, c, _)) = digits_colon_digits(&m[i + 1..])
        {
            return (Some(l), Some(c));
        }
        i += 1;
    }
    (None, None)
}

fn digits_colon_digits(b: &[u8]) -> Option<(u32, u32, usize)> {
    let d1 = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if d1 == 0 || b.get(d1) != Some(&b':') {
        return None;
    }
    let rest = &b[d1 + 1..];
    let d2 = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    if d2 == 0 || rest.get(d2) != Some(&b')') {
        return None;
    }
    let l = std::str::from_utf8(&b[..d1]).ok()?.parse().ok()?;
    let c = std::str::from_utf8(&rest[..d2]).ok()?.parse().ok()?;
    Some((l, c, d1 + d2 + 2))
}

/// `buildExcerpt`: the failing line with two lines of context either side,
/// code-frame style (`> ` marks the failing line; numbers right-aligned to
/// at least two columns).
pub fn build_excerpt(code: &str, failure_line: u32) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    let first = failure_line.saturating_sub(EXCERPT_CONTEXT_LINES).max(1);
    let last = (failure_line + EXCERPT_CONTEXT_LINES).min(lines.len() as u32);
    let width = MIN_LINE_NUMBER_WIDTH.max(last.to_string().len());
    let mut rendered: Vec<String> = Vec::new();
    for line_no in first..=last {
        let marker = if line_no == failure_line { "> " } else { "  " };
        let text = lines
            .get(line_no as usize - 1)
            .copied()
            .unwrap_or("undefined");
        rendered.push(format!("{marker}{line_no:>width$} | {text}"));
    }
    rendered.join("\n")
}

/// `compareSemantics(baseline, after)`: `None` when every invariant held.
pub fn compare_semantics(
    baseline: &FreeNameMeasure,
    after: &FreeNameMeasure,
) -> Option<OutputSemanticFailure> {
    let mut added: Vec<String> = after
        .free_names
        .iter()
        .filter(|n| !baseline.free_names.contains(n))
        .cloned()
        .collect();
    added.sort_by(|a, b| humanify_model::js::cmp_utf16(a, b));
    added.dedup();
    let mut removed: Vec<String> = baseline
        .free_names
        .iter()
        .filter(|n| !after.free_names.contains(n))
        .cloned()
        .collect();
    removed.sort_by(|a, b| humanify_model::js::cmp_utf16(a, b));
    removed.dedup();
    let count_changed = after.total_binding_count != baseline.total_binding_count;
    if added.is_empty() && removed.is_empty() && !count_changed {
        return None;
    }
    let sample = |v: &[String]| {
        v.iter()
            .take(FAILURE_NAME_SAMPLE)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut parts: Vec<String> = Vec::new();
    if !removed.is_empty() {
        parts.push(format!(
            "{} free name(s) became bound (capture): {}",
            removed.len(),
            sample(&removed)
        ));
    }
    if !added.is_empty() {
        parts.push(format!(
            "{} name(s) became free (left-behind reference): {}",
            added.len(),
            sample(&added)
        ));
    }
    if count_changed {
        parts.push(format!(
            "binding count changed {} → {} (split or merged declaration)",
            baseline.total_binding_count, after.total_binding_count
        ));
    }
    Some(OutputSemanticFailure {
        message: format!("Rename semantic invariants violated: {}", parts.join("; ")),
        added_free_names: (!added.is_empty()).then_some(added),
        removed_free_names: (!removed.is_empty()).then_some(removed),
        binding_count_before: count_changed.then_some(baseline.total_binding_count),
        binding_count_after: count_changed.then_some(after.total_binding_count),
    })
}

/// `describeStructuralDivergence`'s text over the two serialized token
/// streams, or `None` when they are equal. The serializer that produces the
/// streams arrives with the rename stage (see the module docs).
pub fn format_divergence(before: &[String], after: &[String]) -> Option<String> {
    let n = before.len().min(after.len());
    let first = (0..n)
        .find(|&i| before[i] != after[i])
        .or((before.len() != after.len()).then_some(n))?;
    let window = |toks: &[String]| {
        let lo = first.saturating_sub(DIVERGENCE_CONTEXT);
        let hi = (first + DIVERGENCE_CONTEXT + 1).min(toks.len());
        toks.get(lo..hi).map(|s| s.join(" ")).unwrap_or_default()
    };
    let tok = |toks: &[String]| {
        let t = toks.get(first).map_or("<end>", String::as_str);
        humanify_model::js::stringify(&humanify_model::js::JsValue::str(t))
    };
    let lengths = if before.len() == after.len() {
        format!("{} tokens each", before.len())
    } else {
        format!("{} tokens before vs {} after", before.len(), after.len())
    };
    Some(format!(
        "  first divergence at token {first} of {lengths}\n    original: {}\n    output:   {}\n    original context: {}\n    output context:   {}",
        tok(before),
        tok(after),
        window(before),
        window(after)
    ))
}

/// The re-parse validity check with oxc: `None` when the code parses as a
/// script-or-module (the TS `sourceType: "unambiguous"`), else the first
/// diagnostic as a failure with its 1-based line / 0-based UTF-16 column
/// and the code frame. The message is oxc's (see the module docs).
pub fn parse_failure_of(code: &str) -> Option<OutputParseFailure> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::unambiguous();
    let ret = oxc_parser::Parser::new(&allocator, code, source_type).parse();
    // Babel reports early errors (a redeclared `const`) at parse time; oxc
    // reports them from the semantic pass — both are "not valid JS".
    let semantic_errors = if ret.diagnostics.is_empty() {
        oxc_semantic::SemanticBuilder::new()
            .with_check_syntax_error(true)
            .build(&ret.program)
            .diagnostics
            .to_vec()
    } else {
        Vec::new()
    };
    let first = ret.diagnostics.first().or(semantic_errors.first())?;
    let offset = first.labels.as_ref().first().map(|l| l.offset() as usize);
    let mut err = ParserError {
        message: first.message.to_string(),
        ..ParserError::default()
    };
    if let Some(off) = offset {
        let before = &code[..off.min(code.len())];
        let line = before.matches('\n').count() as u32 + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        err.loc_line = Some(line);
        err.loc_column = Some(before[line_start..].encode_utf16().count() as u32);
    }
    Some(describe_parse_error(&err, code))
}
