//! What text a prompt shows — the ONE owner (docs/responsibility.md,
//! 2026-10-04). A prompt's surrounding code is chosen at graph time as
//! SPANS of the fresh text ([`Snippet`], [`CallSite`]) and printed at ASK
//! time under the names CURRENT then ([`FnPrinter`] over the rename
//! state): every name this run has already applied — waves, carries,
//! prior names alike — shows as applied. The identifiers the prompt asks
//! about stay under their minified names (that is what is asked), so the
//! printer is told to KEEP them.
//!
//! The refusal-outcomes investigation (2026-10-04) is why: the texts used
//! to be captured once, over the ORIGINAL names, and a module binding asked
//! after its neighbours were named was shown `var JT8 = W(() => { PT8 =
//! Co9; })` while the tree read `setAddPush = setDataEntry` — in 84% of the
//! borrowed-stem refusals the stem the model borrowed was already answered.
//!
//! An excerpt also always SHOWS what it is about ([`excerpt`]): a statement
//! longer than its cap keeps the lines around the mention instead of a
//! positional cut that can stop before it (2.1.197's env-var table cut off
//! the one line naming `x0u`).

use humanify_model::js::{is_js_whitespace, utf16_len, utf16_prefix};
use oxc_span::Span;

use super::generate::Replacement;
use super::render::FnPrinter;
use crate::rename::validated::scopes::BindingId;

const MAX_SNIPPET_CHARS: usize = 800;
const MAX_SNIPPET_LINES: usize = 10;
const MAX_DECLARATION_LINES: usize = 10;
const MAX_DECLARATION_CHARS: usize = 1000;
/// A call site's compact cap (`gatherCallSiteCode`).
const MAX_CALL_SITE_CHARS: usize = 200;
/// Below this a call site is expanded with up to two preceding siblings.
const CALL_SITE_EXPAND_BELOW: usize = 80;
/// Lines kept on each side of the mention when a long statement is
/// windowed.
const WINDOW_RADIUS: usize = 2;
/// The statement's first line, kept as a header above a window.
const HEADER_CHARS: usize = 120;
const ELIDED: &str = "  // ...";

/// How an excerpt is capped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    /// `capDeclarationText`: ten lines (`  // ...` after), 1000 chars.
    Declaration,
    /// `truncateSnippet`: ten lines, trimmed, 800 chars.
    Snippet,
    /// Uncapped (an import declaration).
    Whole,
}

/// One pretty snippet of the fresh text, printed at ask time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub span: Span,
    /// Layout edits a standalone print needs (a for-head declaration's
    /// declarators broken onto lines).
    pub layout: Vec<Replacement>,
    /// Babel's terminating `;` for a for-head declaration printed alone.
    pub semicolon: bool,
    /// The source offset of the occurrence the snippet is ABOUT (the
    /// declared identifier, the assignment, the reference) and its length.
    pub mention: Option<(u32, u32)>,
    pub cap: Cap,
}

impl Snippet {
    /// The snippet under the current names, `keep` under their originals.
    pub fn render(&self, p: &FnPrinter<'_, '_>, keep: &[BindingId]) -> String {
        let mut edits = p.occ.edits_except(p.view.text, p.state, self.span, keep);
        edits.extend(self.layout.iter().cloned());
        let mut code = p.view.pretty(self.span, &edits, true);
        if self.semicolon {
            code.push(';');
        }
        let mention = self.mention.map(|(at, len)| {
            let prefix = p.view.pretty(Span::new(self.span.start, at), &edits, true);
            // The mention's own text under the current names (a kept
            // identifier is its original; anything else may have grown).
            let own = p
                .view
                .pretty(Span::new(at, (at + len).min(self.span.end)), &edits, true);
            (prefix.len(), own.len())
        });
        excerpt(&code, mention, self.cap)
    }
}

