//! The declared-identity READERS: what a package says about itself, read
//! from one module's text and from what its bundler kept
//! ([`super::ModuleProvenance`]). Bundler-independent — no reader knows
//! which bundler wrote the module.
//!
//! Each reader is small, pure and shape-checked: it returns a claim only
//! when the shape it knows is there, and abstains when the module holds
//! two different answers. The list order is the trust order
//! ([`IDENTITY_READERS`]): a path the bundler wrote, an inlined
//! `package.json`, a `pkg vX` banner, a highlight.js language definition,
//! a scoped package quoting itself in a message. Facts too weak to NAME a
//! module are hints ([`HINT_READERS`]: a version constant, a repository
//! URL, an AWS SDK client's commands): shown to the model, never a name —
//! a repository URL names the repository, not the package (finding #95:
//! the old URL rule named 27 modules in 2.1.216, ~20 of them wrongly:
//! `json-schema` from json-schema.org, `w3`, `canny`, `DepType` from a
//! property chain, `golang`, `grpc-node` for `@grpc/grpc-js`).
//!
//! Strings are read from the TOKEN stream (`modules::lex::lex_raw`), so
//! a reader never pairs a closing quote with the next opening one.

use super::{ModuleProvenance, PathKind};

/// One module, as the readers see it.
pub struct ModuleView<'a> {
    /// The module's factory function text.
    pub body: &'a str,
    /// `body`'s raw token stream (strings keep their quotes).
    pub tokens: &'a [String],
    pub provenance: &'a ModuleProvenance,
}

/// What one reader declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Claim {
    pub package: Option<String>,
    pub version: Option<String>,
    pub member: Option<String>,
    pub aliases: Vec<String>,
    pub evidence: String,
}

