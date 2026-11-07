//! The concurrency census's reading of one Rust file: blank comments,
//! string/char literals and `use` declarations; blank inline
//! `#[cfg(test)]` items and collect the out-of-line test modules; then
//! key every construct occurrence into a site (file, scope, construct).
//!
//! A faithful port of scripts/concurrency-census.ts's scanner, not a Rust
//! parser: the allowlist's `scope` strings are what THIS scanner names,
//! byte for byte (e.g. `Type::fn`, `static X`, `impl X`, `<module>`), so
//! a parser that named scopes differently would re-key the allowlist.
//! Offsets are UTF-16 units like the TS: every non-ASCII char becomes as
//! many `_` as it has UTF-16 units, so the char-literal window and the
//! line numbers agree with the old census exactly.

use super::rules::{CONSTRUCTS, boundary, is_space, is_word, matches};

/// One site: several uses of one construct in one scope are ONE site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub file: String,
    pub scope: String,
    pub construct: String,
    /// 1-based lines of its uses, first-seen order.
    pub lines: Vec<usize>,
}

/// An out-of-line `#[cfg(test)] mod name;` with its `#[path]`, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestModule {
    pub name: String,
    pub path_attr: Option<String>,
}

/// `src` with every char as ASCII (`_` per UTF-16 unit of a non-ASCII one).
pub fn ascii_units(src: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    for c in src.chars() {
        if c.is_ascii() {
            out.push(c as u8);
        } else {
            out.extend(std::iter::repeat_n(b'_', c.len_utf16()));
        }
    }
    out
}

/// Blank `[from, to)` (newlines kept, so offsets and lines hold).
fn blank(out: &mut [u8], from: usize, to: usize) {
    let to = to.min(out.len());
    for b in out.iter_mut().take(to).skip(from) {
        if *b != b'\n' {
            *b = b' ';
        }
    }
}

