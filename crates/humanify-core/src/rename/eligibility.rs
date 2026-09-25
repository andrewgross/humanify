//! Rename eligibility + the never-rename skip-set (ported EARLY from the
//! ledger's WP3.1/WP3.2 rows: the WP1.4 functions gate's module-binding
//! rows are filtered by it, so the graph cannot be gated without it).
//! TS originals: `src/rename/skip-list.ts` + `src/rename/rename-eligibility.ts`.
//!
//! Opt-out skip-list replacing the old looksMinified heuristic: everything
//! in scope.bindings is eligible UNLESS it matches a skip-set entry or a
//! pattern-based skip rule — so short names like `get`/`set`/`map` are
//! renamed. Drives REPORTING and the graph's member set; a rename that
//! changes a free reference is a different (stronger) question.

use std::collections::HashSet;

/// Universal — the Node.js module system.
const UNIVERSAL: &[&str] = &["exports", "require", "module", "__filename", "__dirname"];

/// Webpack runtime.
const WEBPACK: &[&str] = &[
    "__webpack_require__",
    "__webpack_modules__",
    "__webpack_exports__",
    "__webpack_module_cache__",
];

/// esbuild runtime.
const ESBUILD: &[&str] = &[
    "__commonJS",
    "__toESM",
    "__toCommonJS",
    "__export",
    "__require",
    "__name",
    "__publicField",
];

/// SWC helpers.
const SWC: &[&str] = &[
    "_interop_require_default",
    "_interop_require_wildcard",
    "_class_call_check",
    "_create_class",
    "_inherits",
    "_create_super",
    "_sliced_to_array",
    "_to_consumable_array",
    "_object_spread",
    "_object_spread_props",
    "_async_to_generator",
    "_ts_generator",
    "_define_property",
    "_object_destructuring_empty",
    "_extends",
    "_object_without_properties",
    "_tagged_template_literal",
];

/// The skip-set for a bundler+minifier combination (`createSkipSet`). The
/// TS caches per combination; the Rust builds are cheap enough to build
/// per call — the sets are tiny.
pub fn create_skip_set(bundler: Option<&str>, minifier: Option<&str>) -> HashSet<&'static str> {
    let mut set: HashSet<&'static str> = UNIVERSAL.iter().copied().collect();
    if bundler == Some("webpack") {
        set.extend(WEBPACK.iter().copied());
    }
    if bundler == Some("esbuild") {
        set.extend(ESBUILD.iter().copied());
    }
    if minifier == Some("swc") {
        set.extend(SWC.iter().copied());
    }
    set
}

/// WORD-LIKE double-underscore prefix → bundler/runtime helper or library
/// API (`__esm`, `__commonJS`, `__createBinding`, `__exportStar`). SHORT
/// dunder bindings (`__c`, `__t`, `__ab`) are NOT reserved: those are
/// minifier-minted app bindings (measured on 216: 22 such bindings, 0 real
/// helpers of this shape — Bun minifies its own helpers to single letters
/// like `Q`/`b`, never `__`-prefixed). Provenance, not shape.
/// TS: /^__[a-z][A-Za-z0-9$]{2,}/ — UNANCHORED at the end: only the two
/// characters after the lowercase letter are constrained, so `__abc_d` is
/// reserved (WP3.1 probe, test/parity/wp31-names.json; the WP1.4 port
/// required ALL the rest to be alphanumeric and called `__abc_d` eligible).
fn is_word_like_dunder(name: &str) -> bool {
    let b = name.as_bytes();
    let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'$';
    b.len() >= 5
        && b[0] == b'_'
        && b[1] == b'_'
        && b[2].is_ascii_lowercase()
        && word(&b[3])
        && word(&b[4])
}

/// SWC helper pattern: _word_word (at least two underscore-separated
/// segments). TS: /^_[a-z]+(_[a-z]+)+$/
fn is_swc_helper_shape(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('_') else {
        return false;
    };
    let mut segments = rest.split('_').peekable();
    let Some(first) = segments.next() else {
        return false;
    };
    if first.is_empty() || !first.bytes().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    let mut count = 1;
    for seg in segments {
        if seg.is_empty() || !seg.bytes().all(|c| c.is_ascii_lowercase()) {
            return false;
        }
        count += 1;
    }
    // `(_[a-z]+)+` — at least one additional segment.
    count >= 2
}

/// True when the identifier is eligible for renaming (`createIsEligible`):
/// false for the skip-set entries and the two pattern rules, true otherwise.
pub fn is_eligible(name: &str, bundler: Option<&str>, minifier: Option<&str>) -> bool {
    Eligibility::new(bundler, minifier).is_eligible(name)
}

/// `createIsEligible(bundler, minifier)` as a value: the skip set built
/// ONCE (the naming waves ask per used name, ~25k names per request).
#[derive(Clone, Debug)]
pub struct Eligibility {
    skip: HashSet<&'static str>,
}

impl Eligibility {
    pub fn new(bundler: Option<&str>, minifier: Option<&str>) -> Eligibility {
        Eligibility {
            skip: create_skip_set(bundler, minifier),
        }
    }

    /// The predicate [`is_eligible`] answers.
    pub fn is_eligible(&self, name: &str) -> bool {
        eligible_with(name, &self.skip)
    }
}

fn eligible_with(name: &str, skip: &HashSet<&'static str>) -> bool {
    if name.is_empty() {
        return false;
    }
    if skip.contains(name) {
        return false;
    }
    if is_word_like_dunder(name) {
        return false;
    }
    if is_swc_helper_shape(name) {
        return false;
    }
    true
}
