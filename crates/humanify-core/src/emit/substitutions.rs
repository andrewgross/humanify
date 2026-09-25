//! Positional text substitutions over a file's lines — TS
//! `src/split/substitutions.ts`, the one owner of "rewrite these (line,
//! col) occurrences in TEXT" (bundle-carry and post-split reconcile, WP5.4,
//! are its consumers). The guard — skip a second substitution at the same
//! position — is the unified behaviour: a second splice at one position
//! would land inside the first replacement's text.
//!
//! Columns are JS string indices (UTF-16 code units), as the TS producers
//! record them from Babel `loc`; `from`'s length is its UTF-16 length.

/// One occurrence to rewrite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Substitution {
    /// 1-based line.
    pub line: usize,
    /// 0-based column, in UTF-16 code units.
    pub col: usize,
    pub from: String,
    pub to: String,
}

/// The byte offset of UTF-16 index `units` in `s` (clamped to the end, as
/// `String.prototype.slice` clamps).
fn byte_at_utf16(s: &str, units: usize) -> usize {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        if used >= units {
            return i;
        }
        used += c.len_utf16();
    }
    s.len()
}

/// `applySubstitutions(lines, subs)`: the lines joined by `\n` after every
/// substitution, right-to-left within a line.
pub fn apply_substitutions(lines: &[String], subs: &[Substitution]) -> String {
    // `byLine` is a Map in insertion order; each line is rewritten
    // independently, so the order lines are visited cannot be observed.
    let mut by_line: Vec<(usize, Vec<&Substitution>)> = Vec::new();
    for sub in subs {
        match by_line.iter_mut().find(|(l, _)| *l == sub.line) {
            Some((_, list)) => list.push(sub),
            None => by_line.push((sub.line, vec![sub])),
        }
    }
    let mut out: Vec<String> = lines.to_vec();
    for (line_no, mut list) in by_line {
        // Stable, descending by column (`(a, b) => b.col - a.col`).
        list.sort_by_key(|sub| std::cmp::Reverse(sub.col));
        let mut text = out[line_no - 1].clone();
        let mut previous_col = usize::MAX;
        for sub in list {
            if sub.col >= previous_col {
                continue; // same position twice: skip
            }
            let start = byte_at_utf16(&text, sub.col);
            let from_units = sub.from.encode_utf16().count();
            let end = byte_at_utf16(&text, sub.col + from_units);
            text = format!("{}{}{}", &text[..start], sub.to, &text[end..]);
            previous_col = sub.col;
        }
        out[line_no - 1] = text;
    }
    out.join("\n")
}

#[cfg(test)]
mod substitutions_test;
