//! CJS module classification — the Bun original (WP1.5, TS:
//! `src/analysis/bun-module-classification.ts`, plus the one function WP1.5
//! needs from `src/shared/bun-helpers.ts` — identifyBunCjsFactory; that
//! file's ledger row is WP5.3 → core::emit; the rest of it stays there),
//! extended to esbuild's module form (exp075): the SECOND bundler, whose
//! factories differ in exactly two wrapper shapes — the factory argument
//! arrives as an OBJECT with one keyed method (whose key is the module's
//! original source path) and the unminified helper writes its
//! `{ exports: {} }` marker spaced. Submodules: `wrapper`
//! (wrapper-detection.ts), `soundness` (soundness.ts),
//! `known_globals` (known-globals.ts).
//!
//! A bundler wraps every CJS module in a `var X = HELPER(fn)` factory
//! that survives minification; factories are essentially guaranteed
//! third-party, so the rename pipeline skips their bindings/functions and
//! the split extracts them to vendor/ — except an app written in CommonJS,
//! whose own modules are factories too: [`app_factories`] (exp172) is the
//! one owner of which factories stay in the app. Classifying them is what makes the
//! function graph's member set match the TS oracle's (the factory-body
//! functions are skipped there).
//!
//! Scope containment by SPAN, not by scope object: a factory body's span
//! contains exactly the nodes in its scope (well-nested code), and spans
//! are the dump's identity — no scope-map plumbing. The TS's scope-walk
//! (isInsideFactoryBody's 64-deep parent chain) and a span containment
//! test agree on well-nested code; the four-pair gate is the judge.
//!
//! Every regex from the TS is emulated byte-for-byte WITHOUT a regex
//! crate — each emulation documents the exact pattern it mirrors, and the
//! probe (test/parity/wp1.5-probe.mjs) holds the TS-derived expectations.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_estree::{CompactSerializer, ESTree};
use oxc_span::{GetSpan, Span};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::babel_view::unparen;
use crate::hash::forms::{CanonicalForm, MATCH_KEY};
use crate::hash::serialize::{SymbolTables, canonical_serialize};
use identity::readers::{BannerInfo, parse_banner};

pub mod app_factories;
pub mod helper_shape;
pub mod identity;
pub mod inline_modules;
pub mod known_globals;
pub mod lex;
pub mod soundness;
pub mod text_assets;
pub mod vendor_content;
pub mod vendor_names;
pub mod vendor_pairing;
pub mod wrapper;

/// The identified CJS factory helper (bun-helpers.ts's IdentifiedHelper).
pub struct IdentifiedHelper {
    pub name: String,
    pub start_offset: u32,
}

/// esbuild's CJS runtime helper name — preserved verbatim in every
/// unminified esbuild build (minified builds minify the name away; the
/// helper's shape, [`helper_shape::find_module_helper`], survives).
pub const ESBUILD_CJS_HELPER: &str = "__commonJS";

/// JS `\s` — the ASCII subset a minified bundle's markers use. (The TS
/// regex `\s` also takes Unicode spaces; a non-ASCII whitespace byte inside
/// `{exports: … {}}` or around an `=` would be a parity miss — the gate is
/// the judge on the real bundles.)
fn is_js_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

fn starts_with_at(bytes: &[u8], at: usize, pat: &[u8]) -> bool {
    bytes.len() >= at + pat.len() && &bytes[at..at + pat.len()] == pat
}

/// The marker Bun emits inside its CJS factory helper's body — the TS
/// pattern is `/\{exports:\s*\{\}\}/g`: the whitespace after the colon is
/// OPTIONAL (the real minified bundles carry `{exports:{}}`, no space).
const MARKER_HEAD: &[u8] = b"{exports:";
const MARKER_TAIL: &[u8] = b"{}}";

/// Scan the source for the factory-body marker and recover the binding that
/// declares the helper (bun-helpers.ts:30-56): the LEFTMOST match of
/// `(?:(?:var|let|const)\s+|,)([$\w]+)\s*=\s*[^;]*$` over the bounded
/// lookback window (2000 chars — a bounded window keeps a miss from
/// re-slicing an ever-growing prefix of a 20MB source). `.match()` without
/// /g takes the leftmost match; on a comma-joined declaration that is the
/// FIRST declarator, not the one nearest the marker.
pub fn identify_bun_cjs_factory(source: &str) -> Option<IdentifiedHelper> {
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    loop {
        let head = find_sub(bytes, search_from, MARKER_HEAD)?;
        // `\s*` between the head and `{}}`; a miss continues the scan one
        // byte past the head (the TS regex engine advances char-wise on a
        // failed attempt), a hit past the whole match.
        let mut p = head + MARKER_HEAD.len();
        while p < bytes.len() && is_js_ws(bytes[p]) {
            p += 1;
        }
        if !starts_with_at(bytes, p, MARKER_TAIL) {
            search_from = head + 1;
            continue;
        }
        let slice_start = lookback_start(source, head, 2000);
        let before = &source[slice_start..head];
        if let Some((match_start, name)) = leftmost_binding_match(before) {
            return Some(IdentifiedHelper {
                name,
                start_offset: (slice_start + match_start) as u32,
            });
        }
        search_from = p + MARKER_TAIL.len();
    }
}