/// One recorded call site of a function (`recordCallSite`): its statement,
/// compact, expanded with up to two preceding siblings when short.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub stmt: Span,
    pub semicolon: bool,
    /// The statement and up to two preceding siblings of its block, when
    /// it sits in one.
    pub siblings: Option<Vec<Span>>,
    /// The callee identifier (what the site is about).
    pub callee: Span,
}

impl CallSite {
    /// `gatherCallSiteCode` under the current names.
    pub fn render(&self, p: &FnPrinter<'_, '_>) -> String {
        let compact = |span: Span| p.view.compact_with(span, &p.edits(span));
        let mut code = compact(self.stmt);
        if self.semicolon {
            code.push(';');
        }
        let mut mention = compact(Span::new(self.stmt.start, self.callee.start)).len();
        let len = compact(self.callee).len();
        if utf16_len(&code) < CALL_SITE_EXPAND_BELOW
            && let Some(siblings) = &self.siblings
        {
            let lines: Vec<String> = siblings.iter().map(|s| compact(*s)).collect();
            let combined = lines.join("\n");
            if utf16_len(&combined) <= MAX_CALL_SITE_CHARS {
                let before: usize = lines[..lines.len() - 1].iter().map(|l| l.len() + 1).sum();
                mention += before;
                code = combined;
            }
        }
        if utf16_len(&code) <= MAX_CALL_SITE_CHARS {
            return code;
        }
        let cut = utf16_prefix(&code, MAX_CALL_SITE_CHARS - 3);
        // The call itself must survive the cut: past it, window the chars
        // around the call instead.
        if mention + len <= cut.len() {
            return format!("{cut}...");
        }
        char_window(
            &code,
            mention.min(code.len()),
            len,
            MAX_CALL_SITE_CHARS,
            "...",
        )
    }
}

/// `code` capped by `cap` — unless the cap would cut off the mention
/// (`(byte offset, byte length)` in `code`): then the lines around it,
/// under the statement's first line, within the same caps.
pub fn excerpt(code: &str, mention: Option<(usize, usize)>, cap: Cap) -> String {
    let (default, kept) = match cap {
        Cap::Whole => return code.to_string(),
        Cap::Declaration => cap_declaration(code),
        Cap::Snippet => match truncate_snippet(code) {
            Some(cut) => cut,
            None => return String::new(),
        },
    };
    let Some((at, len)) = mention else {
        return default;
    };
    if kept.start <= at && at + len <= kept.end {
        return default;
    }
    let (max_lines, max_chars) = match cap {
        Cap::Declaration => (MAX_DECLARATION_LINES, MAX_DECLARATION_CHARS),
        _ => (MAX_SNIPPET_LINES, MAX_SNIPPET_CHARS),
    };
    window(code, at, len, max_lines, max_chars)
}

/// `capDeclarationText`, plus the byte range of `code` it keeps.
fn cap_declaration(code: &str) -> (String, std::ops::Range<usize>) {
    let lines_end = nth_newline(code, MAX_DECLARATION_LINES).unwrap_or(code.len());
    let mut text = if lines_end < code.len() {
        format!("{}\n{ELIDED}", &code[..lines_end])
    } else {
        code.to_string()
    };
    let mut kept = lines_end;
    if utf16_len(&text) > MAX_DECLARATION_CHARS {
        let prefix = utf16_prefix(&text, MAX_DECLARATION_CHARS);
        kept = kept.min(prefix.len());
        text = format!("{prefix}…");
    }
    (text, 0..kept)
}

/// `truncateSnippet`, plus the byte range of `code` it keeps; None when
/// nothing is left.
fn truncate_snippet(code: &str) -> Option<(String, std::ops::Range<usize>)> {
    let lines_end = nth_newline(code, MAX_SNIPPET_LINES).unwrap_or(code.len());
    let joined = &code[..lines_end];
    let start = joined.len() - joined.trim_start_matches(is_js_whitespace).len();
    let trimmed = joined.trim_matches(is_js_whitespace);
    if trimmed.is_empty() {
        return None;
    }
    let mut end = start + trimmed.len();
    let mut snippet = trimmed.to_string();
    if utf16_len(trimmed) > MAX_SNIPPET_CHARS {
        let prefix = utf16_prefix(trimmed, MAX_SNIPPET_CHARS);
        end = start + prefix.len();
        snippet = format!("{prefix}…");
    }
    Some((snippet, start..end))
}