/// A registered identity reader.
pub struct IdentityReader {
    /// The record's `identity.source`.
    pub name: &'static str,
    pub read: fn(&ModuleView<'_>) -> Option<Claim>,
    /// Also run on APP code, report-only ([`super::scan_app_declarations`]).
    /// Off for a reader that needs the bundler's per-module facts (a path,
    /// a kept banner) and for highlight.js — Claude Code defines its OWN
    /// grammar (Cedar) in app code, in the package's shape.
    pub app_report: bool,
}

/// Every identity reader, in trust order (the first claim wins).
pub const IDENTITY_READERS: &[IdentityReader] = &[
    IdentityReader {
        name: "source-path",
        read: read_source_path,
        app_report: false,
    },
    IdentityReader {
        name: "package-json",
        read: read_package_json,
        app_report: true,
    },
    IdentityReader {
        name: "banner",
        read: read_banner,
        app_report: false,
    },
    IdentityReader {
        name: "highlight-js-grammar",
        read: read_highlight_js_grammar,
        app_report: false,
    },
    IdentityReader {
        name: "scoped-error-prefix",
        read: read_scoped_error_prefix,
        app_report: true,
    },
];

/// The hint readers: evidence for the model, never a name.
pub const HINT_READERS: &[fn(&ModuleView<'_>) -> Option<String>] =
    &[hint_version, hint_repository, hint_aws_sdk_client];

const EVIDENCE_CHARS: usize = 80;

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ---------------------------------------------------------------------------
// Token helpers
// ---------------------------------------------------------------------------

/// A quoted string token's inside (not a template).
fn quoted(t: &str) -> Option<&str> {
    let q = t.chars().next()?;
    if !matches!(q, '"' | '\'') || t.len() < 2 || !t.ends_with(q) {
        return None;
    }
    Some(&t[1..t.len() - 1])
}

/// A string literal's VALUE: its escapes decoded (`–`, `\u{1F600}`,
/// `\xE9`, `\n`, `\"` …) — minified code writes non-ASCII as escapes, and
/// a declared name is the value, not its spelling.
pub fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(e) = chars.next() else {
            break;
        };
        let hex = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>, n: usize| {
            let digits: String = (0..n).filter_map(|_| chars.next()).collect();
            u32::from_str_radix(&digits, 16)
                .ok()
                .and_then(char::from_u32)
        };
        let decoded = match e {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'v' => Some('\u{b}'),
            '0' => Some('\0'),
            'x' => hex(&mut chars, 2),
            'u' if chars.peek() == Some(&'{') => {
                chars.next();
                let digits: String = chars.by_ref().take_while(|&d| d != '}').collect();
                u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            'u' => hex(&mut chars, 4),
            '\n' => continue,
            other => Some(other),
        };
        out.extend(decoded);
    }
    out
}

/// A string or template token's inside.
fn text_value(t: &str) -> Option<&str> {
    if let Some(v) = quoted(t) {
        return Some(v);
    }
    let inner = t.strip_prefix('`')?;
    Some(inner.strip_suffix('`').unwrap_or(inner))
}

/// `key` as an object key: bare or quoted.
fn is_key(t: &str, key: &str) -> bool {
    t == key || quoted(t) == Some(key)
}

fn key_text(t: &str) -> &str {
    quoted(t).unwrap_or(t)
}

fn at(tokens: &[String], i: usize) -> &str {
    tokens.get(i).map_or("", String::as_str)
}

/// The index of the token closing the bracket opened at `open`.
fn matching_close(tokens: &[String], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, t) in tokens.iter().enumerate().skip(open) {
        match t.as_str() {
            "{" | "[" | "(" => depth += 1,
            "}" | "]" | ")" => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// The indices of the keys of the object literal `{` at `open` … `close`,
/// its own level only.
fn top_level_keys(tokens: &[String], open: usize, close: usize) -> Vec<usize> {
    let mut keys = Vec::new();
    let mut depth = 0usize;
    for j in open + 1..close {
        match tokens[j].as_str() {
            "{" | "[" | "(" => depth += 1,
            "}" | "]" | ")" => depth = depth.saturating_sub(1),
            _ if depth == 0
                && at(tokens, j + 1) == ":"
                && matches!(at(tokens, j - 1), "{" | ",") =>
            {
                keys.push(j);
            }
            _ => {}
        }
    }
    keys
}

/// `[ "a" , "b" ]` starting at `open`: the strings, or empty when it is not
/// an array of string literals.
fn string_array(tokens: &[String], open: usize) -> Vec<String> {
    if at(tokens, open) != "[" {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut j = open + 1;
    loop {
        let Some(v) = quoted(at(tokens, j)) else {
            return Vec::new();
        };
        out.push(unescape(v));
        match at(tokens, j + 1) {
            "," => j += 2,
            "]" => return out,
            _ => return Vec::new(),
        }
    }
}

/// An npm package name: `[@scope/]name`, lowercase, `[a-z0-9._-~]`.
pub fn is_npm_name(s: &str) -> bool {
    let seg = |p: &str| {
        p.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
            && p.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'.' | b'_' | b'-' | b'~')
            })
    };
    if s.len() > 214 {
        return false;
    }
    match s.strip_prefix('@') {
        Some(rest) => rest
            .split_once('/')
            .is_some_and(|(scope, name)| seg(scope) && seg(name)),
        None => seg(s),
    }
}

/// `x.y.z` with an optional pre-release/build tail.
fn is_semver(s: &str) -> bool {
    let core_end = s.find(['-', '+']).unwrap_or(s.len());
    let core: Vec<&str> = s[..core_end].split('.').collect();
    core.len() == 3
        && core
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The scoped package a message starts with (`@scope/name` followed by
/// `:` or ` - `), if any.
fn leading_scoped_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('@')?;
    let (scope, after) = rest.split_once('/')?;
    let name_len = after
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        .count();
    let name = after[..name_len].trim_end_matches(['.', '-', '_']);
    let tail = &after[name.len()..];
    let full = &text[..1 + scope.len() + 1 + name.len()];
    (is_npm_name(full) && (tail.starts_with(':') || tail.starts_with(" - "))).then_some(full)
}

// ---------------------------------------------------------------------------
// Identity readers
// ---------------------------------------------------------------------------

/// The package (and the file within it) a `node_modules` path names: the
/// innermost `node_modules/<pkg>`.
fn package_of_path(path: &str) -> Option<(String, String)> {
    const NM: &str = "node_modules/";
    let at = path.rfind(NM)? + NM.len();
    let after = &path[at..];
    let mut parts = after.splitn(if after.starts_with('@') { 3 } else { 2 }, '/');
    let package = if after.starts_with('@') {
        format!("{}/{}", parts.next()?, parts.next()?)
    } else {
        parts.next()?.to_string()
    };
    let rest = parts.next().unwrap_or("").to_string();
    is_npm_name(&package.to_lowercase()).then_some((package, rest))
}

/// A path the BUNDLER wrote (its own wrapper key, or an absolute path the
/// module asked for): the package, and for the module's own file, the
/// file. Two packages: abstain.
pub fn read_source_path(view: &ModuleView<'_>) -> Option<Claim> {
    let mut packages: Vec<String> = Vec::new();
    let mut members: Vec<String> = Vec::new();
    let mut evidence = None;
    for p in &view.provenance.paths {
        let Some((package, rest)) = package_of_path(&p.path) else {
            continue;
        };
        evidence.get_or_insert_with(|| p.path.clone());
        if !packages.contains(&package) {
            packages.push(package);
        }
        if p.kind == PathKind::ModuleFile && !rest.is_empty() && !members.contains(&rest) {
            members.push(rest);
        }
    }
    let [package] = packages.as_slice() else {
        return None;
    };
    Some(Claim {
        package: Some(package.clone()),
        member: (members.len() == 1).then(|| members[0].clone()),
        evidence: clip(&evidence?, EVIDENCE_CHARS * 2),
        ..Claim::default()
    })
}

/// Keys that only a `package.json` has beside `name` and `version`.
const PACKAGE_JSON_KEYS: [&str; 22] = [
    "description",
    "main",
    "module",
    "types",
    "typings",
    "license",
    "repository",
    "author",
    "homepage",
    "keywords",
    "scripts",
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "engines",
    "files",
    "exports",
    "bin",
    "bugs",
    "publishConfig",
    "sideEffects",
    "contributors",
];

/// The value token of the top-level key `key` of the object `open` …
/// `close`, when it is a string literal standing alone (`key:"…"` followed
/// by `,` or the closing brace).
fn string_key_value(t: &[String], keys: &[usize], close: usize, key: &str) -> Option<usize> {
    let k = *keys.iter().find(|&&k| key_text(&t[k]) == key)?;
    (quoted(at(t, k + 2)).is_some() && (k + 3 == close || at(t, k + 3) == ",")).then_some(k + 2)
}

/// The object literal enclosing the key at `key`: its `{`.
fn enclosing_open(t: &[String], key: usize) -> Option<usize> {
    let mut depth = 0usize;
    for j in (0..key).rev() {
        match t[j].as_str() {
            "}" | "]" | ")" => depth += 1,
            "{" | "[" | "(" => {
                if depth == 0 {
                    return (t[j] == "{").then_some(j);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// An inlined `package.json` (a bundler inlines `require("./package.json")`
/// whole): an object whose own `name` is an npm name and own `version` a
/// semver, in ANY key order (sharp writes `description` between them), with
/// at least one more key only a package.json has. When the object IS the
/// module's export the module is that package's `package.json` (the member).
pub fn read_package_json(view: &ModuleView<'_>) -> Option<Claim> {
    let t = view.tokens;
    let mut found: Vec<(String, String, usize, usize, String)> = Vec::new();
    for i in 1..t.len() {
        let is_name_key = is_key(&t[i], "name")
            && at(t, i + 1) == ":"
            && matches!(t[i - 1].as_str(), "{" | ",")
            && quoted(at(t, i + 2)).is_some_and(is_npm_name);
        if !is_name_key {
            continue;
        }
        let Some(open) = enclosing_open(t, i) else {
            continue;
        };
        let Some(close) = matching_close(t, open) else {
            continue;
        };
        let keys = top_level_keys(t, open, close);
        let (Some(name_at), Some(version_at)) = (
            string_key_value(t, &keys, close, "name"),
            string_key_value(t, &keys, close, "version"),
        ) else {
            continue;
        };
        let (Some(name), Some(version)) = (quoted(&t[name_at]), quoted(&t[version_at])) else {
            continue;
        };
        let more = keys
            .iter()
            .any(|&k| PACKAGE_JSON_KEYS.contains(&key_text(&t[k])));
        if !(name_at == i + 2 && is_semver(version) && more) || found.iter().any(|f| f.0 == name) {
            continue;
        }
        let evidence = format!(
            "{{{}:{},{}:{}",
            t[name_at - 2],
            t[name_at],
            t[version_at - 2],
            t[version_at]
        );
        found.push((name.to_string(), version.to_string(), open, close, evidence));
    }
    let [(name, version, open, close, evidence)] = found.as_slice() else {
        return None;
    };
    let whole_export = *open >= 3
        && t[open - 1] == "="
        && t[open - 2] == "exports"
        && t[open - 3] == "."
        && t[close + 1..]
            .iter()
            .all(|x| matches!(x.as_str(), "}" | ";" | ")"));
    Some(Claim {
        package: Some(name.clone()),
        version: Some(version.clone()),
        member: whole_export.then(|| "package.json".to_string()),
        aliases: Vec::new(),
        evidence: clip(evidence, EVIDENCE_CHARS),
    })
}

/// A legal banner the bundler kept (`/*! pkg vX.Y.Z … */`) — the first
/// one naming a package. A copyright banner names an author, not a package
/// ([`parse_banner`]'s rules).
pub fn read_banner(view: &ModuleView<'_>) -> Option<Claim> {
    view.provenance.kept_comments.iter().find_map(|c| {
        let info = parse_banner(c);
        Some(Claim {
            package: Some(info.pkg?),
            version: info.version,
            evidence: clip(&info.text, EVIDENCE_CHARS),
            ..Claim::default()
        })
    })
}

/// A banner's text and the package it names.
#[derive(Clone)]
pub struct BannerInfo {
    pub text: String,
    pub pkg: Option<String>,
    pub version: Option<String>,
}

fn is_js_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// The banner rule (the TS parseBanner, exactly):
///   text = raw.replace(/^!/, '').trim()
///   match /^([@\w][@\w./_-]*)(?:\s+v?(\d[\w.+-]*))?(?:\s|$)/i
///   pkg  = match[1] minus trailing [.,_-]+
///   reject: empty after strip, a license-header false positive, and
///           one-word headers with no version and no shape.
pub fn parse_banner(raw: &str) -> BannerInfo {
    const FALSE_POSITIVES: [&str; 12] = [
        "copyright",
        "license",
        "licence",
        "mit",
        "bsd",
        "isc",
        "apache",
        "the",
        "this",
        "use",
        "see",
        "based",
    ];
    let text = raw.strip_prefix('!').unwrap_or(raw).trim();
    let no_match = BannerInfo {
        text: text.to_string(),
        pkg: None,
        version: None,
    };
    let Some(first) = text.chars().next() else {
        return no_match;
    };
    // `[@\w]` — the pkg's first char.
    if !(first == '@' || first.is_ascii_alphanumeric() || first == '_') {
        return no_match;
    }
    // `[@\w./_-]*` — the maximal run. Greedy with no useful backtrack: every
    // shorter run ends on a class char, which is neither `\s` nor `$`.
    let bytes = text.as_bytes();
    let mut pkg_end = first.len_utf8();
    while pkg_end < bytes.len()
        && (bytes[pkg_end].is_ascii_alphanumeric()
            || matches!(bytes[pkg_end], b'@' | b'.' | b'/' | b'_' | b'-'))
    {
        pkg_end += 1;
    }
    let raw_pkg = &text[..pkg_end];
    // The version attempt: `\s+v?(\d[\w.+-]*)` then `(?:\s|$)`. On ANY
    // failure of the version group the regex falls back to skipping it —
    // the match then requires `(?:\s|$)` directly after the pkg.
    let rest = &text[pkg_end..];
    if !rest.is_empty() && !is_js_ws(rest.as_bytes()[0]) {
        return no_match; // neither `\s` nor `$` — the whole regex fails
    }
    let mut version: Option<String> = None;
    let after_ws = rest.trim_start_matches([' ', '\t', '\n', '\u{0b}', '\u{0c}', '\r']);
    let body = after_ws.strip_prefix('v').unwrap_or(after_ws);
    if body.as_bytes().first().is_some_and(|b| b.is_ascii_digit()) {
        let mut run = 1usize;
        while run < body.len()
            && (body.as_bytes()[run].is_ascii_alphanumeric()
                || matches!(body.as_bytes()[run], b'.' | b'+' | b'-'))
        {
            run += 1;
        }
        let after_run = &body[run..];
        if after_run.is_empty() || is_js_ws(after_run.as_bytes()[0]) {
            // match[2] EXCLUDES the `v` prefix.
            version = Some(body[..run].to_string());
        }
    }
    // pkg = raw_pkg minus trailing [.,_-]+
    let pkg = raw_pkg.trim_end_matches(['.', ',', '_', '-']);
    if pkg.is_empty() {
        return no_match;
    }
    if FALSE_POSITIVES.contains(&pkg.to_ascii_lowercase().as_str()) {
        return no_match;
    }
    // Require a scope/path, a hyphen, an interior dot, or an explicit
    // version — one-word headers like "Sharp" are not package banners.
    let has_shape =
        pkg.contains('/') || pkg.contains('-') || pkg.contains('.') || pkg.starts_with('@');
    if !has_shape && version.is_none() {
        return no_match;
    }
    BannerInfo {
        text: text.to_string(),
        pkg: Some(pkg.to_string()),
        version,
    }
}

/// The keys a highlight.js language definition has right after its
/// `name` (the census's grammar shape: 188 grammars per Claude Code tree,
/// none of the other 43 modules with a `name:` field).
const GRAMMAR_KEYS: [&str; 9] = [
    "aliases",
    "case_insensitive",
    "keywords",
    "contains",
    "disableAutodetect",
    "unicodeRegex",
    "subLanguage",
    "illegal",
    "classNameAliases",
];

enum GrammarEvent {
    /// A language definition object: its name and aliases.
    Definition(String, Vec<String>),
    /// `x.name = "…"` or `Object.assign(x, {name: "…"})` (a language
    /// extending another, e.g. Arduino, TypeScript).
    NameAssigned(String),
    /// `x.aliases = […]` (or the `aliases` of that `Object.assign`).
    AliasesAssigned(Vec<String>),
}

/// The STRICT grammar shape (exp095 Part A: `{name, keywords|contains}`
/// matched nothing but highlight.js in 347 packages; the loose
/// `{name, aliases}` is also Claude Code's slash-command table): the object
/// `{` at `open` has its own `keywords` or `contains` — or EVERY key of it
/// is a language-definition key and one of them is not `aliases`
/// (highlight.js's `plaintext.js`: `{name, aliases, disableAutodetect}`).
/// Its aliases, when it is one.
fn grammar_definition(t: &[String], open: usize) -> Option<Vec<String>> {
    let close = matching_close(t, open)?;
    let keys: Vec<&str> = top_level_keys(t, open, close)
        .into_iter()
        .map(|k| key_text(&t[k]))
        .collect();
    let has_body = keys.iter().any(|k| matches!(*k, "keywords" | "contains"));
    let all_grammar = keys
        .iter()
        .all(|k| *k == "name" || *k == "supersetOf" || GRAMMAR_KEYS.contains(k))
        && keys.iter().any(|k| !matches!(*k, "name" | "aliases"));
    (has_body || all_grammar).then(|| object_aliases(t, open, close))
}

/// The `aliases` string array of the object `open` … `close`.
fn object_aliases(t: &[String], open: usize, close: usize) -> Vec<String> {
    top_level_keys(t, open, close)
        .into_iter()
        .find(|&k| key_text(&t[k]) == "aliases")
        .map(|k| string_array(t, k + 2))
        .unwrap_or_default()
}

/// `Object.assign(<x>, {` — the object at `open` is merged into another.
fn is_object_assign_source(t: &[String], open: usize) -> bool {
    open >= 6
        && t[open - 1] == ","
        && is_identifier(&t[open - 2])
        && t[open - 3] == "("
        && t[open - 4] == "assign"
        && t[open - 5] == "."
        && t[open - 6] == "Object"
}

fn grammar_events(t: &[String]) -> Vec<GrammarEvent> {
    let mut events = Vec::new();
    for i in 0..t.len() {
        if t[i] == "{"
            && is_key(at(t, i + 1), "name")
            && at(t, i + 2) == ":"
            && at(t, i + 4) == ","
            && GRAMMAR_KEYS.contains(&key_text(at(t, i + 5)))
            && at(t, i + 6) == ":"
            && let Some(name) = quoted(at(t, i + 3)).filter(|n| (1..=60).contains(&n.len()))
            && let Some(aliases) = grammar_definition(t, i)
        {
            events.push(GrammarEvent::Definition(unescape(name), aliases));
        } else if t[i] == "{"
            && is_key(at(t, i + 1), "name")
            && at(t, i + 2) == ":"
            && is_object_assign_source(t, i)
            && let Some(name) = quoted(at(t, i + 3)).filter(|n| (1..=60).contains(&n.len()))
            && let Some(close) = matching_close(t, i)
        {
            // TypeScript: `Object.assign(r,{name:"TypeScript",aliases:[…]})`
            // renames the JavaScript grammar it extends.
            events.push(GrammarEvent::NameAssigned(unescape(name)));
            events.push(GrammarEvent::AliasesAssigned(object_aliases(t, i, close)));
        } else if t[i] == "." && at(t, i + 2) == "=" {
            match at(t, i + 1) {
                "name" => {
                    if let Some(name) = quoted(at(t, i + 3)).filter(|n| (1..=60).contains(&n.len()))
                    {
                        events.push(GrammarEvent::NameAssigned(unescape(name)));
                    }
                }
                "aliases" => events.push(GrammarEvent::AliasesAssigned(string_array(t, i + 3))),
                _ => {}
            }
        }
    }
    events
}

/// A highlight.js language definition (`{name:"ActionScript",aliases:[…],
/// keywords:…}`, strict: `keywords` or `contains` present): the package's
/// own description of the module. The LAST
/// definition or `.name=` assignment wins — Arduino inlines the C++
/// grammar and renames it; TypeScript assigns its object over
/// JavaScript's. A `.name=` with no definition object is not a grammar.
pub fn read_highlight_js_grammar(view: &ModuleView<'_>) -> Option<Claim> {
    let events = grammar_events(view.tokens);
    let last_definition = events
        .iter()
        .rposition(|e| matches!(e, GrammarEvent::Definition(..)))?;
    let (name, aliases) = match events
        .iter()
        .rposition(|e| !matches!(e, GrammarEvent::AliasesAssigned(_)))?
    {
        i if i == last_definition => match &events[i] {
            GrammarEvent::Definition(n, a) => (n.clone(), a.clone()),
            _ => unreachable!("a definition"),
        },
        i => {
            let GrammarEvent::NameAssigned(n) = &events[i] else {
                unreachable!("a name assignment")
            };
            let aliases = events[last_definition..]
                .iter()
                .rev()
                .find_map(|e| match e {
                    GrammarEvent::AliasesAssigned(a) => Some(a.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            (n.clone(), aliases)
        }
    };
    Some(Claim {
        package: Some("highlight.js".to_string()),
        member: Some(name.clone()),
        evidence: format!("name:\"{name}\""),
        aliases,
        ..Claim::default()
    })
}

/// A scoped package quoting ITSELF at the start of a message passed to a
/// call (`Error("@smithy/util-base64: …")`, `warn(\`@smithy/util-stream - …\`)`).
/// A name mid-sentence, in a list or in an install hint is not one; two
/// different packages: abstain.
pub fn read_scoped_error_prefix(view: &ModuleView<'_>) -> Option<Claim> {
    let t = view.tokens;
    let mut found: Vec<(String, String)> = Vec::new();
    for i in 1..t.len() {
        if t[i - 1] != "(" {
            continue;
        }
        let Some(text) = text_value(&t[i]) else {
            continue;
        };
        if let Some(name) = leading_scoped_name(text)
            && found.iter().all(|(n, _)| n != name)
        {
            found.push((name.to_string(), clip(text, EVIDENCE_CHARS)));
        }
    }
    let [(package, evidence)] = found.as_slice() else {
        return None;
    };
    Some(Claim {
        package: Some(package.clone()),
        evidence: evidence.clone(),
        ..Claim::default()
    })
}

// ---------------------------------------------------------------------------
// Hints
// ---------------------------------------------------------------------------

/// A version constant, as a hint: shown to the model ([`version_constant`]).
pub fn hint_version(view: &ModuleView<'_>) -> Option<String> {
    let version = version_constant(view)?.version?;
    Some(format!("version {version}"))
}

/// One `github.com/<owner>/<repo>` (a `.git` suffix dropped) — the
/// repository, which is often not the package (`grpc/grpc-node` holds
/// `@grpc/grpc-js`).
pub fn hint_repository(view: &ModuleView<'_>) -> Option<String> {
    const HOST: &str = "github.com/";
    let lower = view.body.to_ascii_lowercase();
    let mut found: Vec<String> = Vec::new();
    let mut from = 0;
    while let Some(hit) = lower[from..].find(HOST) {
        let start = from + hit + HOST.len();
        from = start;
        let run = |s: &str| {
            s.bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                .count()
        };
        let rest = &view.body[start..];
        let owner_len = run(rest);
        if owner_len == 0 || rest.as_bytes().get(owner_len) != Some(&b'/') {
            continue;
        }
        let repo_part = &rest[owner_len + 1..];
        let repo = repo_part[..run(repo_part)].trim_end_matches('.');
        let repo = repo.strip_suffix(".git").unwrap_or(repo);
        if repo.is_empty() {
            continue;
        }
        let pair = format!("{}/{repo}", &rest[..owner_len]);
        if !found.iter().any(|f| f.eq_ignore_ascii_case(&pair)) {
            found.push(pair);
        }
    }
    let [pair] = found.as_slice() else {
        return None;
    };
    Some(format!("repository github.com/{pair}"))
}

// ---------------------------------------------------------------------------
// The version constant: a version, never a name
// ---------------------------------------------------------------------------

/// The names a package gives its version constant (exp095 Part A census:
/// 56 of 56 such assignments in owner-known npm source equal the true
/// version). `version` counts only as a property ASSIGNMENT (`e.version=`):
/// as an object field it is any record's version.
const VERSION_NAMES: [&str; 6] = [
    "VERSION",
    "PACKAGE_VERSION",
    "SDK_VERSION",
    "LIB_VERSION",
    "libVersion",
    "sdkVersion",
];

fn ends_expression(t: &str) -> bool {
    matches!(t, "" | "," | ";" | "}" | ")")
}

/// A semver string literal at `i` standing alone: its value, `v` dropped.
fn semver_literal(t: &[String], i: usize) -> Option<&str> {
    let v = quoted(at(t, i))?;
    let v = v.strip_prefix('v').unwrap_or(v);
    (is_semver(v) && ends_expression(at(t, i + 1))).then_some(v)
}

/// The one semver literal the identifier `name` is assigned in the module
/// (`var Nvt="1.15.2"`), if exactly one.
fn bound_semver<'t>(t: &'t [String], name: &str) -> Option<&'t str> {
    let mut found: Vec<&str> = Vec::new();
    for j in 0..t.len() {
        if t[j] == name
            && (j == 0 || t[j - 1] != ".")
            && at(t, j + 1) == "="
            && let Some(v) = semver_literal(t, j + 2)
            && !found.contains(&v)
        {
            found.push(v);
        }
    }
    let [v] = found.as_slice() else {
        return None;
    };
    Some(v)
}

/// The value at `i` (a literal, or an identifier bound to one) when it
/// is a semver.
fn semver_value(t: &[String], i: usize) -> Option<&str> {
    semver_literal(t, i).or_else(|| {
        let name = at(t, i);
        (is_identifier(name) && ends_expression(at(t, i + 1)))
            .then(|| bound_semver(t, name))
            .flatten()
    })
}

/// A package's version constant: `X.VERSION="1.9.0"`, `var VERSION="…"`,
/// `rB.VERSION=Nvt` with `var Nvt="1.15.2"`, an ES module's export getter
/// `{VERSION:()=>Nvt}`, `e.version=Oo`. A VERSION only — it never names a
/// package; [`super::declare`] adds it to the identity another reader
/// names, and it is a hint otherwise. Two different values: abstain.
pub fn version_constant(view: &ModuleView<'_>) -> Option<Claim> {
    let t = view.tokens;
    let mut found: Vec<(&str, String)> = Vec::new();
    for i in 0..t.len() {
        let name = t[i].as_str();
        let after_dot = i > 0 && t[i - 1] == ".";
        let named = VERSION_NAMES.contains(&name) || (name == "version" && after_dot);
        if !named {
            continue;
        }
        let value = if at(t, i + 1) == "=" {
            semver_value(t, i + 2).map(|v| (v, format!("{name}={}", at(t, i + 2))))
        } else if VERSION_NAMES.contains(&name)
            && !after_dot
            && at(t, i + 1) == ":"
            && matches!(t[i - 1].as_str(), "{" | ",")
            && at(t, i + 2) == "("
            && at(t, i + 3) == ")"
            && at(t, i + 4) == "=>"
        {
            semver_value(t, i + 5).map(|v| (v, format!("{name}:()=>{}", at(t, i + 5))))
        } else {
            None
        };
        if let Some((v, evidence)) = value
            && found.iter().all(|(x, _)| *x != v)
        {
            found.push((v, evidence));
        }
    }
    let [(version, evidence)] = found.as_slice() else {
        return None;
    };
    Some(Claim {
        version: Some((*version).to_string()),
        evidence: clip(evidence, EVIDENCE_CHARS),
        ..Claim::default()
    })
}

// ---------------------------------------------------------------------------
// AWS SDK v3 client/command pairs: the SERVICE, never the package
// ---------------------------------------------------------------------------

/// The AWS SDK v3 client one module's commands belong to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwsSdkClient {
    pub client: String,
    pub commands: Vec<String>,
}

fn is_pascal_with_suffix(s: &str, suffix: &str) -> bool {
    s.len() > suffix.len()
        && s.ends_with(suffix)
        && s.starts_with(|c: char| c.is_ascii_uppercase())
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A smithy command's `.n("BedrockClient","ListTagsForResourceCommand")`:
/// the client it belongs to. The client names the SERVICE, not the
/// package — `STSClient` ships in both `@aws-sdk/client-sts` and
/// `@aws-sdk/nested-clients` — so this is a hint and a report, never an
/// identity. Two clients: abstain.
pub fn aws_sdk_client(view: &ModuleView<'_>) -> Option<AwsSdkClient> {
    let t = view.tokens;
    let mut clients: Vec<AwsSdkClient> = Vec::new();
    for i in 1..t.len() {
        if t[i - 1] != "(" || at(t, i + 1) != "," || at(t, i + 3) != ")" {
            continue;
        }
        let (Some(client), Some(command)) = (quoted(&t[i]), quoted(at(t, i + 2))) else {
            continue;
        };
        if !is_pascal_with_suffix(client, "Client") || !is_pascal_with_suffix(command, "Command") {
            continue;
        }
        let entry = match clients.iter_mut().find(|c| c.client == client) {
            Some(e) => e,
            None => {
                clients.push(AwsSdkClient {
                    client: client.to_string(),
                    commands: Vec::new(),
                });
                clients.last_mut().expect("just pushed")
            }
        };
        if !entry.commands.iter().any(|c| c == command) {
            entry.commands.push(command.to_string());
        }
    }
    let [client] = <[AwsSdkClient; 1]>::try_from(clients).ok()?;
    Some(client)
}

/// [`aws_sdk_client`] as a hint for the model.
pub fn hint_aws_sdk_client(view: &ModuleView<'_>) -> Option<String> {
    let c = aws_sdk_client(view)?;
    let n = c.commands.len();
    let shown = c
        .commands
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let more = if n > 3 { ", …" } else { "" };
    let noun = if n == 1 { "command" } else { "commands" };
    Some(format!(
        "AWS SDK v3 client {} ({n} {noun}: {shown}{more})",
        c.client
    ))
}

// ---------------------------------------------------------------------------
// The export list: a FILE fingerprint, never a package name
// ---------------------------------------------------------------------------

/// The factory's parameter names (`function(e,t)`, `(e,t)=>`, `e=>`, a
/// method's `(exports2, module2)`).
fn factory_params(t: &[String]) -> Vec<&str> {
    let mut i = 0;
    if at(t, 0) == "function" {
        i = 1;
        if at(t, i) != "(" {
            i += 1; // a named function expression
        }
    }
    if at(t, i) != "(" {
        return if at(t, i + 1) == "=>" {
            vec![at(t, i)]
        } else {
            Vec::new()
        };
    }
    let mut params = Vec::new();
    let mut j = i + 1;
    while j < t.len() && t[j] != ")" {
        if t[j] != "," {
            params.push(t[j].as_str());
        }
        j += 1;
    }
    params
}

fn is_identifier(t: &str) -> bool {
    t.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && t.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The module's export names, in first-seen order: assignments to its
/// (renamed) exports parameter (`P.A = P.B = void 0`, `P.x = …`),
/// `M.exports.x = …` on its module parameter, and
/// `Object.defineProperty(P, "x", …)` — TypeScript's compiled export list
/// survives minification this way (38% of Claude Code's factories) while a
/// literal `exports.` does not (2%).
pub fn export_names(t: &[String]) -> Vec<String> {
    let params = factory_params(t);
    let exports = params.first().copied().unwrap_or("exports");
    let module = params.get(1).copied().unwrap_or("module");
    let mut names: Vec<String> = Vec::new();
    let mut push = |n: &str| {
        if n != "__esModule" && is_identifier(n) && !names.iter().any(|x| x == n) {
            names.push(n.to_string());
        }
    };
    for i in 0..t.len() {
        let free = i == 0 || at(t, i - 1) != ".";
        if !free {
            continue;
        }
        if t[i] == exports && at(t, i + 1) == "." && at(t, i + 3) == "=" {
            push(at(t, i + 2));
        } else if t[i] == module
            && at(t, i + 1) == "."
            && at(t, i + 2) == "exports"
            && at(t, i + 3) == "."
            && at(t, i + 5) == "="
        {
            push(at(t, i + 4));
        } else if t[i] == "Object"
            && at(t, i + 1) == "."
            && at(t, i + 2) == "defineProperty"
            && at(t, i + 3) == "("
            && at(t, i + 4) == exports
            && at(t, i + 5) == ","
            && let Some(n) = quoted(at(t, i + 6))
        {
            push(n);
        }
    }
    names
}
