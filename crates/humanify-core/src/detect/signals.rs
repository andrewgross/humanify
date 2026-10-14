//! The per-bundler and per-minifier signal detectors (TS:
//! src/detection/signals/*.ts, WPB.1). Each TS regex is ported as a hand
//! matcher over `js_text`'s primitives; the TS pattern sits in each doc
//! comment so the two can be read side by side. Every bundler signal
//! carries its [`SignalStrength`]: the bundler verdict is the STRONGEST
//! signal's bundler (`detect::pick_bundler`; the ranking's rules are on
//! `SignalStrength`), never the first one listed. Signal order is still
//! part of the output (the `signals` list; the minifier verdict keeps the
//! first highest-tier minifier), so every detector emits in the TS's order.

use humanify_model::detection::{
    BundlerType, DetectionSignal, DetectionTier, MinifierType, SignalStrength,
};

use crate::modules::helper_shape::{ExportRegistrar, find_module_helper, first_export_registrar};

use super::js_text::{
    contains_word, count_shapes, eat, eat_quote, eat_shape, is_line_terminator, is_word_boundary,
    js_prefix, skip_js_space, skip_preamble,
};

/// A bundler signal; its tier follows from its strength
/// ([`SignalStrength::tier`]).
fn bundler_signal(
    source: &str,
    pattern: &str,
    bundler: BundlerType,
    strength: SignalStrength,
) -> DetectionSignal {
    DetectionSignal {
        source: source.to_string(),
        pattern: pattern.to_string(),
        bundler: Some(bundler),
        minifier: None,
        tier: strength.tier(),
        strength: Some(strength),
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
        strength: None,
    }
}

