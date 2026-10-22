//! Name evidence (exp136): a source map can credit a unit to a file that
//! does not hold it (Bun 1.4 maps a module's code to another module's
//! file, exp132/exp135: the truth side's map labelled an encoding table
//! another package's `index.js`, and the index side's map files code under
//! the wrong file and package). What no minifier renames — string contents
//! and property names — says whether the file holds the unit.

use std::collections::BTreeSet;

/// Property and string names nearly every file holds: no evidence of WHICH
/// file a unit came from.
const COMMON: &[&str] = &[
    "length",
    "prototype",
    "call",
    "apply",
    "bind",
    "push",
    "pop",
    "shift",
    "unshift",
    "slice",
    "splice",
    "concat",
    "join",
    "split",
    "indexOf",
    "includes",
    "forEach",
    "map",
    "filter",
    "reduce",
    "some",
    "every",
    "keys",
    "values",
    "entries",
    "assign",
    "create",
    "freeze",
    "defineProperty",
    "getOwnPropertyNames",
    "getOwnPropertyDescriptor",
    "hasOwnProperty",
    "toString",
    "valueOf",
    "constructor",
    "name",
    "type",
    "value",
    "data",
    "default",
    "exports",
    "module",
    "then",
    "catch",
    "finally",
    "resolve",
    "reject",
    "done",
    "next",
    "return",
    "throw",
    "charCodeAt",
    "charAt",
    "substring",
    "substr",
    "replace",
    "match",
    "test",
    "exec",
    "trim",
    "toLowerCase",
    "toUpperCase",
    "startsWith",
    "endsWith",
    "isArray",
    "string",
    "number",
    "object",
    "function",
    "boolean",
    "undefined",
    "symbol",
    "bigint",
    "message",
    "stack",
    "error",
    "code",
    "options",
    "args",
    "start",
    "end",
    "size",
    "from",
    "emit",
    "listeners",
    "parent",
    "node",
    "props",
    "state",
    "self",
    "window",
    "document",
    "process",
    "env",
];

/// A unit's name evidence: the contents of its string literals (4..=200
/// characters, three letters in a row, no escape) and its `.property`
/// names of four or more characters, less [`COMMON`]. Read with a small
/// lexer: quotes pair up, comments are skipped (a regex literal can still
/// fool it; rare in bundled code, and it only adds tokens).
pub fn name_evidence(text: &str) -> BTreeSet<&str> {
    let b = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'' | b'`') => {
                let mut j = i + 1;
                while j < b.len() && b[j] != q {
                    if b[j] == b'\\' {
                        j += 1;
                    } else if q == b'`' && b[j] == b'$' && b.get(j + 1) == Some(&b'{') {
                        break;
                    }
                    j += 1;
                }
                let j = j.min(b.len());
                if let Some(s) = text.get(i + 1..j)
                    && (4..=200).contains(&s.chars().count())
                    && !s.contains('\\')
                    && has_word(s)
                {
                    out.insert(s);
                }
                i = j + 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = text[i..].find('\n').map_or(b.len(), |n| i + n);
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
            }
            b'.' => {
                let s = i + 1;
                let mut j = s;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'$')
                {
                    j += 1;
                }
                if j - s >= 4 && !b[s].is_ascii_digit() {
                    out.insert(&text[s..j]);
                }
                i = j.max(i + 1);
            }
            _ => i += 1,
        }
    }
    out.retain(|t| !COMMON.contains(t));
    out
}

/// Three ASCII letters in a row.
fn has_word(s: &str) -> bool {
    let mut run = 0;
    for c in s.chars() {
        run = if c.is_ascii_alphabetic() { run + 1 } else { 0 };
        if run >= 3 {
            return true;
        }
    }
    false
}

/// Does `file` hold the unit whose evidence is `toks` (at least half of
/// it)? None when there are under three tokens: too little to say.
pub fn file_holds(file: &str, toks: &BTreeSet<&str>) -> Option<bool> {
    if toks.len() < 3 {
        return None;
    }
    let held = toks.iter().filter(|t| file.contains(**t)).count();
    Some(2 * held >= toks.len())
}
