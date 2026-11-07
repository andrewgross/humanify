//! The few Python `re` patterns the library scorer and its layer builders
//! match, written out by hand over Python's own character classes
//! (`pyclass_table`), so a match is the one Python's `re` makes: `\w` is
//! `isalnum() or '_'`, `\d` is `isdecimal()`, `\s` is `isspace()` — for
//! every code point, at Python 3.11's Unicode version.

use super::pyclass_table::{ALNUM, DECIMAL, SPACE};

fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    let i = table.partition_point(|&(_, hi)| hi < c);
    i < table.len() && table[i].0 <= c
}

/// Python `\w` (str pattern): `isalnum() or '_'`.
pub fn is_word(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric() || c == '_'
    } else {
        in_table(ALNUM, c)
    }
}

/// Python `\d` (str pattern): `isdecimal()`.
pub fn is_digit(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_digit()
    } else {
        in_table(DECIMAL, c)
    }
}

/// Python `\s` (str pattern): `isspace()`.
pub fn is_space(c: char) -> bool {
    if c.is_ascii() {
        matches!(
            c,
            ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1f'
        )
    } else {
        in_table(SPACE, c)
    }
}

/// `re.findall(r"[A-Za-z_$][\w$]{3,}|\d{3,}", text)` (truth_audit `TOKEN`):
/// identifier-like words of at least four characters and runs of at least
/// three decimal digits, leftmost first, never overlapping.
pub fn tokens(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_of = |k: usize| chars.get(k).map_or(text.len(), |c| c.0);
    let mut out = Vec::new();
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k].1;
        if c.is_ascii_alphabetic() || c == '_' || c == '$' {
            let mut j = k + 1;
            while j < chars.len() && (is_word(chars[j].1) || chars[j].1 == '$') {
                j += 1;
            }
            if j - k > 3 {
                out.push(&text[chars[k].0..end_of(j)]);
                k = j;
                continue;
            }
        }
        if is_digit(c) {
            let mut j = k + 1;
            while j < chars.len() && is_digit(chars[j].1) {
                j += 1;
            }
            if j - k >= 3 {
                out.push(&text[chars[k].0..end_of(j)]);
                k = j;
                continue;
            }
        }
        k += 1;
    }
    out
}

/// `re.search(r"\?commonjs-[a-z-]+$", name)` (truth_audit `VIRTUAL`): a
/// rollup CommonJS-plugin virtual id. `$` also matches before one final
/// newline, as Python's does.
pub fn is_commonjs_virtual(name: &str) -> bool {
    let body = name.strip_suffix('\n').unwrap_or(name);
    let mut search = body;
    while let Some(at) = search.find("?commonjs-") {
        let rest = &search[at + "?commonjs-".len()..];
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
            return true;
        }
        search = &search[at + 1..];
    }
    false
}

fn not_space(c: char) -> bool {
    !is_space(c)
}

/// origins.py `VENDOR_DIR.search(inside)`: the package directory under a
/// vendored directory (`.../third_party/<pkg>/...`), leftmost match.
pub fn vendor_dir(inside: &str) -> Option<&str> {
    const DIRS: [&str; 7] = [
        "third_party",
        "third-party",
        "vendor",
        "vendored",
        "_vendor",
        "bundled",
        "external",
    ];
    for (i, _) in inside.char_indices() {
        if i > 0 && inside.as_bytes()[i - 1] != b'/' {
            continue;
        }
        let rest = &inside[i..];
        let Some(after) = DIRS
            .iter()
            .find_map(|d| rest.strip_prefix(d).and_then(|r| r.strip_prefix('/')))
        else {
            continue;
        };
        if let Some(g) = package_dir(after) {
            return Some(g);
        }
    }
    None
}

/// `(@[^/]+/[^/]+|[^/@][^/]*)/` at the start of `s`: the group.
fn package_dir(s: &str) -> Option<&str> {
    let end = if let Some(rest) = s.strip_prefix('@') {
        let a = rest.find('/')?;
        if a == 0 {
            return None;
        }
        let b = rest[a + 1..].find('/')?;
        if b == 0 {
            return None;
        }
        1 + a + 1 + b
    } else {
        if s.starts_with('/') || s.starts_with('@') || s.is_empty() {
            return None;
        }
        s.find('/')?
    };
    Some(&s[..end])
}

/// origins.py `NODE_MODULES_PKG.findall(path)`:
/// `node_modules/(@[^/\s]+/[^/\s]+|[^/@\s][^/\s]*)/`, every match.
pub fn node_modules_pkgs(path: &str) -> Vec<&str> {
    const NM: &str = "node_modules/";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = path[from..].find(NM) {
        let start = from + at + NM.len();
        let s = &path[start..];
        let seg = |x: &str| -> Option<usize> {
            let n = x.find(|c: char| c == '/' || is_space(c)).unwrap_or(x.len());
            (n > 0).then_some(n)
        };
        let got = if let Some(rest) = s.strip_prefix('@') {
            seg(rest).and_then(|a| {
                (rest[a..].starts_with('/')).then_some(())?;
                let b = seg(&rest[a + 1..])?;
                rest[a + 1 + b..].starts_with('/').then_some(1 + a + 1 + b)
            })
        } else {
            match s.chars().next() {
                Some(c) if c != '/' && c != '@' && not_space(c) => {
                    seg(s).and_then(|n| s[n..].starts_with('/').then_some(n))
                }
                _ => None,
            }
        };
        match got {
            Some(n) => {
                out.push(&s[..n]);
                from = start + n + 1;
            }
            None => from = from + at + 1,
        }
    }
    out
}

