//! File/folder NAME shaping shared by every grouping strategy — TS
//! `src/split/stable-split.ts` (`BAD_STEM`, `hasMintedNumber`,
//! `LEADING_STOPWORD`, `toKebabCase`, `GENERIC_NAMES`, `toCamelCase`,
//! `acceptProposedName`, `isRejectedStem`) and `fossil-assign.ts`'s
//! `stemOf`. One owner of "is this a legal/meaningful file stem"
//! (docs/responsibility.md): the mechanical stem pickers and the LLM
//! proposal validator both route through [`is_rejected_stem`], so a bad
//! name is blocked whichever produced it.
//!
//! The TS regexes are non-unicode JS `RegExp`s: `\w`/`\d` are ASCII, and
//! the `/i` flag folds ASCII only (a non-ASCII char never canonicalizes to
//! ASCII — ES `Canonicalize`). They are ported as explicit primitives with
//! those semantics (lesson 15), never as Rust `char` classes.

/// ASCII `\w` — `[A-Za-z0-9_]`.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `s` consists of ASCII `\w` only (the regex tail `\w*`).
fn all_word(s: &str) -> bool {
    s.chars().all(is_word)
}

/// `s` consists of ASCII digits only (the regex tail `\d*`).
fn all_digits(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit())
}

/// Strip one optional `[-_]`.
fn strip_sep(s: &str) -> &str {
    s.strip_prefix(['-', '_']).unwrap_or(s)
}

/// `no[-_]?ops?\w*`
fn is_noop_stem(s: &str) -> bool {
    s.strip_prefix("no")
        .map(strip_sep)
        .and_then(|r| r.strip_prefix("op"))
        .is_some_and(all_word)
}

/// `silent[-_]?noops?\w*`
fn is_silent_noop(s: &str) -> bool {
    s.strip_prefix("silent")
        .map(strip_sep)
        .and_then(|r| r.strip_prefix("noop"))
        .is_some_and(all_word)
}

/// `empty(function|callback|operation|handler)s?\d*`
fn is_empty_stub(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("empty") else {
        return false;
    };
    ["function", "callback", "operation", "handler"]
        .iter()
        .filter_map(|w| rest.strip_prefix(w))
        .any(|tail| all_digits(tail.strip_prefix('s').unwrap_or(tail)) || all_digits(tail))
}

/// `idle[-_]?operation\d*`
fn is_idle_operation(s: &str) -> bool {
    s.strip_prefix("idle")
        .map(strip_sep)
        .and_then(|r| r.strip_prefix("operation"))
        .is_some_and(all_digits)
}

/// `<prefix>\d+`
fn prefix_then_digits(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix)
        .is_some_and(|d| !d.is_empty() && all_digits(d))
}

/// `_+\d*`
fn is_underscore_run(s: &str) -> bool {
    let tail = s.trim_start_matches('_');
    tail.len() < s.len() && all_digits(tail)
}

/// `\w+Val\d*` — the whole string is `\w`, its trailing digit run is
/// preceded by `val`, and at least one `\w` precedes that.
fn is_val_decorated(s: &str) -> bool {
    if !all_word(s) {
        return false;
    }
    let head = s.trim_end_matches(|c: char| c.is_ascii_digit());
    head.len() >= 4 && head.ends_with("val")
}

/// `BAD_STEM` (anchored, `/i`): minted/placeholder/decorated names that make
/// bad file names — the noop/doNothing/empty-stub families the LLM gives
/// tree-shaken stub modules.
pub fn is_bad_stem(name: &str) -> bool {
    // `/i` without `u`: ASCII case folding only.
    let s = name.to_ascii_lowercase();
    is_noop_stem(&s)
        || s.strip_prefix("donothing").is_some_and(all_word)
        || is_silent_noop(&s)
        || is_empty_stub(&s)
        || is_idle_operation(&s)
        || prefix_then_digits(&s, "initializemodule")
        || s.strip_prefix("placeholder").is_some_and(all_word)
        || is_underscore_run(&s)
        || prefix_then_digits(&s, "reactlib")
        || is_val_decorated(&s)
}

/// Digit runs that are a real part of a technical name, not a minted
/// disambiguator: bit widths, hash sizes, versions.
const KNOWN_NUMBER_TOKENS: [&str; 8] = ["8", "16", "32", "64", "128", "256", "512", "1024"];

/// `hasMintedNumber`: a run of 2+ ASCII digits that is NOT a known unit
/// token (appInitializer17 is minted; float64Error, sha256Hasher are real).
pub fn has_minted_number(name: &str) -> bool {
    name.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .any(|run| run.len() >= 2 && !KNOWN_NUMBER_TOKENS.contains(&run))
}