/// TS `SignalPattern`: a test over the scanned text + the pattern label.
type SignalPattern = (fn(&str) -> bool, &'static str);

/// TS `matchPatterns` (pattern-helper.ts): one signal per matching
/// pattern, in table order, all of one strength.
fn match_patterns(
    code: &str,
    source: &str,
    bundler: BundlerType,
    strength: SignalStrength,
    patterns: &[SignalPattern],
) -> Vec<DetectionSignal> {
    patterns
        .iter()
        .filter(|(test, _)| test(code))
        .map(|(_, pattern)| bundler_signal(source, pattern, bundler, strength))
        .collect()
}

// ---- webpack.ts ---------------------------------------------------------------

/// `/__webpack_require__/`, `/__webpack_modules__/`, `/webpackChunk/` —
/// webpack's own runtime names. Then the runtime's SHAPES, which survive a
/// minifier that renames every one of those names (exp105: webpack 5's
/// production build keeps none of them): the bootstrap opening on its
/// module table ([`opens_with_module_table`]), the require function's
/// call of a module (`M[id](module, module.exports, require)`, webpack 4's
/// `.call(module.exports, …)` form too) and webpack 5's export definer
/// (`for (k in d) R.o(d, k) && !R.o(e, k) && …`) — each a [`Shape`]
/// signal. `code` is the runtime prologue (`detect::detect_bundle`).
///
/// [`Shape`]: SignalStrength::Shape
pub fn detect_webpack(code: &str) -> Vec<DetectionSignal> {
    let mut signals = match_patterns(
        code,
        "webpack",
        BundlerType::Webpack,
        SignalStrength::OwnRuntimeName,
        &[
            (|c| c.contains("__webpack_require__"), "__webpack_require__"),
            (|c| c.contains("__webpack_modules__"), "__webpack_modules__"),
            (|c| c.contains("webpackChunk"), "webpackChunk"),
        ],
    );
    signals.extend(match_patterns(
        code,
        "webpack",
        BundlerType::Webpack,
        SignalStrength::Shape,
        &[
            (
                opens_with_module_table,
                "bootstrap opening on a module table (webpack's module shape)",
            ),
            (
                |c| {
                    WEBPACK_MODULE_CALLS
                        .iter()
                        .any(|p| count_shapes(c, p, 1) > 0)
                },
                "require function calling M[id](module, module.exports, require)",
            ),
            (
                |c| count_shapes(c, WEBPACK_DEFINER, 1) > 0,
                "export definer for (k in d) R.o(d,k) && !R.o(e,k)",
            ),
        ],
    ));
    signals
}

/// webpack's require function calls a module as `(module, module.exports,
/// require)` — webpack 5's direct call and the `.call(module.exports, …)`
/// form (webpack 4, and 5's non-arrow output).
const WEBPACK_MODULE_CALLS: [&str; 2] = [
    "]( $m , $m .exports , $r )",
    "].call( $m .exports , $m , $m .exports , $r )",
];

/// webpack 5's `__webpack_require__.d` (define getters for an ES module's
/// exports), minified: the own-property helper `.o` asked twice.
const WEBPACK_DEFINER: &str = "for ( var $k in $d ) $r .o( $d , $k ) && ! $r .o( $e , $k )";

/// Does the program open on webpack's bootstrap — an IIFE whose first
/// statement declares the MODULE TABLE: an object whose first key is a
/// module id (a number) and whose value is the module function
/// (`123(e,t,n){`, `123:(e,t,n)=>{`, `123:function(e){`, `123:e=>{`)?
/// Position matters: an app's own object of functions is never the first
/// thing in the file, and a bundled copy of a webpack build never opens
/// the bundle that holds it.
fn opens_with_module_table(code: &str) -> bool {
    let at = skip_preamble(code);
    let Some(at) = ["( ( ) => {", "( function ( ) {", "! function ( ) {"]
        .iter()
        .find_map(|p| eat_shape(code, at, p))
    else {
        return false;
    };
    let strict = skip_js_space(code, at);
    let at = ["\"use strict\"", "'use strict'"]
        .iter()
        .find_map(|d| eat(code, strict, d))
        .map_or(at, |end| eat_shape(code, end, ";").unwrap_or(end));
    let Some(at) = ["var $e = {", "let $e = {", "const $e = {"]
        .iter()
        .find_map(|p| eat_shape(code, at, p))
    else {
        return false;
    };
    let at = skip_js_space(code, at);
    let digits = code[at..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let at = at + digits;
    // a method `id(params){`, or `id:` + a function value
    if let Some(after) = eat_params(code, at) {
        return eat_shape(code, after, "{").is_some();
    }
    let Some(at) = eat_shape(code, at, ":") else {
        return false;
    };
    if let Some(after) = eat_shape(code, at, "function") {
        return eat_params(code, after).is_some();
    }
    let arrow = eat_params(code, at).or_else(|| eat_shape(code, at, "$p"));
    arrow.is_some_and(|after| eat_shape(code, after, "=>").is_some())
}

/// A module function's parameter list: `(` up to three identifiers `)`.
fn eat_params(code: &str, at: usize) -> Option<usize> {
    ["( )", "( $a )", "( $a , $b )", "( $a , $b , $c )"]
        .iter()
        .find_map(|p| eat_shape(code, at, p))
}

// ---- rollup -------------------------------------------------------------------

/// The commonjs plugin's guarded require function (`function requireX()
/// { if (hasRequiredX) return x; hasRequiredX = 1; … }`).
const ROLLUP_REQUIRE_GUARD: &str = "if ( $f ) return $m ; $f = 1";

/// The commonjs plugin's `getDefaultExportFromCjs` (and its siblings that
/// read `default` the same way).
const ROLLUP_DEFAULT_EXPORT: &str =
    ".__esModule && Object.prototype.hasOwnProperty.call( $e , \"default\"|'default' )";

/// The commonjs plugin's `commonjsGlobal`, minified and as written.
const ROLLUP_COMMONJS_GLOBAL: [&str; 2] = [
    "\"undefined\" != typeof globalThis ? globalThis : \"undefined\" != typeof window ? window : \"undefined\" != typeof global ? global : \"undefined\" != typeof self ? self : { }",
    "typeof globalThis !== 'undefined'|\"undefined\" ? globalThis : typeof window !== 'undefined'|\"undefined\" ? window : typeof global !== 'undefined'|\"undefined\" ? global : typeof self !== 'undefined'|\"undefined\" ? self : { }",
];

/// rollup's own interop for a CommonJS output's namespace import of an
/// external (`_interopNamespaceDefault`), minified and as written.
const ROLLUP_INTEROP_NAMESPACE: [&str; 2] = [
    "Object.create( null ) ; return $e && Object.keys( $e ).forEach( function ( $k ) { if ( \"default\" !== $k )",
    "Object.create( null ) ; if ( $e ) { Object.keys( $e ).forEach( function ( $k ) { if ( $k !== 'default'|\"default\" )",
];

/// Rollup writes NO runtime: a scope-hoisted bundle is its modules'
/// statements, renamed. What it does write is interop: its own
/// `_interopNamespaceDefault` (a CommonJS output's namespace import of an
/// external — a Node builtin, say) and, when the app pulls in CommonJS,
/// `@rollup/plugin-commonjs`'s helpers and guards — each a [`Shape`]
/// signal: two guarded require functions (one is a pattern hand-written
/// code can have), `getDefaultExportFromCjs`'s read of `default`,
/// `commonjsGlobal`'s globalThis/window/global/self chain. A rollup build
/// with none of them stays unknown (the fallback). Text that holds another bundler's runtime — a module
/// helper (Bun, esbuild) or webpack's — is not rollup's output, whatever
/// its bundled code holds (a dependency built with rollup): nothing.
///
/// [`Shape`]: SignalStrength::Shape
pub fn detect_rollup(code: &str) -> Vec<DetectionSignal> {
    if find_module_helper(code).is_some() || !detect_webpack(code).is_empty() {
        return Vec::new();
    }
    match_patterns(
        code,
        "rollup",
        BundlerType::Rollup,
        SignalStrength::Shape,
        &[
            (
                |c| count_shapes(c, ROLLUP_REQUIRE_GUARD, 2) == 2,
                "commonjs plugin's guarded require functions (if (r) return m; r = 1)",
            ),
            (
                |c| count_shapes(c, ROLLUP_DEFAULT_EXPORT, 1) > 0,
                "commonjs plugin's getDefaultExportFromCjs",
            ),
            (
                |c| {
                    ROLLUP_COMMONJS_GLOBAL
                        .iter()
                        .any(|p| count_shapes(c, p, 1) > 0)
                },
                "commonjs plugin's commonjsGlobal",
            ),
            (
                |c| {
                    ROLLUP_INTEROP_NAMESPACE
                        .iter()
                        .any(|p| count_shapes(c, p, 1) > 0)
                },
                "rollup's _interopNamespaceDefault",
            ),
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
/// else `/\[0\]\.call\(/` + `/\.exports\s*\}/` (a shape), then
/// `/installedModules/` — one common word, so a weak token: `likely`, a
/// verdict the toolchain does not act on (review R19).
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
            SignalStrength::Shape,
        ));
    }
    if code.contains("installedModules") {
        signals.push(bundler_signal(
            "browserify",
            "installedModules (no webpack)",
            BundlerType::Browserify,
            SignalStrength::WeakToken,
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
/// `/\bvar\s+__export\b/`, `/\b__require\b/` — esbuild's runtime helper
/// names, which Bun's runtime writes too: shared helper names. Then the
/// module helper's SHAPE, which survives `--minify` (spec I2): a form only
/// esbuild writes ([`esbuild_only`] — the `try` guard,
/// the indirect call), or any form whose bundle's FIRST `__export`
/// registrar is esbuild's ([`first_export_registrar`]) — each a [`Shape`]
/// signal. `code` is the runtime prologue (`detect::detect_bundle`).
///
/// [`Shape`]: SignalStrength::Shape
/// [`esbuild_only`]: crate::modules::helper_shape::ModuleHelperShape::esbuild_only
pub fn detect_esbuild(code: &str) -> Vec<DetectionSignal> {
    let mut signals = match_patterns(
        code,
        "esbuild-bundler",
        BundlerType::Esbuild,
        SignalStrength::SharedHelperName,
        &[
            (|c| contains_word(c, "__commonJS"), "__commonJS"),
            (|c| contains_word(c, "__toESM"), "__toESM"),
            (|c| contains_word(c, "__toCommonJS"), "__toCommonJS"),
            (has_var_export, "__export (esbuild definition)"),
            (|c| contains_word(c, "__require"), "__require"),
        ],
    );
    if let Some(helper) = find_module_helper(code) {
        let pattern = if helper.esbuild_only() {
            Some("module helper shape (esbuild's guarded or indirect form)")
        } else if first_export_registrar(code) == Some(ExportRegistrar::Esbuild) {
            Some("module helper shape + esbuild's __export registrar")
        } else {
            None
        };
        if let Some(pattern) = pattern {
            signals.push(bundler_signal(
                "esbuild-bundler",
                pattern,
                BundlerType::Esbuild,
                SignalStrength::Shape,
            ));
        }
    }
    signals
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

/// `/parcelRequire/`, `require("_bundle_loader")` — parcel's own runtime
/// names.
pub fn detect_parcel(code: &str) -> Vec<DetectionSignal> {
    match_patterns(
        code,
        "parcel",
        BundlerType::Parcel,
        SignalStrength::OwnRuntimeName,
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

/// TS `detectBunBundler`: the `// @bun` banner (the strongest signal
/// there is); else the `{exports:{}}` factory + the `createRequire` import
/// together (a shape); else Bun's own `__export` registrar as the bundle's
/// first ([`first_export_registrar`]: `configurable` + `set`, which
/// `--target node` builds keep with no banner — a shape); else the module
/// helper in the plain form both bundlers write ([`find_module_helper`],
/// spec I2) — a [`SharedShape`]: an input nothing more specific names gets
/// Bun's pieces.
///
/// [`SharedShape`]: SignalStrength::SharedShape
pub fn detect_bun_bundler(code: &str) -> Vec<DetectionSignal> {
    if has_bun_banner(code) {
        return vec![bundler_signal(
            "bun-bundler",
            "// @bun banner",
            BundlerType::Bun,
            SignalStrength::Banner,
        )];
    }
    if has_cjs_factory(code) && has_create_require_import(code) {
        return vec![bundler_signal(
            "bun-bundler",
            "{exports:{}} + createRequire import",
            BundlerType::Bun,
            SignalStrength::Shape,
        )];
    }
    if first_export_registrar(code) == Some(ExportRegistrar::Bun) {
        return vec![bundler_signal(
            "bun-bundler",
            "Bun's __export registrar (configurable + set)",
            BundlerType::Bun,
            SignalStrength::Shape,
        )];
    }
    if find_module_helper(code).is_some_and(|h| !h.esbuild_only()) {
        return vec![bundler_signal(
            "bun-bundler",
            "module helper shape (the plain form Bun and esbuild share)",
            BundlerType::Bun,
            SignalStrength::SharedShape,
        )];
    }
    Vec::new()
}

/// Bun ends every sourcemap debug ID with `64756E2164756E21` — "dun!dun!"
/// in hex — and writes it as the file's LAST line, `//# debugId=<32 hex>`.
/// Read from the end of the whole input (the prologue scan never reaches
/// it): Bun's own mark, as strong as a runtime name. Present only when the
/// build wrote a sourcemap; the one Bun signal a bundle with no runtime at
/// all (a single ESM module) can carry.
pub fn detect_bun_debug_id(code: &str) -> Vec<DetectionSignal> {
    const PREFIX: &str = "//# debugId=";
    const BUN_SUFFIX: &str = "64756E2164756E21";
    let last_line = code.trim_end().rsplit('\n').next().unwrap_or("");
    let is_bun = last_line.trim().strip_prefix(PREFIX).is_some_and(|id| {
        id.len() == 32
            && id.bytes().all(|b| b.is_ascii_hexdigit())
            && id[16..].eq_ignore_ascii_case(BUN_SUFFIX)
    });
    if is_bun {
        return vec![bundler_signal(
            "bun-bundler",
            "Bun's sourcemap debug ID (…64756E2164756E21)",
            BundlerType::Bun,
            SignalStrength::OwnRuntimeName,
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

/// TS `detectSwcMinifier`: `\b(?:m1|m2|...)\b`. The backtracking
/// alternation matches iff some marker occurs word-bounded on both sides.
/// The markers are swc's helper names that Babel does not share — the
/// one swc list's `detection_marker` entries
/// (`rename::eligibility::SWC_HELPERS`, toolchain review R20).
pub fn detect_swc_minifier(code: &str) -> Vec<DetectionSignal> {
    if crate::rename::eligibility::SWC_HELPERS
        .iter()
        .filter(|h| h.detection_marker)
        .any(|h| contains_word(code, h.name))
    {
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