/// A run of `\S+` from the start of `s`: its byte length (0 if none).
fn non_space_run(s: &str) -> usize {
    s.find(is_space).unwrap_or(s.len())
}

/// origins.py `MARKERS`: the path a module-marker line names, if it is one
/// (esbuild `// p`, rolldown `//#region p`, webpack `/***/ "p"`, webpack's
/// banner `/*!***...!*** p *`), tried in that order.
pub fn marker_path(line: &str) -> Option<&str> {
    for prefix in ["// ", "//#region "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let n = non_space_run(rest);
            if n > 0 && n == rest.len() {
                return Some(rest);
            }
        }
    }
    if let Some(rest) = line.strip_prefix("/***/ \"") {
        let run = &rest[..non_space_run(rest)];
        if let Some(q) = run.rfind('"')
            && q > 0
        {
            return Some(&run[..q]);
        }
    }
    if let Some(rest) = line.strip_prefix("/*!") {
        let stars = rest.len() - rest.trim_start_matches('*').len();
        if stars > 0 {
            let rest = rest[stars..].trim_start_matches(is_space);
            if let Some(rest) = rest.strip_prefix("!*** ") {
                let n = non_space_run(rest);
                if n > 0 && rest[n..].starts_with(" *") {
                    return Some(&rest[..n]);
                }
            }
        }
    }
    None
}

const EXTS: [&str; 12] = [
    "js", "jsx", "ts", "tsx", "cjs", "cjsx", "cts", "ctsx", "mjs", "mjsx", "mts", "mtsx",
];

/// origins.py `PATHLIKE.match(p)` (`patched`: scorer 12's `PATHLIKE12`,
/// which allows `=` after the first segment).
pub fn pathlike(p: &str, patched: bool) -> bool {
    let Some((head, ext)) = p.rsplit_once('.') else {
        return false;
    };
    if !(EXTS.contains(&ext) || ext == "json") {
        return false;
    }
    let first = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| is_word(c) || matches!(c, '@' | '.' | '-'))
    };
    let later = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| is_word(c) || matches!(c, '@' | '.' | '+' | '-') || (patched && c == '='))
    };
    let parts: Vec<&str> = head.split('/').collect();
    (0..parts.len()).any(|k| {
        parts[..k].iter().all(|x| matches!(*x, "" | "." | ".."))
            && first(parts[k])
            && parts[k + 1..].iter().all(|x| later(x))
    })
}

/// origins.py `LAZY.match(line)`: a rolldown lazy module body,
/// `\s*(?:(?:var|let|const) [\w$]+ = )?__esm(?:Min)?\(`.
pub fn lazy_body(line: &str) -> bool {
    let rest = line.trim_start_matches(is_space);
    let esm = |s: &str| s.starts_with("__esm(") || s.starts_with("__esmMin(");
    for kw in ["var ", "let ", "const "] {
        if let Some(r) = rest.strip_prefix(kw) {
            let n = r
                .find(|c: char| !(is_word(c) || c == '$'))
                .unwrap_or(r.len());
            if n > 0
                && let Some(after) = r[n..].strip_prefix(" = ")
                && esm(after)
            {
                return true;
            }
        }
    }
    esm(rest)
}

/// origins.py `EXPORT.match(line)`: `^export\b`.
pub fn export_line(line: &str) -> bool {
    line.strip_prefix("export")
        .is_some_and(|r| r.chars().next().is_none_or(|c| !is_word(c)))
}

/// origins.py `SRC_EXT.sub("", base)`: drop a JS/TS source extension.
pub fn strip_src_ext(base: &str) -> &str {
    match base.rsplit_once('.') {
        Some((head, ext)) if EXTS.contains(&ext) => head,
        _ => base,
    }
}

/// origins.py `PNPM_PKG.findall(path)`: every `.pnpm/<name>@<version>`
/// (a scope spelled `@scope+name`), as (name, version).
pub fn pnpm_pkgs(path: &str) -> Vec<(&str, &str)> {
    const P: &str = ".pnpm/";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = path[from..].find(P) {
        let start = from + at + P.len();
        let s = &path[start..];
        let mut name_start = 0;
        let mut ok = true;
        if let Some(rest) = s.strip_prefix('@') {
            match rest.find(['/', '@', '+']) {
                Some(n) if n > 0 && rest[n..].starts_with('+') => name_start = 1 + n + 1,
                _ => ok = false,
            }
        }
        let mut matched = None;
        if ok {
            let t = &s[name_start..];
            let n = t.find(['/', '@']).unwrap_or(t.len());
            if n > 0 && t[n..].starts_with('@') {
                let v = &t[n + 1..];
                let m = v.find(['/', '_', '(']).unwrap_or(v.len());
                if m > 0 {
                    matched = Some((name_start + n, name_start + n + 1 + m));
                }
            }
        }
        match matched {
            Some((name_end, end)) => {
                out.push((&s[..name_end], &s[name_end + 1..end]));
                from = start + end;
            }
            None => from = from + at + 1,
        }
    }
    out
}

#[cfg(test)]
#[path = "pyre_test.rs"]
mod pyre_test;