fn find(s: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > s.len() {
        return None;
    }
    s[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// End (exclusive) of a raw string starting at `i` (`b?r#*"`), if one does.
fn raw_string_end(s: &[u8], i: usize) -> Option<usize> {
    let window = (i + 260).min(s.len());
    let w = &s[i..window];
    let mut j = 0;
    if w.first() == Some(&b'b') {
        j += 1;
    }
    if w.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hashes = w[j..].iter().take_while(|b| **b == b'#').count();
    j += hashes;
    if w.get(j) != Some(&b'"') {
        return None;
    }
    let mut close = vec![b'"'];
    close.extend(std::iter::repeat_n(b'#', hashes));
    Some(find(s, &close, i + j + 1).map_or(s.len(), |e| e + close.len()))
}

/// End (exclusive) of a quoted string whose `"` is at `i`.
fn quoted_end(s: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < s.len() && s[j] != b'"' {
        j += if s[j] == b'\\' { 2 } else { 1 };
    }
    j + 1
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// End of a char literal at `i` (the `'`), or None for a lifetime: the TS
/// regex `^'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'` over a
/// 12-unit window.
fn char_end(s: &[u8], i: usize) -> Option<usize> {
    let w = &s[i..(i + 12).min(s.len())];
    let at = |k: usize| w.get(k).copied();
    let closes = |k: usize| (at(k) == Some(b'\'')).then_some(i + k + 1);
    match at(1)? {
        b'\\' => {
            let hex2 =
                at(2) == Some(b'x') && at(3).is_some_and(is_hex) && at(4).is_some_and(is_hex);
            if let Some(e) = hex2.then(|| closes(5)).flatten() {
                return Some(e);
            }
            if at(2) == Some(b'u') && at(3) == Some(b'{') {
                let n = w[4.min(w.len())..]
                    .iter()
                    .take_while(|b| is_hex(**b))
                    .count();
                if n > 0
                    && at(4 + n) == Some(b'}')
                    && let Some(e) = closes(5 + n)
                {
                    return Some(e);
                }
            }
            let any = at(2).is_some_and(|b| b != b'\n' && b != b'\r');
            any.then(|| closes(3)).flatten()
        }
        b'\'' => None,
        _ => closes(2),
    }
}

/// End of a (nestable) block comment starting at `i`.
fn block_comment_end(s: &[u8], i: usize) -> usize {
    let mut depth = 0usize;
    let mut j = i;
    while j < s.len() {
        if s[j..].starts_with(b"/*") {
            depth += 1;
            j += 2;
        } else if s[j..].starts_with(b"*/") {
            depth = depth.saturating_sub(1);
            j += 2;
            if depth == 0 {
                return j;
            }
        } else {
            j += 1;
        }
    }
    j
}

fn comment_end(s: &[u8], i: usize) -> Option<usize> {
    if s[i] != b'/' {
        return None;
    }
    match s.get(i + 1) {
        Some(b'/') => Some(find(s, b"\n", i).unwrap_or(s.len())),
        Some(b'*') => Some(block_comment_end(s, i)),
        _ => None,
    }
}

fn literal_end(s: &[u8], i: usize) -> Option<usize> {
    let c = s[i];
    let prev_ident = i > 0 && is_word(s[i - 1]);
    if (c == b'r' || c == b'b') && !prev_ident {
        if let Some(e) = raw_string_end(s, i) {
            return Some(e);
        }
        if c == b'b' && s.get(i + 1) == Some(&b'"') {
            return Some(quoted_end(s, i + 1));
        }
    }
    match c {
        b'"' => Some(quoted_end(s, i)),
        b'\'' => char_end(s, i),
        _ => None,
    }
}

/// Blank `use` declarations: the TS regex
/// `/(^|[;{}]\s*)(?:pub(?:\([^)]*\))?\s+)?use\s[^;]*;/gm`, matched on
/// `text`, blanking `out` from after group 1 to the `;`.
fn blank_uses(text: &[u8], out: &mut [u8]) {
    let mut p = 0;
    while p < text.len() {
        match use_decl_at(text, p) {
            Some((from, end)) => {
                blank(out, from, end);
                p = end;
            }
            None => p += 1,
        }
    }
}

/// A `use` declaration matched at `p`: (blank-from, end).
fn use_decl_at(t: &[u8], p: usize) -> Option<(usize, usize)> {
    let line_start = p == 0 || matches!(t[p - 1], b'\n' | b'\r');
    if line_start && let Some(end) = use_body(t, p) {
        return Some((p, end));
    }
    if !matches!(t[p], b';' | b'{' | b'}') {
        return None;
    }
    let from = skip_ws(t, p + 1);
    use_body(t, from).map(|end| (from, end))
}

fn skip_ws(t: &[u8], mut i: usize) -> usize {
    while i < t.len() && is_space(t[i]) {
        i += 1;
    }
    i
}

/// `(?:pub(?:\([^)]*\))?\s+)?use\s[^;]*;` at `i`: its end.
fn use_body(t: &[u8], i: usize) -> Option<usize> {
    let after_pub = pub_prefix(t, i);
    for start in after_pub.into_iter().chain(std::iter::once(i)) {
        if t[start..].starts_with(b"use") && t.get(start + 3).is_some_and(|b| is_space(*b)) {
            let semi = find(t, b";", start + 4)?;
            return Some(semi + 1);
        }
    }
    None
}

/// `pub(?:\([^)]*\))?\s+` at `i`: where it ends.
fn pub_prefix(t: &[u8], i: usize) -> Option<usize> {
    if !t[i..].starts_with(b"pub") {
        return None;
    }
    let mut j = i + 3;
    if t.get(j) == Some(&b'(') {
        let close = t[j..].iter().position(|b| *b == b')')?;
        let with_vis = j + close + 1;
        if t.get(with_vis).is_some_and(|b| is_space(*b)) {
            j = with_vis;
        }
    }
    let k = skip_ws(t, j);
    (k > j).then_some(k)
}

/// Source with comments, string/char literals and `use` declarations
/// blanked.
pub fn sanitize(src: &str) -> Vec<u8> {
    let ascii = ascii_units(src);
    let mut out = ascii.clone();
    let mut i = 0;
    while i < ascii.len() {
        match comment_end(&ascii, i).or_else(|| literal_end(&ascii, i)) {
            Some(end) => {
                blank(&mut out, i, end);
                i = end;
            }
            None => i += 1,
        }
    }
    let text = out.clone();
    blank_uses(&text, &mut out);
    out
}

/// Index of the brace matching the `{` at `open`.
fn matching_brace(t: &[u8], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, b) in t.iter().enumerate().skip(open) {
        if *b == b'{' {
            depth += 1;
        } else if *b == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return i;
            }
        }
    }
    t.len().saturating_sub(1)
}

