//! Code selection for the LLM rename prompt — src/rename/code-window.ts.
//!
//! A function over [`MAX_CODE_LINES`] is shown as declaration-anchored
//! line windows: the header (first 30 lines) and closing line always, plus
//! a window around each requested identifier's declaration line, merged,
//! with elision markers between. Anchors are input-file lines mapped onto
//! generated lines — exact only when the generated line count equals the
//! function's loc span; otherwise (or with no locs) the legacy flat cut.
//!
//! Line math is the TS's exactly: lines are `code.split("\n")` (a `\r`
//! stays on its line; a trailing newline makes an empty last line), counts
//! are line counts, and the identifier-rescue search is a whole-token
//! match over UTF-16 units (probe cases `crlf`, `trailing-newline`,
//! `name-rescue-astral`, `empty-name` in test/parity/wp42-vectors.json).
//! The TS's `debug.log` lines are not reproduced (log-only; no output
//! reads them).

/// Maximum lines of function code shown to the LLM per request.
pub const MAX_CODE_LINES: usize = 500;

/// Header window: function signature + opening context, always shown.
const HEADER_LINES: i64 = 30;
/// Window padding around an anchored declaration line.
const PAD_BEFORE: i64 = 20;
const PAD_AFTER: i64 = 40;
/// Padding floor when the budget forces windows to shrink.
const MIN_PAD: i64 = 2;

/// The flat cut's tail marker (also [`cap_context_code`]'s).
const TRUNCATED_TAIL: &str = "\n  // ... [truncated] ...\n}";

/// One rename request's code selection (`FunctionCodeSelection`). Lines
/// are 1-based input-file lines; `None` is the TS `undefined`.
#[derive(Clone, Copy, Debug)]
pub struct FunctionCodeSelection<'a> {
    /// Full generated function code.
    pub code: &'a str,
    /// Session id (the TS logs it; no output depends on it).
    pub session_id: &'a str,
    pub fn_start_line: Option<i64>,
    pub fn_end_line: Option<i64>,
    /// Declaration lines of the requested identifiers, positionally
    /// aligned with `identifier_names`. An entry that is None or outside
    /// the function's range yields no window on its own.
    pub anchor_start_lines: Option<&'a [Option<i64>]>,
    /// Used only to rescue an entry with no usable declaration loc: the
    /// identifier is located in the GENERATED code instead.
    pub identifier_names: Option<&'a [String]>,
}

fn flat_cut(lines: &[&str]) -> String {
    format!("{}{TRUNCATED_TAIL}", lines[..MAX_CODE_LINES].join("\n"))
}

/// Cap prompt CONTEXT code (the prior version of a close-matched function)
/// at the code budget: an uncapped multi-thousand-line prior overflows the
/// model context and 400-fails the whole batch.
pub fn cap_context_code(code: &str, _session_id: &str) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    if lines.len() <= MAX_CODE_LINES {
        return code.to_string();
    }
    flat_cut(&lines)
}

/// 1-based inclusive line window.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Window {
    from: i64,
    to: i64,
}

/// Merge windows that overlap or touch (a stable sort by `from`, as the
/// TS's `Array.prototype.sort`).
fn merge_windows(mut windows: Vec<Window>) -> Vec<Window> {
    windows.sort_by_key(|w| w.from);
    let mut merged: Vec<Window> = Vec::with_capacity(windows.len());
    for w in windows {
        match merged.last_mut() {
            Some(last) if w.from <= last.to + 1 => {
                if w.to > last.to {
                    last.to = w.to;
                }
            }
            _ => merged.push(w),
        }
    }
    merged
}

fn total_lines(windows: &[Window]) -> i64 {
    windows.iter().map(|w| w.to - w.from + 1).sum()
}

fn build_windows(anchors: &[i64], line_count: i64, pad_before: i64, pad_after: i64) -> Vec<Window> {
    let mut windows = vec![
        Window {
            from: 1,
            to: HEADER_LINES.min(line_count),
        },
        Window {
            from: line_count,
            to: line_count,
        },
    ];
    windows.extend(anchors.iter().map(|a| Window {
        from: (a - pad_before).max(1),
        to: (a + pad_after).min(line_count),
    }));
    merge_windows(windows)
}

