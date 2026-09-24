//! The naming floor's name-shape predicates (the part of
//! `src/rename/minted-census.ts` validated rename reads: the carried-names
//! rule records a binding when an APPLIED name is below the floor). WP4.5
//! owns the rest of minted-census.ts and extends this module.
//!
//! Pinned to the TS by `test/parity/wp31-names.json`. Two JS-isms carried
//! over on purpose: `name.length` counts UTF-16 code units (so `é` is a
//! length-1 name), and `toLowerCase()` is Unicode lowercasing.

/// Short words that are real names, not mints (`SHORT_WORDS`).
const SHORT_WORDS: &[&str] = &[
    "a", "abs", "add", "arg", "b", "cb", "col", "ctx", "cwd", "db", "del", "dir", "e", "end",
    "env", "err", "ext", "fn", "fs", "get", "gid", "go", "has", "i", "id", "idx", "io", "ip", "j",
    "k", "key", "len", "log", "map", "max", "min", "msg", "n", "now", "num", "obj", "ok", "os",
    "out", "pid", "pos", "raw", "req", "res", "row", "run", "sep", "set", "str", "sum", "t", "tag",
    "ui", "uid", "url", "val", "x", "y",
];

/// Domain stems a mint-shaped head is allowed to carry (`DOMAIN_STEMS`).
const DOMAIN_STEMS: &[&str] = &[
    "e164", "ec2", "s3", "sha1", "sha256", "sha512", "md5", "utf8", "utf16", "base64", "http2",
    "oauth2", "i18n", "l10n", "a11y", "es5", "es6", "es2015", "ipv4", "ipv6", "v8", "w3c", "k8s",
    "b64", "u2f", "x509",
];

/// Stems that count only with a word tail (`SUFFIX_REQUIRED_STEMS`).
const SUFFIX_REQUIRED_STEMS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6", "it2", "v1", "x0"];

/// UTF-16 length (JS `name.length`).
fn js_len(name: &str) -> usize {
    name.encode_utf16().count()
}

/// The UTF-16 unit at `index` (JS `name[index]`), when it is ASCII.
fn js_char_at(name: &str, index: usize) -> Option<u16> {
    name.encode_utf16().nth(index)
}

/// `isWordTailBoundary`: `_` or an ASCII capital.
fn is_word_tail_boundary(next: Option<u16>) -> bool {
    next.is_some_and(|c| c == u16::from(b'_') || (u16::from(b'A')..=u16::from(b'Z')).contains(&c))
}

/// `CONSTANT_CASE`: `/^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+$/`.
fn is_constant_case(name: &str) -> bool {
    let parts: Vec<&str> = name.split('_').collect();
    parts.len() >= 2
        && name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
}

/// `hasDomainStemHead`.
fn has_domain_stem_head(name: &str) -> bool {
    let lower = name.to_lowercase();
    let domain = DOMAIN_STEMS.iter().any(|stem| {
        lower.starts_with(stem) && {
            let next = js_char_at(name, stem.len());
            next.is_none() || is_word_tail_boundary(next)
        }
    });
    domain
        || SUFFIX_REQUIRED_STEMS.iter().any(|stem| {
            lower.starts_with(stem) && is_word_tail_boundary(js_char_at(name, stem.len()))
        })
}

/// `/^[A-Za-z]{1,2}[0-9_]/`.
fn has_mint_head(name: &str) -> bool {
    let b = name.as_bytes();
    let tail = |c: Option<&u8>| c.is_some_and(|c| c.is_ascii_digit() || *c == b'_');
    match b.first() {
        Some(c) if c.is_ascii_alphabetic() => {
            tail(b.get(1)) || (b.get(1).is_some_and(u8::is_ascii_alphabetic) && tail(b.get(2)))
        }
        _ => false,
    }
}

/// `isBunToken`: the shape of a minifier-minted token.
pub fn is_bun_token(name: &str) -> bool {
    if name.contains('$') || name.ends_with('_') {
        return true;
    }
    if is_constant_case(name) || has_domain_stem_head(name) {
        return false;
    }
    if has_mint_head(name) {
        return true;
    }
    js_len(name) <= 2 && !SHORT_WORDS.contains(&name.to_lowercase().as_str())
}

/// `isDecoratedDescriptive`: a descriptive stem wearing the conflict
/// ladder's trailing `_` (`fsPromises_`).
pub fn is_decorated_descriptive(name: &str) -> bool {
    if !name.ends_with('_') {
        return false;
    }
    let stem = name.trim_end_matches('_');
    !stem.is_empty() && !is_bun_token(stem)
}

/// `isBelowFloorName`: minted-shaped and not a decorated descriptive name.
pub fn is_below_floor_name(name: &str) -> bool {
    is_bun_token(name) && !is_decorated_descriptive(name)
}