/// Skip whitespace and `#[...]` attributes from `i`: the item start.
fn skip_attributes(t: &[u8], i: usize) -> usize {
    let mut j = i;
    loop {
        j = skip_ws(t, j);
        if !t[j.min(t.len())..].starts_with(b"#[") {
            return j;
        }
        let mut depth = 0usize;
        while j < t.len() {
            if t[j] == b'[' {
                depth += 1;
            } else if t[j] == b']' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            j += 1;
        }
        j += 1;
    }
}

/// `^(pub(\([^)]*\))?\s+)?mod\s+(\w+)\s*;` over `decl` (which ends at
/// the first `;`): the module name.
fn mod_decl(decl: &[u8]) -> Option<String> {
    let starts = pub_prefix(decl, 0).into_iter().chain(std::iter::once(0));
    for s in starts {
        if !decl[s..].starts_with(b"mod") {
            continue;
        }
        let n = skip_ws(decl, s + 3);
        if n == s + 3 {
            continue;
        }
        let e = decl[n..].iter().take_while(|b| is_word(**b)).count() + n;
        if e > n && decl.get(skip_ws(decl, e)) == Some(&b';') {
            return Some(String::from_utf8_lossy(&decl[n..e]).into_owned());
        }
    }
    None
}

/// `#\[path\s*=\s*"([^"]+)"\]` in `attrs`.
fn path_attr(attrs: &str) -> Option<String> {
    let b = attrs.as_bytes();
    let mut from = 0;
    while let Some(at) = find(b, b"#[path", from) {
        from = at + 1;
        let eq = skip_ws(b, at + 6);
        if b.get(eq) != Some(&b'=') {
            continue;
        }
        let q = skip_ws(b, eq + 1);
        if b.get(q) != Some(&b'"') {
            continue;
        }
        let len = b[q + 1..].iter().take_while(|c| **c != b'"').count();
        if len > 0 && b.get(q + 1 + len + 1) == Some(&b']') {
            return Some(attrs[q + 1..q + 1 + len].to_string());
        }
    }
    None
}

/// Blank every inline `#[cfg(test)]` item of `text` (sanitized); return
/// the blanked text and the out-of-line test modules (`mod x;`) with any
/// `#[path]` they carry (read from `raw_units`, the raw UTF-16 source).
pub fn strip_cfg_test(text: &[u8], raw_units: &[u16]) -> (Vec<u8>, Vec<TestModule>) {
    const ATTR: &[u8] = b"#[cfg(test)]";
    let mut out = text.to_vec();
    let mut modules = Vec::new();
    let mut from = 0;
    while let Some(at) = find(text, ATTR, from) {
        from = at + ATTR.len();
        let start = skip_attributes(text, from);
        let semi = find(text, b";", start);
        let brace = find(text, b"{", start);
        match (semi, brace) {
            (Some(semi), b) if b.is_none_or(|b| semi < b) => {
                if let Some(name) = mod_decl(&text[start..=semi]) {
                    let attrs =
                        String::from_utf16_lossy(&raw_units[at..start.min(raw_units.len())]);
                    modules.push(TestModule {
                        name,
                        path_attr: path_attr(&attrs),
                    });
                }
                blank(&mut out, at, semi + 1);
            }
            (_, Some(brace)) => blank(&mut out, at, matching_brace(text, brace) + 1),
            _ => {}
        }
    }
    (out, modules)
}

// ─── scopes ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Fn,
    Impl,
    Item,
    Block,
}

#[derive(Clone, Debug)]
struct Frame {
    kind: Kind,
    name: String,
}