fn render_windows(lines: &[&str], windows: &[Window]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut prev_end = 0;
    for w in windows {
        if w.from > prev_end + 1 {
            parts.push(format!(
                "  // … [lines {}–{} omitted] …",
                prev_end + 1,
                w.from - 1
            ));
        }
        parts.extend((w.from..=w.to).map(|i| lines[(i - 1) as usize].to_string()));
        prev_end = w.to;
    }
    parts.join("\n")
}

/// Map input-file anchor lines to function-relative generated lines; None
/// when the loc→generated mapping cannot be trusted.
fn resolve_anchors(sel: &FunctionCodeSelection<'_>, lines: &[&str]) -> Option<Vec<i64>> {
    let (start, end, anchor_lines) =
        match (sel.fn_start_line, sel.fn_end_line, sel.anchor_start_lines) {
            (Some(s), Some(e), Some(a)) => (s, e, a),
            _ => return None,
        };
    if end - start + 1 != lines.len() as i64 {
        return None;
    }
    let mut anchors = Vec::with_capacity(anchor_lines.len());
    for (i, line) in anchor_lines.iter().enumerate() {
        if let Some(line) = line.filter(|l| *l >= start && *l <= end) {
            anchors.push(line - start + 1);
            continue;
        }
        let found = sel
            .identifier_names
            .and_then(|names| names.get(i))
            .map_or(-1, |name| first_occurrence_line(lines, name));
        if found > 0 {
            anchors.push(found);
        }
    }
    Some(anchors)
}

fn is_ident_unit(u: u16) -> bool {
    u8::try_from(u).is_ok_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

/// 1-based line of the first whole-token occurrence of `name`, or -1:
/// the TS regex `(?<![A-Za-z0-9_$])NAME(?![A-Za-z0-9_$])` (no `u` flag —
/// it scans UTF-16 code units, so an empty name matches between the two
/// halves of a surrogate pair).
fn first_occurrence_line(lines: &[&str], name: &str) -> i64 {
    let needle: Vec<u16> = name.encode_utf16().collect();
    for (i, line) in lines.iter().enumerate() {
        let hay: Vec<u16> = line.encode_utf16().collect();
        if has_token(&hay, &needle) {
            return i as i64 + 1;
        }
    }
    -1
}

fn has_token(hay: &[u16], needle: &[u16]) -> bool {
    if needle.len() > hay.len() {
        return false;
    }
    (0..=hay.len() - needle.len()).any(|p| {
        hay[p..p + needle.len()] == *needle
            && (p == 0 || !is_ident_unit(hay[p - 1]))
            && hay.get(p + needle.len()).is_none_or(|u| !is_ident_unit(*u))
    })
}

/// Select the code shown to the LLM for one rename request: under the cap
/// the code unchanged; over it, declaration-anchored windows, or the flat
/// cut when anchors are unavailable. Padding halves (floor 2) until the
/// windows fit the budget.
pub fn select_function_code(sel: &FunctionCodeSelection<'_>) -> String {
    let lines: Vec<&str> = sel.code.split('\n').collect();
    if lines.len() <= MAX_CODE_LINES {
        return sel.code.to_string();
    }
    let Some(anchors) = resolve_anchors(sel, &lines) else {
        return flat_cut(&lines);
    };
    let line_count = lines.len() as i64;
    let (mut pad_before, mut pad_after) = (PAD_BEFORE, PAD_AFTER);
    let mut windows = build_windows(&anchors, line_count, pad_before, pad_after);
    while total_lines(&windows) > MAX_CODE_LINES as i64 && pad_after > MIN_PAD {
        pad_before = MIN_PAD.max(pad_before / 2);
        pad_after = MIN_PAD.max(pad_after / 2);
        windows = build_windows(&anchors, line_count, pad_before, pad_after);
    }
    render_windows(&lines, &windows)
}

#[cfg(test)]
pub(crate) mod code_window_test;