/// `LEADING_STOPWORD = /^(and|or|but|nor|the|an|a)(?=[A-Z0-9]|$)/` —
/// case-SENSITIVE; the first alternative that satisfies the lookahead wins,
/// and the lookahead only reads the next char, so the alternatives' order
/// is immaterial to the verdict.
pub fn has_leading_stopword(name: &str) -> bool {
    ["and", "or", "but", "nor", "the", "an", "a"]
        .iter()
        .filter_map(|w| name.strip_prefix(w))
        .any(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
}

/// `isRejectedStem`: a binding that must never become a file/folder stem.
pub fn is_rejected_stem(name: &str) -> bool {
    is_bad_stem(name) || has_minted_number(name) || has_leading_stopword(name)
}

/// `.replace(/([a-z0-9])([A-Z])/g, "$1-$2")` — a left-to-right,
/// non-overlapping scan (a matched pair consumes both chars).
pub fn split_lower_upper(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let pair = (c.is_ascii_lowercase() || c.is_ascii_digit())
            && chars.get(i + 1).is_some_and(char::is_ascii_uppercase);
        out.push(c);
        if pair {
            out.push('-');
            out.push(chars[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// `.replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")`: an uppercase run of 2+
/// followed by a lowercase letter splits before its LAST capital
/// (`ABCDef` → `ABC-Def`); a run not followed by lowercase is untouched.
fn split_acronym(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_uppercase() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && chars[j].is_ascii_uppercase() {
            j += 1;
        }
        let lower_next = chars.get(j).is_some_and(char::is_ascii_lowercase);
        if j - i >= 2 && lower_next {
            out.extend(&chars[i..j - 1]);
            out.push('-');
            out.push(chars[j - 1]);
            out.push(chars[j]);
            i = j + 1;
        } else {
            out.extend(&chars[i..j]);
            i = j;
        }
    }
    out
}

/// Replace every maximal run of chars matching `is_run` with `with`.
fn replace_runs(s: &str, is_run: impl Fn(char) -> bool, with: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for c in s.chars() {
        if is_run(c) {
            if !in_run {
                out.push_str(with);
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// `toKebabCase`: camelCase / PascalCase / acronym / mixed → kebab-case,
/// the src/ tree's file+folder convention.
pub fn to_kebab_case(name: &str) -> String {
    let split = split_acronym(&split_lower_upper(name));
    let lower = split.to_lowercase();
    let dashed = replace_runs(
        &lower,
        |c| !(c.is_ascii_lowercase() || c.is_ascii_digit()),
        "-",
    );
    dashed.trim_matches('-').to_string()
}

/// `toCamelCase`: `/[-_]+([A-Za-z0-9])/g` → the char upper-cased. A
/// separator run not followed by an ASCII alphanumeric is kept verbatim.
fn to_camel_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '-' && chars[i] != '_' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && (chars[j] == '-' || chars[j] == '_') {
            j += 1;
        }
        match chars.get(j) {
            Some(c) if c.is_ascii_alphanumeric() => {
                out.push(c.to_ascii_uppercase());
                i = j + 1;
            }
            _ => {
                out.extend(&chars[i..j]);
                i = j;
            }
        }
    }
    out
}

/// Names too generic to be a file/folder name.
const GENERIC_NAMES: [&str; 17] = [
    "utils",
    "util",
    "helpers",
    "helper",
    "misc",
    "core",
    "common",
    "lib",
    "libs",
    "main",
    "index",
    "shared",
    "module",
    "modules",
    "code",
    "src",
    "functions",
];

/// `/^[A-Za-z_$][A-Za-z0-9_$-]{1,39}$/`
fn is_identifier_ish(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest: Vec<char> = chars.collect();
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && (1..=39).contains(&rest.len())
        && rest
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '-'))
}

/// `acceptProposedName`: validate a namer proposal and normalize it to
/// camelCase, or `None` when it is not identifier-ish, is generic, or is
/// minted/placeholder-shaped (checked on the normalized form).
pub fn accept_proposed_name(name: &str) -> Option<String> {
    if !is_identifier_ish(name) {
        return None;
    }
    let camel = to_camel_case(name);
    if GENERIC_NAMES.contains(&camel.to_lowercase().as_str()) || is_rejected_stem(&camel) {
        return None;
    }
    Some(camel)
}

/// `stemOf` (fossil-assign.ts): kebab-case a declared identifier into a
/// file stem — leading `_`/`$` dropped, `[_$]` runs → `-`, dash runs
/// collapsed, ONE leading and ONE trailing dash trimmed; `module` when
/// nothing is left. Unlike [`to_kebab_case`] it keeps non-ASCII letters.
pub fn stem_of(name: &str) -> String {
    let trimmed = name.trim_start_matches(['_', '$']);
    let split = split_lower_upper(trimmed);
    let dashed = replace_runs(&split, |c| c == '_' || c == '$', "-").to_lowercase();
    let collapsed = replace_runs(&dashed, |c| c == '-', "-");
    let head = collapsed.strip_prefix('-').unwrap_or(&collapsed);
    let kebab = head.strip_suffix('-').unwrap_or(head);
    if kebab.is_empty() {
        "module".to_string()
    } else {
        kebab.to_string()
    }
}

#[cfg(test)]
mod stems_test;