/// The declared binding site of `__commonJS` — the ESBUILD CJS helper
/// (exp075, verified against real esbuild 0.27.2 output). An unminified
/// esbuild build writes `{ exports: {} }` (SPACED — bun's tight
/// `{exports:{}}` marker does not match) but keeps the helper's NAME, the
/// same name the detection stage keys its definitive esbuild verdict on —
/// so the helper is identified by its `var|let|const NAME =` (or
/// `, NAME =`) DECLARATION, leftmost. Returns None when the name only
/// appears as a reference (word-bounded check on both sides).
pub fn identify_esbuild_cjs_factory(source: &str) -> Option<IdentifiedHelper> {
    let bytes = source.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    for (rel, _) in source.match_indices(ESBUILD_CJS_HELPER) {
        let at = rel;
        let after = at + ESBUILD_CJS_HELPER.len();
        // Word-bounded on both sides, and followed by `=` — a declaration,
        // not a use.
        let mut p = after;
        while p < bytes.len() && is_js_ws(bytes[p]) {
            p += 1;
        }
        let bounded = at > 0
            && !(is_ident(bytes[at - 1]) || bytes[at - 1] == b'"' || bytes[at - 1] == b'\'')
            && bytes.get(p) == Some(&b'=');
        if !bounded {
            continue;
        }
        // Preceded by `var|let|const` + whitespace, or by a `,` (a joined
        // declaration). The match starts at the keyword / comma.
        let mut q = at;
        while q > 0 && is_js_ws(bytes[q - 1]) {
            q -= 1;
        }
        if q > 0 && bytes[q - 1] == b',' {
            return Some(IdentifiedHelper {
                name: ESBUILD_CJS_HELPER.to_string(),
                start_offset: (q - 1) as u32,
            });
        }
        for kw in ["var", "let", "const"] {
            let Some(kw_at) = q.checked_sub(kw.len()) else {
                continue;
            };
            // keyword, then the whitespace run [kw_at+len, at), at least
            // one byte (q sits at the run's start — q == at means there
            // was NO whitespace, a miss), then the name at `at`.
            let ws_gap = &bytes[kw_at + kw.len()..at];
            if starts_with_at(bytes, kw_at, kw.as_bytes())
                && (kw_at == 0 || !is_ident(bytes[kw_at - 1]))
                && !ws_gap.is_empty()
                && ws_gap.iter().all(|&b| is_js_ws(b))
            {
                return Some(IdentifiedHelper {
                    name: ESBUILD_CJS_HELPER.to_string(),
                    start_offset: kw_at as u32,
                });
            }
        }
    }
    None
}

/// The CJS factory helper of EITHER bundler — the one owner of that
/// question for every classification site: the helper by SHAPE first
/// ([`helper_shape::find_module_helper`]: the declarator holding the
/// `(B = {exports: {}}).exports, B)` core, in every form both bundlers
/// write, minified or not), then bun's tight `{exports:{}}` marker with
/// its leftmost-binding rule (the frozen WP1.5 surface), then esbuild's
/// named `__commonJS` declaration. On every real bundle where the old
/// scans were right the shape names the same binding; it is first
/// because the marker scan names the FIRST declarator of a joined
/// declaration (a re-minified esbuild build's `var cye=Object.create,…,
/// ee=(e,t)=>…`) and the name scan needs a name `--minify` removes.
pub fn identify_cjs_factory(source: &str) -> Option<IdentifiedHelper> {
    helper_shape::find_module_helper(source)
        .map(|shape| shape.helper)
        .or_else(|| identify_bun_cjs_factory(source))
        .or_else(|| identify_esbuild_cjs_factory(source))
}

/// The byte offset of JS `Math.max(0, at - units)` for byte offset `at`:
/// `units` UTF-16 code units back, never inside a char. Where the JS index
/// would land between a surrogate pair's halves, the JS slice starts with a
/// lone low surrogate — a character no pattern here can match — so starting
/// after the whole char is equivalent. (Counting bytes instead shrank the
/// window on non-ASCII text and could slice mid-char — a panic.)
fn lookback_start(source: &str, at: usize, units: usize) -> usize {
    let mut used = 0;
    for (i, c) in source[..at].char_indices().rev() {
        used += c.len_utf16();
        if used > units {
            return i + c.len_utf8();
        }
    }
    0
}

fn find_sub(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|r| r + from)
}

/// `(?:(?:var|let|const)\s+|,)([$\w]+)\s*=\s*[^;]*$` — the LEFTMOST match
/// over `before`: position 0..len in order; at each position the keyword
/// alternatives, then the comma; the first position whose remainder matches
/// wins. The regex is unanchored, so a position inside a word can match
/// (`covar x=...` matches at the embedded "var") — the scan is per byte.
pub(crate) fn leftmost_binding_match(before: &str) -> Option<(usize, String)> {
    let bytes = before.as_bytes();
    for i in 0..bytes.len() {
        let keyword_end = if starts_with_at(bytes, i, b"const") {
            Some(i + 5)
        } else if starts_with_at(bytes, i, b"var") || starts_with_at(bytes, i, b"let") {
            Some(i + 3)
        } else {
            None
        };
        if let Some(after_kw) = keyword_end {
            // `\s+` — at least one whitespace char.
            let mut j = after_kw;
            let ws_start = j;
            while j < bytes.len() && is_js_ws(bytes[j]) {
                j += 1;
            }
            if j > ws_start
                && let Some(name) = name_assign_tail(bytes, j)
            {
                return Some((i, name));
            }
        }
        if bytes[i] == b','
            && let Some(name) = name_assign_tail(bytes, i + 1)
        {
            return Some((i, name));
        }
    }
    None
}

