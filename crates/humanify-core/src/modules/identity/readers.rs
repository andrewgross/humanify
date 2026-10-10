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
//! module are hints ([`HINT_READERS`]): shown to the model, never a name —
//! a repository URL names the repository, not the package (finding #95:
//! the old URL rule named 27 modules in 2.1.216, ~20 of them wrongly:
//! `json-schema` from json-schema.org, `w3`, `canny`, `DepType` from a
//! property chain, `golang`, `grpc-node` for `@grpc/grpc-js`).
//!
//! Strings are read from the TOKEN stream (`vendor_pairing::lex_raw`), so
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
}

/// Every identity reader, in trust order (the first claim wins).
pub const IDENTITY_READERS: &[IdentityReader] = &[
    IdentityReader {
        name: "source-path",
        read: read_source_path,
    },
    IdentityReader {
        name: "package-json",
        read: read_package_json,
    },
    IdentityReader {
        name: "banner",
        read: read_banner,
    },
    IdentityReader {
        name: "highlight-js-grammar",
        read: read_highlight_js_grammar,
    },
    IdentityReader {
        name: "scoped-error-prefix",
        read: read_scoped_error_prefix,
    },
];

/// The hint readers: evidence for the model, never a name.
pub const HINT_READERS: &[fn(&ModuleView<'_>) -> Option<String>] = &[hint_version, hint_repository];

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

/// An inlined `package.json` (a bundler inlines `require("./package.json")`
/// whole): `{name:"<npm name>",version:"<semver>", …}` with at least one
/// more key only a package.json has. When the object IS the module's export
/// the module is that package's `package.json` (the member).
pub fn read_package_json(view: &ModuleView<'_>) -> Option<Claim> {
    let t = view.tokens;
    let mut found: Vec<(String, String, usize, usize)> = Vec::new();
    for i in 0..t.len() {
        if t[i] != "{" || !is_key(at(t, i + 1), "name") || at(t, i + 2) != ":" {
            continue;
        }
        let (Some(name), Some(version)) = (quoted(at(t, i + 3)), quoted(at(t, i + 7))) else {
            continue;
        };
        let shaped = at(t, i + 4) == ","
            && is_key(at(t, i + 5), "version")
            && at(t, i + 6) == ":"
            && is_npm_name(name)
            && is_semver(version);
        if !shaped {
            continue;
        }
        let Some(close) = matching_close(t, i) else {
            continue;
        };
        let more = top_level_keys(t, i, close)
            .iter()
            .any(|&k| PACKAGE_JSON_KEYS.contains(&key_text(&t[k])));
        if more && found.iter().all(|(n, ..)| n != name) {
            found.push((name.to_string(), version.to_string(), i, close));
        }
    }
    let [(name, version, open, close)] = found.as_slice() else {
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
        evidence: clip(&t[*open..(*open + 8).min(t.len())].concat(), EVIDENCE_CHARS),
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
    /// `x.name = "…"` (a language extending another, e.g. Arduino).
    NameAssigned(String),
    /// `x.aliases = […]`.
    AliasesAssigned(Vec<String>),
}

/// The aliases of the definition object `{` at `open`.
fn definition_aliases(t: &[String], open: usize) -> Vec<String> {
    let Some(close) = matching_close(t, open) else {
        return Vec::new();
    };
    top_level_keys(t, open, close)
        .into_iter()
        .find(|&k| key_text(&t[k]) == "aliases")
        .map(|k| string_array(t, k + 2))
        .unwrap_or_default()
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
        {
            events.push(GrammarEvent::Definition(
                unescape(name),
                definition_aliases(t, i),
            ));
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
/// keywords:…}`): the package's own description of the module. The LAST
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

/// A version constant (`VERSION = "1.9.0"`, `version: "1.9.0"`) — one
/// value only.
pub fn hint_version(view: &ModuleView<'_>) -> Option<String> {
    let t = view.tokens;
    let mut found: Vec<&str> = Vec::new();
    for i in 0..t.len() {
        if matches!(key_text(&t[i]), "VERSION" | "version")
            && matches!(at(t, i + 1), "=" | ":")
            && let Some(v) = quoted(at(t, i + 2))
            && is_semver(v.strip_prefix('v').unwrap_or(v))
            && !found.contains(&v)
        {
            found.push(v);
        }
    }
    let [v] = found.as_slice() else {
        return None;
    };
    Some(format!("version {v}"))
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
