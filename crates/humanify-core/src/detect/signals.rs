//! The per-bundler and per-minifier signal detectors (TS:
//! src/detection/signals/*.ts, WPB.1). Each TS regex is ported as a hand
//! matcher over `js_text`'s primitives; the TS pattern sits in each doc
//! comment so the two can be read side by side. Signal ORDER is part of
//! the output (`detect_bundle` keeps the first definitive bundler and the
//! first highest-tier minifier), so every detector emits in the TS's order.

use humanify_model::detection::{BundlerType, DetectionSignal, DetectionTier, MinifierType};

use super::js_text::{
    contains_word, eat, eat_quote, is_line_terminator, is_word_boundary, js_prefix, skip_js_space,
};

fn bundler_signal(source: &str, pattern: &str, bundler: BundlerType) -> DetectionSignal {
    DetectionSignal {
        source: source.to_string(),
        pattern: pattern.to_string(),
        bundler: Some(bundler),
        minifier: None,
        tier: DetectionTier::Definitive,
    }
}

fn minifier_signal(
    source: &str,
    pattern: &str,
    minifier: MinifierType,
    tier: DetectionTier,
) -> DetectionSignal {
    DetectionSignal {
        source: source.to_string(),
        pattern: pattern.to_string(),
        bundler: None,
        minifier: Some(minifier),
        tier,
    }
}