/// `([$\w]+)\s*=\s*[^;]*$` from `j`: an identifier-ish name (`\w` plus `$`,
/// ASCII — the TS class), optional whitespace, `=`, optional whitespace,
/// then a tail to the end of the window with NO semicolon.
fn name_assign_tail(bytes: &[u8], mut j: usize) -> Option<String> {
    let name_start = j;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'$')
    {
        j += 1;
    }
    if j == name_start {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[name_start..j]).into_owned();
    while j < bytes.len() && is_js_ws(bytes[j]) {
        j += 1;
    }
    if bytes.get(j) != Some(&b'=') {
        return None;
    }
    j += 1;
    while j < bytes.len() && is_js_ws(bytes[j]) {
        j += 1;
    }
    if bytes[j..].contains(&b';') {
        return None;
    }
    Some(name)
}

/// Where a factory's name came from (the TS nameSource union). "llm" is set
/// post-cascade by the adapter's LLM pass (`vendor_names::
/// name_fallback_factories_with_llm`) over fallback records only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameSource {
    /// Named from the module's DECLARED identity (finding #95,
    /// [`identity`]): what its bundler kept or its package wrote — the
    /// record's `identity` says which reader and on what evidence.
    Declared,
    CarryOver,
    /// Carried from the prior module whose declared identity is the same
    /// and unique on both sides (finding #95,
    /// [`identity::pair_by_declared_identity`]) — run state only: the
    /// manifest label stays the prior's.
    DeclaredPair,
    /// Carried from the prior module this one was paired with BY CONTENT
    /// (finding #90, [`vendor_pairing`]) — run state only: the manifest
    /// label stays the prior's.
    ContentPair,
    Llm,
    Fallback,
    /// An app TEXT asset ([`text_assets`]): named from its own text, never
    /// a package name.
    Asset,
    /// Named for the package the library index's placement answer gave the
    /// module (`index_path`, library-names plan step 4): the file
    /// `vendor/<package>/<file>` is the index's real npm spelling, and the
    /// model is never asked for it. Run-only provenance: the manifest
    /// label is written the same way, so a later carry keeps it.
    Index,
}

impl NameSource {
    /// The manifest's `nameSource` string — the written field.
    pub fn as_str(self) -> &'static str {
        match self {
            NameSource::Declared => "declared",
            NameSource::CarryOver => "carry-over",
            NameSource::DeclaredPair => "declared-pair",
            NameSource::ContentPair => "content-pair",
            NameSource::Llm => "llm",
            NameSource::Fallback => "fallback",
            NameSource::Asset => "asset",
            NameSource::Index => "index",
        }
    }

    /// Read a prior manifest's `nameSource` label back, for a carry. A
    /// missing or unknown label reads as "carry-over": the name was
    /// carried, from where is not recorded.
    pub fn of_prior_label(label: Option<&str>) -> NameSource {
        [
            NameSource::Declared,
            NameSource::Llm,
            NameSource::Fallback,
            NameSource::Asset,
            NameSource::Index,
        ]
        .into_iter()
        .find(|s| Some(s.as_str()) == label)
        .unwrap_or(NameSource::CarryOver)
    }
}

/// A name the prior release's manifest offers for carrying, with the label
/// that manifest wrote for it — where the name ORIGINALLY came from, which
/// a carry keeps (finding #71: relabelling every carried library
/// "carry-over" churned ~3,000 manifest lines per hop for nothing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarriedName {
    pub name: String,
    pub origin: NameSource,
}

impl CarriedName {
    /// A prior entry's name and its raw `nameSource` field.
    pub fn new(name: impl Into<String>, label: Option<&str>) -> CarriedName {
        CarriedName {
            name: name.into(),
            origin: NameSource::of_prior_label(label),
        }
    }
}