/// `\b<kw> (\w+)` at `p`: the identifier.
fn kw_ident(s: &[u8], p: usize, kw: &[u8]) -> Option<String> {
    if !s[p..].starts_with(kw) || s.get(p + kw.len()) != Some(&b' ') || !boundary(s, p) {
        return None;
    }
    let from = p + kw.len() + 1;
    let n = s[from..].iter().take_while(|b| is_word(**b)).count();
    (n > 0).then(|| String::from_utf8_lossy(&s[from..from + n]).into_owned())
}

/// The leftmost `\b(kw1|kw2|...) (\w+)` in `s`: (offset, keyword, ident).
fn leftmost_kw(s: &[u8], kws: &[&'static str]) -> Option<(usize, &'static str, String)> {
    (0..s.len()).find_map(|p| {
        kws.iter()
            .find_map(|kw| kw_ident(s, p, kw.as_bytes()).map(|id| (p, *kw, id)))
    })
}

fn collapse_ws(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &b in s {
        if is_space(b) {
            if out.last() != Some(&b' ') {
                out.push(b' ');
            }
        } else {
            out.push(b);
        }
    }
    out
}

/// What a `{` opens, from the text since the previous `;`, `{` or `}`.
fn classify_header(header: &[u8]) -> Frame {
    let h = collapse_ws(header);
    if let Some((_, _, name)) = leftmost_kw(&h, &["fn"]) {
        return Frame {
            kind: Kind::Fn,
            name,
        };
    }
    if let Some(target) = impl_target(&h) {
        return Frame {
            kind: Kind::Impl,
            name: impl_name(&target),
        };
    }
    if let Some((p, kw, id)) = leftmost_kw(&h, &["struct", "enum", "trait", "union", "mod"]) {
        let whole = format!("{kw} {id}");
        let first = find(&h, whole.as_bytes(), 0).unwrap_or(p);
        if !h[..first].iter().any(|b| *b == b'=' || *b == b'(') {
            return Frame {
                kind: Kind::Item,
                name: whole,
            };
        }
    }
    Frame {
        kind: Kind::Block,
        name: String::new(),
    }
}

/// `^\s*(unsafe )?impl\b(?:\s*<[^{]*?>)?\s+(.*)$` over a collapsed
/// header: group 2.
fn impl_target(h: &[u8]) -> Option<Vec<u8>> {
    let mut p = skip_ws(h, 0);
    if h[p..].starts_with(b"unsafe ") && h[p + 7..].starts_with(b"impl") {
        p += 7;
    }
    if !h[p..].starts_with(b"impl") || !boundary(h, p + 4) {
        return None;
    }
    p += 4;
    let rest_after = |q: usize| {
        let r = skip_ws(h, q);
        (r > q).then(|| h[r..].to_vec())
    };
    let g = skip_ws(h, p);
    if h.get(g) == Some(&b'<') {
        for (k, &b) in h.iter().enumerate().skip(g + 1) {
            if b == b'{' {
                break;
            }
            if b == b'>'
                && let Some(rest) = rest_after(k + 1)
            {
                return Some(rest);
            }
        }
    }
    rest_after(p)
}

/// Remove `<[^<>]*(<[^<>]*>[^<>]*)*>` everywhere (one nesting level).
fn strip_generics(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'<'
            && let Some(end) = generic_end(s, i)
        {
            i = end;
            continue;
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

fn generic_end(s: &[u8], open: usize) -> Option<usize> {
    let plain = |mut j: usize| {
        while j < s.len() && s[j] != b'<' && s[j] != b'>' {
            j += 1;
        }
        j
    };
    let mut j = plain(open + 1);
    loop {
        match s.get(j) {
            Some(b'>') => return Some(j + 1),
            Some(b'<') => {
                let k = plain(j + 1);
                if s.get(k) != Some(&b'>') {
                    return None;
                }
                j = plain(k + 1);
            }
            _ => return None,
        }
    }
}

/// The impl's type name: the TS's ` for ` split, generics stripped, then
/// `(\w+)\s*(<.*)?\s*(where\b.*)?$` leftmost; `?` when nothing matches.
fn impl_name(target: &[u8]) -> String {
    let t = String::from_utf8_lossy(target).into_owned();
    let last = t.rsplit(" for ").next().unwrap_or("");
    let stripped = strip_generics(last.as_bytes());
    let s = String::from_utf8_lossy(&stripped)
        .trim()
        .as_bytes()
        .to_vec();
    for p in 0..s.len() {
        let run = s[p..].iter().take_while(|b| is_word(**b)).count();
        for e in (p + 1..=p + run).rev() {
            if name_tail_ok(&s, e) {
                return String::from_utf8_lossy(&s[p..e]).into_owned();
            }
        }
    }
    "?".to_string()
}

/// `\s*(<.*)?\s*(where\b.*)?$` at `e`.
fn name_tail_ok(s: &[u8], e: usize) -> bool {
    let q = skip_ws(s, e);
    q == s.len()
        || s[q] == b'<'
        || (s[q..].starts_with(b"where") && (q + 5 == s.len() || !is_word(s[q + 5])))
}

/// The scope of a construct, given the open frames and the text of its
/// statement so far.
fn scope_of(stack: &[Frame], statement: &[u8]) -> String {
    let mut fns: Vec<&str> = stack
        .iter()
        .filter(|f| f.kind == Kind::Fn)
        .map(|f| f.name.as_str())
        .collect();
    // A construct in a signature belongs to the function it declares.
    let signature = leftmost_kw(statement, &["fn"]).map(|(_, _, id)| id);
    if let Some(sig) = &signature {
        fns.push(sig);
    }
    if !fns.is_empty() {
        let first_fn = stack
            .iter()
            .position(|f| f.kind == Kind::Fn)
            .unwrap_or(stack.len());
        let owner = stack[..first_fn]
            .iter()
            .rev()
            .find(|f| matches!(f.kind, Kind::Impl | Kind::Item));
        let prefix = owner.map_or(String::new(), |o| format!("{}::", o.name));
        return prefix + &fns.join("::");
    }
    if let Some(top) = stack.iter().rev().find(|f| f.kind != Kind::Block) {
        return if top.kind == Kind::Impl {
            format!("impl {}", top.name)
        } else {
            top.name.clone()
        };
    }
    let item = leftmost_kw(
        statement,
        &["static", "const", "type", "struct", "enum", "fn"],
    );
    item.map_or("<module>".to_string(), |(_, kw, id)| format!("{kw} {id}"))
}

/// Every construct occurrence in one sanitized file, keyed into sites
/// (first-seen order).
pub fn sites_of(file: &str, text: &[u8]) -> Vec<Site> {
    let mut hits: Vec<(usize, &str)> = CONSTRUCTS
        .iter()
        .flat_map(|c| matches(text, c).into_iter().map(move |at| (at, c.id)))
        .collect();
    hits.sort_by_key(|h| h.0);
    let mut sites: Vec<Site> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut boundary_at = 0;
    let mut line = 1;
    let mut h = 0;
    let mut i = 0;
    while i <= text.len() && h < hits.len() {
        while h < hits.len() && hits[h].0 == i {
            let scope = scope_of(&stack, &text[boundary_at..i]);
            add_site(&mut sites, file, scope, hits[h].1, line);
            h += 1;
        }
        match text.get(i) {
            Some(b'\n') => line += 1,
            Some(b'{') => stack.push(classify_header(&text[boundary_at..i])),
            Some(b'}') => {
                stack.pop();
            }
            _ => {}
        }
        if matches!(text.get(i), Some(b'{' | b'}' | b';')) {
            boundary_at = i + 1;
        }
        i += 1;
    }
    sites
}

fn add_site(sites: &mut Vec<Site>, file: &str, scope: String, construct: &str, line: usize) {
    match sites
        .iter_mut()
        .find(|s| s.scope == scope && s.construct == construct)
    {
        Some(s) => {
            if !s.lines.contains(&line) {
                s.lines.push(line);
            }
        }
        None => sites.push(Site {
            file: file.to_string(),
            scope,
            construct: construct.to_string(),
            lines: vec![line],
        }),
    }
}

#[cfg(test)]
#[path = "scan_test.rs"]
mod scan_test;