/// TS `SignalPattern`: a test over the scanned text + the pattern label.
type SignalPattern = (fn(&str) -> bool, &'static str);

/// TS `matchPatterns` (pattern-helper.ts): one definitive signal per
/// matching pattern, in table order.
fn match_patterns(
    code: &str,
    source: &str,
    bundler: BundlerType,
    patterns: &[SignalPattern],
) -> Vec<DetectionSignal> {
    patterns
        .iter()
        .filter(|(test, _)| test(code))
        .map(|(_, pattern)| bundler_signal(source, pattern, bundler))
        .collect()
}

// ---- webpack.ts ---------------------------------------------------------------

/// `/__webpack_require__/`, `/__webpack_modules__/`, `/webpackChunk/`.
pub fn detect_webpack(code: &str) -> Vec<DetectionSignal> {
    match_patterns(
        code,
        "webpack",
        BundlerType::Webpack,
        &[
            (|c| c.contains("__webpack_require__"), "__webpack_require__"),
            (|c| c.contains("__webpack_modules__"), "__webpack_modules__"),
            (|c| c.contains("webpackChunk"), "webpackChunk"),
        ],
    )
}

// ---- browserify.ts ------------------------------------------------------------

/// `/\.exports\s*\}/`.
fn has_exports_close(code: &str) -> bool {
    code.match_indices(".exports").any(|(i, lit)| {
        let at = skip_js_space(code, i + lit.len());
        eat(code, at, "}").is_some()
    })
}

/// TS `detectBrowserify`: nothing when `__webpack_require__` appears;
/// else `/\[0\]\.call\(/` + `/\.exports\s*\}/`, then `/installedModules/`.
pub fn detect_browserify(code: &str) -> Vec<DetectionSignal> {
    if code.contains("__webpack_require__") {
        return Vec::new();
    }
    let mut signals = Vec::new();
    if code.contains("[0].call(") && has_exports_close(code) {
        signals.push(bundler_signal(
            "browserify",
            "browserify module call pattern",
            BundlerType::Browserify,
        ));
    }
    if code.contains("installedModules") {
        signals.push(bundler_signal(
            "browserify",
            "installedModules (no webpack)",
            BundlerType::Browserify,
        ));
    }
    signals
}

// ---- esbuild.ts ---------------------------------------------------------------

/// `/\bvar\s+__export\b/`.
fn has_var_export(code: &str) -> bool {
    code.match_indices("var").any(|(i, lit)| {
        let after_var = i + lit.len();
        let at = skip_js_space(code, after_var);
        at > after_var
            && is_word_boundary(code, i)
            && eat(code, at, "__export").is_some_and(|end| is_word_boundary(code, end))
    })
}

/// `/\b__commonJS\b/`, `/\b__toESM\b/`, `/\b__toCommonJS\b/`,
/// `/\bvar\s+__export\b/`, `/\b__require\b/`.
pub fn detect_esbuild(code: &str) -> Vec<DetectionSignal> {
    match_patterns(
        code,
        "esbuild-bundler",
        BundlerType::Esbuild,
        &[
            (|c| contains_word(c, "__commonJS"), "__commonJS"),
            (|c| contains_word(c, "__toESM"), "__toESM"),
            (|c| contains_word(c, "__toCommonJS"), "__toCommonJS"),
            (has_var_export, "__export (esbuild definition)"),
            (|c| contains_word(c, "__require"), "__require"),
        ],
    )
}

// ---- parcel.ts ----------------------------------------------------------------

/// `/require\s*\(\s*["']_bundle_loader["']\s*\)/`.
fn has_bundle_loader_require(code: &str) -> bool {
    code.match_indices("require").any(|(i, lit)| {
        let at = eat(code, skip_js_space(code, i + lit.len()), "(");
        at.and_then(|at| eat_quote(code, skip_js_space(code, at)))
            .and_then(|at| eat(code, at, "_bundle_loader"))
            .and_then(|at| eat_quote(code, at))
            .and_then(|at| eat(code, skip_js_space(code, at), ")"))
            .is_some()
    })
}

/// `/parcelRequire/`, `require("_bundle_loader")`.
pub fn detect_parcel(code: &str) -> Vec<DetectionSignal> {
    match_patterns(
        code,
        "parcel",
        BundlerType::Parcel,
        &[
            (|c| c.contains("parcelRequire"), "parcelRequire"),
            (has_bundle_loader_require, "require(\"_bundle_loader\")"),
        ],
    )
}

// ---- bun.ts -------------------------------------------------------------------

/// `/^\s*\/\/\s*@bun\b/` — anchored at the start of the input (no `m`
/// flag), so a stray `@bun` later never matches.
fn has_bun_banner(code: &str) -> bool {
    let at = skip_js_space(code, 0);
    eat(code, at, "//")
        .and_then(|at| eat(code, skip_js_space(code, at), "@bun"))
        .is_some_and(|end| is_word_boundary(code, end))
}

/// `/\{exports:\s*\{\}\}/`.
fn has_cjs_factory(code: &str) -> bool {
    code.match_indices("{exports:").any(|(i, lit)| {
        let at = skip_js_space(code, i + lit.len());
        eat(code, at, "{}}").is_some()
    })
}

/// `/import\s*\{[^}]*createRequire[^}]*\}\s*from\s*["']node:module["']/`.
/// `[^}]*` cannot cross a `}`, so the braces' contents are exactly the text
/// up to the FIRST `}` after the `{`.
fn has_create_require_import(code: &str) -> bool {
    code.match_indices("import").any(|(i, lit)| {
        let Some(open) = eat(code, skip_js_space(code, i + lit.len()), "{") else {
            return false;
        };
        let Some(close) = code[open..].find('}').map(|k| open + k) else {
            return false;
        };
        code[open..close].contains("createRequire")
            && eat(code, skip_js_space(code, close + 1), "from")
                .and_then(|at| eat_quote(code, skip_js_space(code, at)))
                .and_then(|at| eat(code, at, "node:module"))
                .and_then(|at| eat_quote(code, at))
                .is_some()
    })
}

/// TS `detectBunBundler`: the `// @bun` banner alone is definitive; else
/// the `{exports:{}}` factory + the `createRequire` import together.
pub fn detect_bun_bundler(code: &str) -> Vec<DetectionSignal> {
    if has_bun_banner(code) {
        return vec![bundler_signal(
            "bun-bundler",
            "// @bun banner",
            BundlerType::Bun,
        )];
    }
    if has_cjs_factory(code) && has_create_require_import(code) {
        return vec![bundler_signal(
            "bun-bundler",
            "{exports:{}} + createRequire import",
            BundlerType::Bun,
        )];
    }
    Vec::new()
}

// ---- minifier.ts --------------------------------------------------------------

/// `/!0\b/` or `/!1\b/` (the `\b` sits after the digit, a word char, so it
/// is "the next char is non-word or end").
fn has_bool_coercion(code: &str) -> bool {
    ["!0", "!1"].iter().any(|lit| {
        code.match_indices(lit)
            .any(|(i, _)| is_word_boundary(code, i + 2))
    })
}

/// TS `detectTerser`: the universal-minification fallback, tier unknown.
pub fn detect_terser(code: &str) -> Vec<DetectionSignal> {
    let mut signals = Vec::new();
    if code.contains("void 0") {
        signals.push(minifier_signal(
            "terser",
            "void 0",
            MinifierType::Terser,
            DetectionTier::Unknown,
        ));
    }
    if has_bool_coercion(code) {
        signals.push(minifier_signal(
            "terser",
            "!0/!1 boolean coercion",
            MinifierType::Terser,
            DetectionTier::Unknown,
        ));
    }
    signals
}

/// `/\/\/ .+\.js\n/` over `code.slice(0, 200)`. `.` refuses every line
/// terminator, so the `\n` is the first terminator after `// `, and the
/// line's text between them is at least one character followed by `.js`.
fn has_esbuild_banner(head: &str) -> bool {
    head.match_indices("// ").any(|(i, lit)| {
        let rest = &head[i + lit.len()..];
        let Some((end, term)) = rest.char_indices().find(|&(_, c)| is_line_terminator(c)) else {
            return false;
        };
        let line = &rest[..end];
        term == '\n' && line.ends_with(".js") && line.chars().count() >= 4
    })
}

/// TS `detectEsbuildMinifier`.
pub fn detect_esbuild_minifier(code: &str) -> Vec<DetectionSignal> {
    if has_esbuild_banner(js_prefix(code, 200)) {
        return vec![minifier_signal(
            "esbuild-minifier",
            "esbuild banner comment",
            MinifierType::Esbuild,
            DetectionTier::Likely,
        )];
    }
    Vec::new()
}

/// TS `detectBunMinifier`: more than 10 matches of `/\$[a-zA-Z][a-zA-Z0-9]/g`.
/// A match cannot contain a `$`, so the global scan's non-overlapping
/// matches are exactly the `$` positions that satisfy the pattern.
pub fn detect_bun_minifier(code: &str) -> Vec<DetectionSignal> {
    const THRESHOLD: usize = 10;
    let bytes = code.as_bytes();
    let count = code
        .match_indices('$')
        .filter(|&(i, _)| {
            bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic)
                && bytes.get(i + 2).is_some_and(u8::is_ascii_alphanumeric)
        })
        .take(THRESHOLD + 1)
        .count();
    if count > THRESHOLD {
        return vec![minifier_signal(
            "bun-minifier",
            "$-prefixed mixed-case identifiers",
            MinifierType::Bun,
            DetectionTier::Likely,
        )];
    }
    Vec::new()
}