/// One detected CJS factory (CjsFactoryRecord's dumped fields).
pub struct FactoryRecord {
    /// The minified handle (the declarator's id name).
    pub factory_var: String,
    /// The whole VariableDeclarator's span (byteRange).
    pub span: Span,
    /// The factory BODY function's span (the scope-containment region).
    pub body_span: Span,
    /// 1-indexed start/end line of the declarator (the TS lineRange).
    pub line_range: (u32, u32),
    /// First 16 hex of sha256 of the DECLARATOR's source slice (the TS
    /// contentHash — in-bundle dedup; NOT cross-version-stable, Bun re-rolls
    /// the minified identifiers).
    pub content_hash: String,
    /// The body's structural hash (blurred) — the cross-version join key.
    pub structural_hash: String,
    /// The banner's stripped, trimmed text — a bang-block's value minus the
    /// leading `!` (the TS parseBanner stores text = raw.replace(/^!/,'').trim(),
    /// so the `!` never reaches the record).
    pub banner_text: Option<String>,
    pub banner_package: Option<String>,
    pub banner_version: Option<String>,
    /// Final assigned name — set by the naming cascade (`nameCjsFactories`).
    pub name: Option<String>,
    /// How THIS run got the name — set by the naming cascade (run state:
    /// the LLM pass reads it, the per-source counts report it).
    pub name_source: Option<NameSource>,
    /// Where the NAME originally came from — the manifest's `nameSource`
    /// label. Equal to `name_source`, except a carry, which keeps the
    /// prior manifest's label so an unchanged library's entry is
    /// byte-identical across hops (finding #71).
    pub name_origin: Option<NameSource>,
    /// The module's ORIGINAL source path, when the bundler kept it —
    /// esbuild's unminified form uses it as the factory object's key
    /// (exp075). Absent for bun and for ANY minified build. Since finding
    /// #95 it is one input of the declared identity (the esbuild
    /// `ModuleIdentitySource` hands it to the readers); nothing else reads
    /// it.
    pub source_path: Option<String>,
    /// The factory came from esbuild's object-METHOD form (its span carries
    /// no `function` keyword) — [`FactoryArg::object_method`].
    pub object_method: bool,
    /// The module's whole body is `module.exports = <text>` (a string, or a
    /// template literal with no `${}`): the text. The shape half of the app
    /// text asset rule ([`text_assets`]); None for anything else.
    pub exported_text: Option<String>,
    /// What a content pair carried from the prior release (finding #90) —
    /// the prior file and identifier the unpack reuses. None otherwise.
    pub carried: Option<vendor_pairing::CarriedIdentity>,
    /// What the module declares about itself (finding #95,
    /// [`identity::declare`]) — set by the unpack before any name is
    /// chosen; None for an app text asset or when no reader named it.
    pub identity: Option<identity::DeclaredIdentity>,
    /// Declared facts too weak to name the module (a version constant, a
    /// repository URL) — shown to the model, never a name.
    pub hints: Vec<String>,
    /// The names-mode placement answer (library-names plan step 4): the
    /// full vendor path the library index gives this wholly claimed module
    /// (`vendor/<package>/<file>.js`). Set by the unpack before any name
    /// is chosen; the naming ladder's Index rung and the file-name chooser
    /// are its only readers. None: the planner left the module in place —
    /// unclaimed, partial, contested, or the index disagrees — or no
    /// names mode is on.
    pub index_path: Option<String>,
    /// Why the names-mode planner left the module in place, when it did
    /// (`unclaimed`, `partial`, `contested`, `index-disagrees`): the
    /// diagnostic side of [`Self::index_path`], read by the `discover`
    /// verb's dump (exp157). None on a move — a file race's loser re-paths
    /// inside its package (D1, plan step 7), it does not stay.
    pub index_leave: Option<&'static str>,
    /// The whole-module vendor content key (`modules::vendor_content`) the
    /// names-mode planner read the index by — the diagnostic side of the
    /// placement decision (exp157). None when the body does not key.
    pub module_key: Option<String>,
}

/// The classification: the helper var + every factory, in source order.
pub struct BunModuleClassification {
    pub helper_var: String,
    pub factories: Vec<FactoryRecord>,
    /// The container's statement spans, in source order (the wrapper body's
    /// or the program's) — the scope planner's container view.
    pub container: Vec<Span>,
    /// WHERE each vendor name came from, once `name_cjs_factories` has run.
    /// Undefined until then — absent is not "all zero" (the TS note).
    pub name_counts: Option<FactoryNameCounts>,
}

/// Counts of factories named by each source (FactoryNameCounts).
#[derive(Default)]
pub struct FactoryNameCounts {
    /// Named from a declared identity (finding #95) — counted after the
    /// pairings.
    pub declared: usize,
    pub carry_over: usize,
    /// Carried by an equal, unique declared identity (finding #95).
    pub declared_pair: usize,
    /// Carried by content (finding #90) — counted after the cascade.
    pub content_pair: usize,
    pub llm: usize,
    pub fallback: usize,
    /// App text assets, decided before the cascade.
    pub asset: usize,
    /// Named for the library index's placement answer (names mode, after
    /// the carry rung) — never asked of the model.
    pub index: usize,
}

/// The structural hash of a factory body FUNCTION (arrow or function
/// expression; None for anything else) under `tables` and `form` — the
/// cross-version join key, via the same canonical serializer the function
/// graph uses. The ONE owner: the classification hashes under
/// [`MATCH_KEY`] (the manifest's `structuralHash`, stamped with its
/// version) and the content re-key ([`vendor_content`]) under
/// `hash::forms::VENDOR_CONTENT`.
pub fn factory_structural_hash(
    function: &oxc_ast::ast::Expression<'_>,
    tables: &SymbolTables,
    form: &CanonicalForm,
) -> Option<String> {
    let mut ser = CompactSerializer::new(false, false);
    match function {
        oxc_ast::ast::Expression::ArrowFunctionExpression(a) => a.serialize(&mut ser),
        oxc_ast::ast::Expression::FunctionExpression(f) => f.serialize(&mut ser),
        _ => return None,
    }
    let estree = ser.into_string();
    let mut de = serde_json::Deserializer::from_str(&estree);
    de.disable_recursion_limit();
    let body_json: Value = Deserialize::deserialize(&mut de).unwrap_or(Value::Null);
    Some(canonical_serialize(&body_json, tables, form).hash)
}