/// The byte offset of the `n`-th `\n` (1-based), if there is one.
fn nth_newline(code: &str, n: usize) -> Option<usize> {
    code.match_indices('\n').nth(n - 1).map(|(i, _)| i)
}

/// The lines around the mention, under the statement's first line (cut
/// to [`HEADER_CHARS`]), within `max_lines` / `max_chars` — narrowing to
/// the mention's own line, then to the chars around it.
fn window(code: &str, at: usize, len: usize, max_lines: usize, max_chars: usize) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    let m = code[..at].matches('\n').count();
    let last = lines.len() - 1;
    for radius in (0..=WINDOW_RADIUS).rev() {
        let mut parts: Vec<String> = Vec::new();
        let first = if m > 0 {
            parts.push(header(lines[0]));
            1
        } else {
            0
        };
        let lo = m.saturating_sub(radius).max(first);
        let hi = (m + radius).min(last);
        if lo > first {
            parts.push(ELIDED.to_string());
        }
        parts.extend(lines[lo..=hi].iter().map(|l| (*l).to_string()));
        if hi < last {
            parts.push(ELIDED.to_string());
        }
        let text = parts.join("\n");
        if parts.len() <= max_lines + 2 && utf16_len(&text) <= max_chars {
            return text;
        }
    }
    // One line too long to show whole: the chars around the mention.
    let line_start = code[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = code[at..].find('\n').map_or(code.len(), |i| at + i);
    char_window(
        &code[line_start..line_end],
        at - line_start,
        len,
        max_chars,
        "…",
    )
}

/// The statement's first line, cut to [`HEADER_CHARS`].
fn header(line: &str) -> String {
    if utf16_len(line) <= HEADER_CHARS {
        line.to_string()
    } else {
        format!("{}…", utf16_prefix(line, HEADER_CHARS))
    }
}

/// At most `max_chars` (UTF-16 units, the elision marks included) of
/// `text` centred on `[at, at + len)`.
fn char_window(text: &str, at: usize, len: usize, max_chars: usize, mark: &str) -> String {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let mut end = (at + len).min(text.len());
    while !text.is_char_boundary(end) {
        end += 1;
    }
    let len = end - at;
    let budget = max_chars.saturating_sub(2 * utf16_len(mark)).max(len);
    let side = budget.saturating_sub(len) / 2;
    // Right side first; what it cannot use goes to the left.
    let (hi, right) = extend(&text[end..], side, false);
    let hi = end + hi;
    let (back, _) = extend(&text[..at], budget.saturating_sub(len) - right, true);
    let lo = at - back;
    let mut out = String::new();
    if lo > 0 {
        out.push_str(mark);
    }
    out.push_str(&text[lo..hi]);
    if hi < text.len() {
        out.push_str(mark);
    }
    out
}

/// How many bytes of `text` fit `units` UTF-16 units, read from its start
/// (or its end, `backward`), and the units they take.
fn extend(text: &str, units: usize, backward: bool) -> (usize, usize) {
    let mut used = 0;
    let mut bytes = 0;
    let chars: Box<dyn Iterator<Item = char>> = if backward {
        Box::new(text.chars().rev())
    } else {
        Box::new(text.chars())
    };
    for ch in chars {
        if used + ch.len_utf16() > units {
            break;
        }
        used += ch.len_utf16();
        bytes += ch.len_utf8();
    }
    (bytes, used)
}

#[cfg(test)]
mod prompt_text_test;