/// TS `SWC_HELPER_MARKERS`: swc's snake_case, multi-word runtime helpers.
const SWC_HELPER_MARKERS: [&str; 15] = [
    "_interop_require_default",
    "_interop_require_wildcard",
    "_class_call_check",
    "_create_class",
    "_create_super",
    "_sliced_to_array",
    "_to_consumable_array",
    "_object_spread",
    "_object_spread_props",
    "_async_to_generator",
    "_ts_generator",
    "_define_property",
    "_object_destructuring_empty",
    "_object_without_properties",
    "_tagged_template_literal",
];

/// TS `detectSwcMinifier`: `\b(?:m1|m2|...)\b`. The backtracking
/// alternation matches iff some marker occurs word-bounded on both sides.
pub fn detect_swc_minifier(code: &str) -> Vec<DetectionSignal> {
    if SWC_HELPER_MARKERS.iter().any(|m| contains_word(code, m)) {
        return vec![minifier_signal(
            "swc-minifier",
            "swc snake_case helper names",
            MinifierType::Swc,
            DetectionTier::Likely,
        )];
    }
    Vec::new()
}

/// TS `detectMinifier`: terser, esbuild, bun, swc — in that order.
pub fn detect_minifier(code: &str) -> Vec<DetectionSignal> {
    let mut signals = detect_terser(code);
    signals.extend(detect_esbuild_minifier(code));
    signals.extend(detect_bun_minifier(code));
    signals.extend(detect_swc_minifier(code));
    signals
}