/// A factory call argument, unwrapped: the factory FUNCTION, the module's
/// original source path when the bundler kept it, and whether the function
/// is an object METHOD — esbuild's `{"src/x.js"(a, b) { … }}` spelling,
/// whose function span carries NO `function` keyword, so a span slice of
/// it is not a complete expression (the extraction must supply one).
#[derive(Clone)]
pub struct FactoryArg<'a> {
    pub function: &'a oxc_ast::ast::Expression<'a>,
    /// esbuild's object-key source path; None in every other form.
    pub source_path: Option<String>,
    /// The function came from an object METHOD declaration.
    pub object_method: bool,
}

/// The factory function inside a `HELPER(…)` call's first argument, in
/// either bundler's observed form (exp075, verified against real esbuild
/// 0.27.2 output): bun passes the function DIRECTLY (arrow or function
/// expression); esbuild's unminified form passes an OBJECT with a single
/// keyed method (or property) whose KEY IS the module's original source
/// path. Returns the unwrapping — the ONE owner for the classification
/// here and the extraction in `unpack::bun`.
pub fn factory_arg_function<'a>(arg: &'a oxc_ast::ast::Expression<'a>) -> Option<FactoryArg<'a>> {
    let arg = unparen(arg);
    match arg {
        e @ (oxc_ast::ast::Expression::ArrowFunctionExpression(_)
        | oxc_ast::ast::Expression::FunctionExpression(_)) => Some(FactoryArg {
            function: e,
            source_path: None,
            object_method: false,
        }),
        oxc_ast::ast::Expression::ObjectExpression(obj) => {
            let [oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop)] =
                obj.properties.as_slice()
            else {
                return None;
            };
            if prop.kind != oxc_ast::ast::PropertyKind::Init || prop.computed {
                return None;
            }
            let function = unparen(&prop.value);
            if !matches!(
                function,
                oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                    | oxc_ast::ast::Expression::FunctionExpression(_)
            ) {
                return None;
            }
            // The key: a string literal is esbuild's own spelling; a bare
            // identifier (an esbuild source path that happens to be a valid
            // unquoted key, or a generated name) is read by its text.
            let source_path = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
                oxc_ast::ast::PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
                _ => None,
            };
            Some(FactoryArg {
                function,
                source_path,
                object_method: prop.method,
            })
        }
        _ => None,
    }
}

/// `lib_<first 8 chars of the hash>` (hashFallbackName) — THE spelling of
/// "nothing identified this module" (toolchain review R28). A classified
/// module passes its structural hash (rename-invariant, so the name holds
/// across releases); text with no classification goes through
/// [`content_fallback_name`].
pub fn hash_fallback_name(structural_hash: &str) -> String {
    format!("lib_{}", &structural_hash[..8.min(structural_hash.len())])
}

/// The fallback for a module body that has NO classification to take a
/// structural hash from — the unpack's file-name floor for an unrecorded
/// module and the fresh grouping's vendor bucket: [`hash_fallback_name`]
/// over `sha256(body_text)`. Two inputs, one spelling: on the real Bun
/// pair every one of the 1,493 vendor files is named from a structural
/// hash (this floor wrote none), as on the esbuild fixtures.
pub fn content_fallback_name(body_text: &str) -> String {
    let digest = Sha256::digest(body_text.as_bytes());
    let hex: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    hash_fallback_name(&hex)
}

