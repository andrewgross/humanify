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

use humanify_model::detection::{BundlerType, MinifierType};

/// Universal — what the CommonJS module system hands every module (the
/// toolchain's one list, `crate::toolchain::COMMONJS_CONTEXT`).
const UNIVERSAL: &[&str] = &crate::toolchain::COMMONJS_CONTEXT_NAMES;

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

/// One swc runtime helper name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwcHelper {
    pub name: &'static str,
    /// Does seeing this name say the input was compiled by swc? False for
    /// the names Babel's helpers share (`_inherits`, `_extends`): Babel
    /// output carries them too, so minifier detection must not read them.
    pub detection_marker: bool,
}

const fn swc(name: &'static str) -> SwcHelper {
    SwcHelper {
        name,
        detection_marker: true,
    }
}

const fn shared_with_babel(name: &'static str) -> SwcHelper {
    SwcHelper {
        name,
        detection_marker: false,
    }
}

/// swc's runtime helper names — THE list (toolchain review R20; the
/// minifier detector held a second, 15-name copy with nothing saying why
/// it left two out). Every one is never renamed under a swc verdict; the
/// swc minifier signal (`detect::signals::detect_swc_minifier`) reads only
/// the [`SwcHelper::detection_marker`] ones.
pub const SWC_HELPERS: &[SwcHelper] = &[
    swc("_interop_require_default"),
    swc("_interop_require_wildcard"),
    swc("_class_call_check"),
    swc("_create_class"),
    shared_with_babel("_inherits"),
    swc("_create_super"),
    swc("_sliced_to_array"),
    swc("_to_consumable_array"),
    swc("_object_spread"),
    swc("_object_spread_props"),
    swc("_async_to_generator"),
    swc("_ts_generator"),
    swc("_define_property"),
    swc("_object_destructuring_empty"),
    shared_with_babel("_extends"),
    swc("_object_without_properties"),
    swc("_tagged_template_literal"),
];

/// The never-rename helper lists one run uses (docs/plugin-spec.md P7): at
/// most one bundler runtime list and one minifier helper list on top of
/// [`UNIVERSAL`]. Chosen once per run by the toolchain
/// (`crate::toolchain::resolve_toolchain`) and carried as a value to every
/// eligibility question — the naming stage, the match stage's graphs and
/// the post-split reconcile alike, so they cannot answer differently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NeverRename {
    bundler: Option<BundlerHelpers>,
    minifier: Option<MinifierHelpers>,
}

/// The bundlers whose runtime helper names have a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BundlerHelpers {
    Webpack,
    Esbuild,
}

/// The minifiers whose helper names have a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MinifierHelpers {
    Swc,
}

impl NeverRename {
    /// Only the universal list (and the two always-on shape rules).
    pub const UNIVERSAL: NeverRename = NeverRename {
        bundler: None,
        minifier: None,
    };

    /// The lists for a bundler + minifier verdict (`createSkipSet`'s
    /// selection): webpack's and esbuild's runtime names, swc's helpers.
    pub fn for_verdicts(bundler: BundlerType, minifier: MinifierType) -> NeverRename {
        NeverRename {
            bundler: match bundler {
                BundlerType::Webpack => Some(BundlerHelpers::Webpack),
                BundlerType::Esbuild => Some(BundlerHelpers::Esbuild),
                _ => None,
            },
            minifier: match minifier {
                MinifierType::Swc => Some(MinifierHelpers::Swc),
                _ => None,
            },
        }
    }

    /// The recorded name: the lists in use, `+`-joined, or `universal`.
    pub fn name(self) -> String {
        let mut parts = Vec::new();
        match self.bundler {
            Some(BundlerHelpers::Webpack) => parts.push("webpack"),
            Some(BundlerHelpers::Esbuild) => parts.push("esbuild"),
            None => {}
        }
        if self.minifier == Some(MinifierHelpers::Swc) {
            parts.push("swc");
        }
        if parts.is_empty() {
            "universal".to_string()
        } else {
            parts.join("+")
        }
    }
}

/// The skip-set for one run's lists (`createSkipSet`). The TS caches per
/// combination; the Rust builds are cheap enough to build per call — the
/// sets are tiny.
pub fn create_skip_set(lists: NeverRename) -> HashSet<&'static str> {
    let mut set: HashSet<&'static str> = UNIVERSAL.iter().copied().collect();
    match lists.bundler {
        Some(BundlerHelpers::Webpack) => set.extend(WEBPACK.iter().copied()),
        Some(BundlerHelpers::Esbuild) => set.extend(ESBUILD.iter().copied()),
        None => {}
    }
    if lists.minifier == Some(MinifierHelpers::Swc) {
        set.extend(SWC_HELPERS.iter().map(|h| h.name));
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
pub fn is_eligible(name: &str, lists: NeverRename) -> bool {
    Eligibility::new(lists).is_eligible(name)
}

/// `createIsEligible(bundler, minifier)` as a value: the skip set built
/// ONCE (the naming waves ask per used name, ~25k names per request).
#[derive(Clone, Debug)]
pub struct Eligibility {
    skip: HashSet<&'static str>,
}

impl Eligibility {
    pub fn new(lists: NeverRename) -> Eligibility {
        Eligibility {
            skip: create_skip_set(lists),
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