/// True when `name` is the hash fallback — identifies NO package, only that
/// nothing else identified the module. Callers deciding whether a name is a
/// real library identity must test THIS rather than the name source: a
/// fallback name carried over from a prior release arrives as carry-over,
/// and a source test silently flips on the second hop.
pub fn is_hash_fallback_name(name: &str) -> bool {
    // /^lib_[0-9a-f]{8}$/
    let Some(rest) = name.strip_prefix("lib_") else {
        return false;
    };
    rest.len() == 8
        && rest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Per-factory position within its structuralHash group, plus each group's
/// size. One hash can legitimately cover SEVERAL DISTINCT modules (re-export
/// shims are structurally identical but proxy different libraries), so a
/// hash alone cannot key a name; position within the group does.
struct HashIndex {
    /// Position of each factory (by Vec index) within its hash group.
    occurrence: Vec<usize>,
    group_size: HashMap<String, usize>,
}

fn index_by_hash(factories: &[FactoryRecord]) -> HashIndex {
    let mut occurrence = Vec::with_capacity(factories.len());
    let mut group_size: HashMap<String, usize> = HashMap::new();
    for factory in factories {
        let n = group_size
            .entry(factory.structural_hash.clone())
            .or_insert(0);
        occurrence.push(*n);
        *n += 1;
    }
    HashIndex {
        occurrence,
        group_size,
    }
}

/// The prior release's name for this factory, or None. The group must be
/// INTACT — the same number of factories share the hash now as did in the
/// prior — or positions no longer line up and carrying would silently
/// misname every member. A changed group earns a fresh name, never a guess.
fn prior_name_for(
    idx: usize,
    hash: &str,
    prior_names: Option<&HashMap<String, Vec<CarriedName>>>,
    index: &HashIndex,
) -> Option<CarriedName> {
    let group = prior_names?.get(hash)?;
    if group.len() != *index.group_size.get(hash)? {
        return None;
    }
    // An app text asset's name is never a vendor name.
    group
        .get(index.occurrence[idx])
        .filter(|c| c.origin != NameSource::Asset)
        .cloned()
}

/// Apply the Phase 3 naming cascade to each classified factory.
///
/// The first two rungs: the cross-bundle carry-over via `prior_names` (an
/// exact structural-hash match), then the structural-hash fallback. The
/// rest of the ladder runs in the unpack adapter over the fallbacks, in
/// order: the declared-identity pair, the content pair, the declared
/// identity's own name (finding #95, [`identity`]), the LLM pass —
/// deterministic sources always win and this step stays sync/pure. A
/// carried name wins over a declared one, so a module's name and file are
/// stable once given.
///
/// The fallback and carry-over keys are deliberately the STRUCTURAL hash,
/// not the raw content hash: Bun re-rolls minified identifiers between
/// builds, so the content hash would change every release for unchanged
/// libraries and defeat stable filenames.
pub fn name_cjs_factories(
    classification: &mut BunModuleClassification,
    prior_names: Option<&HashMap<String, Vec<CarriedName>>>,
) -> FactoryNameCounts {
    let mut counts = FactoryNameCounts::default();
    let index = index_by_hash(&classification.factories);
    for (idx, factory) in classification.factories.iter_mut().enumerate() {
        // An app text asset is named from its own text before the cascade
        // (`text_assets`) — no declared or carried package name applies.
        if factory.name_source == Some(NameSource::Asset) {
            counts.asset += 1;
            continue;
        }
        if let Some(carried) = prior_name_for(idx, &factory.structural_hash, prior_names, &index) {
            factory.name = Some(carried.name);
            factory.name_source = Some(NameSource::CarryOver);
            factory.name_origin = Some(carried.origin);
            counts.carry_over += 1;
            continue;
        }
        // The names-mode placement answer (library-names plan step 4):
        // AFTER the carry — a prior release's path is reused before the
        // index is asked, so an unchanged library's entry is
        // byte-identical across hops — and BEFORE the model: a placed
        // module is never asked of it. The package is the answer's
        // folder; the name is the only thing the ladder ever decides,
        // the file path comes with the chooser.
        if let Some(package) = factory
            .index_path
            .as_deref()
            .and_then(vendor_names::index_folder_of)
        {
            factory.name = Some(package.to_string());
            factory.name_source = Some(NameSource::Index);
            factory.name_origin = Some(NameSource::Index);
            counts.index += 1;
            continue;
        }
        factory.name = Some(hash_fallback_name(&factory.structural_hash));
        factory.name_source = Some(NameSource::Fallback);
        factory.name_origin = Some(NameSource::Fallback);
        counts.fallback += 1;
    }
    counts
}

/// Classify the factories of a parsed bundle — either bundler's form
/// (bun's direct function, esbuild's single-key object; the helper
/// identification is [`identify_cjs_factory`], bun's marker first).
/// `wrapper_body` = the wrapper function's BODY block span when a wrapper
/// exists (the container of the top-level statements), else None (the
/// program's own body is the container).
pub fn classify_bun_modules<'a>(
    source: &'a str,
    program: &'a oxc_ast::ast::Program<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    wrapper_body: Option<Span>,
    tables: &SymbolTables,
) -> Option<BunModuleClassification> {
    let helper = identify_cjs_factory(source)?;

    let nodes = semantic.nodes();
    let container_stmts: Vec<Span> =
        crate::cut::container_statements(program, semantic, wrapper_body)
            .iter()
            .map(|s| s.span())
            .collect();

    let comments = semantic.comments();
    let mut line_starts: Vec<u32> = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i as u32 + 1);
        }
    }

    let mut factories: Vec<FactoryRecord> = Vec::new();
    // ONE pass over the nodes: a linear `nodes.iter().find` per container
    // statement is O(statements x nodes) — quadratic on a 12MB bundle (the
    // wrapper body holds the whole program's statements). The container
    // statement spans go into a set; every VariableDeclaration whose span
    // is in it is a container statement, in node order (= source order).
    use std::collections::HashSet;
    let container_set: HashSet<(u32, u32)> =
        container_stmts.iter().map(|s| (s.start, s.end)).collect();
    let mut stmt_index: HashMap<(u32, u32), usize> = HashMap::new();
    for (i, sp) in container_stmts.iter().enumerate() {
        stmt_index.insert((sp.start, sp.end), i);
    }
    let container_declarations = nodes
        .iter()
        .filter(|n| container_set.contains(&(n.span().start, n.span().end)))
        .filter_map(|n| match n.kind() {
            AstKind::VariableDeclaration(d) => Some((n.span(), d)),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (stmt_span, var_decl) in container_declarations {
        let i = stmt_index[&(stmt_span.start, stmt_span.end)];
        // The banner attached at statement level: the LAST bang-block in the
        // gap between the prior sibling's end (or the container's start for
        // the first statement) and this statement's start.
        let gap_start = if i == 0 {
            wrapper_body.map(|w| w.start).unwrap_or(0)
        } else {
            container_stmts[i - 1].end
        };
        let stmt_banner = collect_banner(source, comments, gap_start, stmt_span.start);
        for decl in var_decl.declarations.iter() {
            let declarator_span = decl.span();
            let Some(init) = decl.init.as_ref() else {
                continue;
            };
            let oxc_ast::ast::Expression::CallExpression(call) = init else {
                continue;
            };
            let oxc_ast::ast::Expression::Identifier(callee) = unparen(&call.callee) else {
                continue;
            };
            if callee.name != helper.name.as_str() {
                continue;
            }
            let Some(arg0) = call.arguments.first() else {
                continue;
            };
            let Some(arg_expr) = arg0.as_expression() else {
                continue;
            };
            // Either bundler's form: the function directly (bun, and any
            // minified build) or esbuild's single-key object — the
            // unwrapping owner, which also hands over the object key as
            // the module's original source path.
            let Some(factory_arg) = factory_arg_function(arg_expr) else {
                continue;
            };
            let factory_fn = factory_arg.function;
            let source_path = factory_arg.source_path;
            let object_method = factory_arg.object_method;
            let body_span = factory_fn.span();
            // The banner: statement-level first (even WITHOUT a package —
            // bannerText is recorded either way), the in-body fallback only
            // when statement level found nothing (the TS
            // `banner ?? findBannerInsideBody(arg0)`).
            let banner = stmt_banner
                .clone()
                .or_else(|| find_banner_inside_body(source, comments, body_span));
            // The structural hash of the body function (blurred) — the
            // cross-version join key, via the same canonical serializer the
            // function graph uses.
            let structural_hash = factory_structural_hash(factory_fn, tables, &MATCH_KEY)
                .expect("shape-checked above");
            // The content hash covers the DECLARATOR's slice (the TS
            // contentHash), not the body's.
            let content_hash: String = {
                let decl_source =
                    &source[declarator_span.start as usize..declarator_span.end as usize];
                let digest = Sha256::digest(decl_source.as_bytes());
                digest[..8].iter().map(|b| format!("{b:02x}")).collect()
            };
            let factory_var = match &decl.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.to_string(),
                _ => "<destructured>".to_string(),
            };
            factories.push(FactoryRecord {
                factory_var,
                span: declarator_span,
                body_span,
                line_range: (
                    line_of(declarator_span.start, &line_starts),
                    line_of(declarator_span.end, &line_starts),
                ),
                content_hash,
                structural_hash,
                banner_text: banner.as_ref().map(|b| b.text.clone()),
                banner_package: banner.as_ref().and_then(|b| b.pkg.clone()),
                banner_version: banner.as_ref().and_then(|b| b.version.clone()),
                name: None,
                name_source: None,
                name_origin: None,
                source_path,
                object_method,
                exported_text: text_assets::exported_text(factory_fn),
                carried: None,
                identity: None,
                hints: Vec::new(),
                index_path: None,
                index_leave: None,

                module_key: None,
            });
        }
    }

    Some(BunModuleClassification {
        helper_var: helper.name,
        factories,
        // The container's statement spans, in order — the scope planner
        // locates a captured binding's declaring statement in this list
        // (finding #60's bridge records).
        container: container_stmts.clone(),
        name_counts: None,
    })
}

/// The container-body node kinds: a function's body (oxc FunctionBody;
/// Babel BlockStatement) or a bare block.
fn line_of(offset: u32, line_starts: &[u32]) -> u32 {
    match line_starts.binary_search(&offset) {
        Ok(i) => i as u32 + 1,
        Err(i) => i as u32,
    }
}

/// The statement-level banner (the TS collectBanner): the LAST bang-block
/// comment in the gap between the prior statement's end and this
/// statement's start. Babel attaches a comment sitting between two
/// statements to the prior one's trailingComments or the next one's
/// leadingComments — the union of both buckets is exactly that gap, and
/// `[...trailing, ...leading].pop()` is its last bang-block in source
/// order.
fn collect_banner(
    source: &str,
    comments: &[oxc_ast::ast::Comment],
    gap_start: u32,
    stmt_start: u32,
) -> Option<BannerInfo> {
    let mut last: Option<(u32, String)> = None; // (span start, value)
    for comment in comments {
        if comment.is_line() {
            continue;
        }
        let span = comment.span();
        if span.start < gap_start || span.end > stmt_start {
            continue;
        }
        let Some(value) = bang_block_value(source, span) else {
            continue;
        };
        if last.as_ref().is_none_or(|(s, _)| span.start >= *s) {
            last = Some((span.start, value));
        }
    }
    let (_, raw) = last?;
    Some(parse_banner(&raw))
}

/// The bang-block banner inside a factory body (the TS
/// findBannerInsideBody): the FIRST bang-block whose package parses. The TS
/// walks the body DFS-with-a-stack (children in reverse push order); here
/// the scan is source order — the two agree unless one body carries MORE
/// THAN ONE bang-block where a later-in-source one parses first, which the
/// four-pair gate measures (deviation noted in the ledger claim).
fn find_banner_inside_body(
    source: &str,
    comments: &[oxc_ast::ast::Comment],
    body_span: Span,
) -> Option<BannerInfo> {
    for comment in comments {
        if comment.is_line() {
            continue;
        }
        let span = comment.span();
        if span.start < body_span.start || span.end > body_span.end {
            continue;
        }
        let Some(value) = bang_block_value(source, span) else {
            continue;
        };
        let info = parse_banner(&value);
        if info.pkg.is_some() {
            return Some(info);
        }
    }
    None
}

/// A block comment's value (between the delimiters) when it is a
/// bang-block: `isBangBlock` = CommentBlock whose value starts with `!`.
pub(crate) fn bang_block_value(source: &str, span: Span) -> Option<String> {
    let raw = &source[span.start as usize..span.end as usize];
    let value = raw.strip_prefix("/*")?.strip_suffix("*/")?;
    if !value.starts_with('!') {
        return None;
    }
    Some(value.to_string())
}

/// modules.json's rows in the `--dump-artifacts` catalog: the Bun
/// classification per site (helper var, wrapper, factory rows).
pub mod modules_dump {

    use crate::hash::serialize::SymbolTables;
    use crate::ingest::Ingest;
    use oxc_allocator::Allocator;
    use serde_json::{Value, json};

    /// One classification site's modules.json object (`recordBunModules`):
    /// helper var, wrapper, factory rows sorted by span — Null without a
    /// classification.
    pub fn site_json(
        data: Option<(
            &super::BunModuleClassification,
            Option<&super::wrapper::WrapperFunction>,
        )>,
        label: &str,
    ) -> Value {
        let Some((data, wrapper)) = data else {
            return Value::Null;
        };
        let span = |s: oxc_span::Span| json!({"text": label, "start": s.start, "end": s.end});
        let mut rows: Vec<Value> = data
            .factories
            .iter()
            .map(|f| {
                let mut row = json!({
                    "key": span(f.span),
                    "factoryVar": f.factory_var,
                    "lineRange": [f.line_range.0, f.line_range.1],
                    "contentHash": f.content_hash,
                    "structuralHash": f.structural_hash,
                });
                let obj = row.as_object_mut().expect("row object");
                // The TS writes the banner fields only when present
                // (JSON.stringify drops undefined) — mirror the omission.
                if let Some(t) = &f.banner_text {
                    obj.insert("bannerText".into(), json!(t));
                }
                if let Some(p) = &f.banner_package {
                    obj.insert("bannerPackage".into(), json!(p));
                }
                if let Some(v) = &f.banner_version {
                    obj.insert("bannerVersion".into(), json!(v));
                }
                // esbuild's unminified builds carry the module's original
                // source path (exp075) — recorded, never load-bearing.
                if let Some(p) = &f.source_path {
                    obj.insert("sourcePath".into(), json!(p));
                }
                row
            })
            .collect();
        rows.sort_by(|a, b| {
            let key = |v: &Value| {
                (
                    v["key"]["start"].as_u64().unwrap_or(0),
                    v["key"]["end"].as_u64().unwrap_or(0),
                )
            };
            key(a).cmp(&key(b))
        });
        let wrapper_json = wrapper.map(|w| {
            json!({
                "span": {"text": label, "start": w.span.start, "end": w.span.end},
                "bodySpan": {"text": label, "start": w.body_span.start, "end": w.body_span.end},
                "bindingCount": w.binding_count,
            })
        });
        json!({
            "helperVar": data.helper_var,
            "wrapper": wrapper_json,
            "factories": rows,
        })
    }

    /// One classification site over a text: the Bun CJS classification and
    /// the wrapper it ran against (None when no helper scan hit).
    pub fn classify_site(
        text: &str,
        layout: crate::toolchain::BundleLayout,
        grammar: crate::toolchain::ModuleWrapperGrammar,
    ) -> Result<
        Option<(
            super::BunModuleClassification,
            Option<super::wrapper::WrapperFunction>,
        )>,
        String,
    > {
        // No factory helper, no classification — and no parse (an ESM text
        // is not a bundle; the scan is the classifier's own first step —
        // the run's grammar's helper).
        if grammar.identify_factory_helper(text).is_none() {
            return Ok(None);
        }
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, text);
        if !ingest.errors.is_empty() {
            return Err(format!("oxc: {} diagnostic(s)", ingest.errors.len()));
        }
        let wrapper = layout.find_wrapper(ingest.program, ingest.semantic());
        let tables = SymbolTables::build(ingest.semantic());
        let classification = grammar.classify_factories(
            text,
            ingest.program,
            ingest.semantic(),
            wrapper.as_ref().map(|w| w.body_span),
            &tables,
        );
        Ok(classification.map(|c| (c, wrapper)))
    }
}

#[cfg(test)]
mod vendor_names_test;
