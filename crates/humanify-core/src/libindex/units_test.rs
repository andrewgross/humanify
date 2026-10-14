//! The unit fingerprint: what a statement unit and its functions hash to.

use oxc_allocator::Allocator;
use oxc_estree::{CompactSerializer, ESTree};

use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::libindex::units::{StatementPrints, statement_prints};

/// The prints of each top-level statement of `code`.
fn prints(code: &str) -> Vec<StatementPrints> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(ingest.semantic());
    ingest
        .program
        .body
        .iter()
        .map(|s| {
            let mut ser = CompactSerializer::new(false, false);
            s.serialize(&mut ser);
            statement_prints(
                &crate::ingest::parse_estree_json(&ser.into_string()),
                &tables,
            )
        })
        .collect()
}

#[test]
fn renamed_locals_hash_alike_and_globals_do_not() {
    let a = prints("function f(e, t) { return e.split(t).map(Number); }\n");
    let b = prints("function q(x, y) { return x.split(y).map(Number); }\n");
    let c = prints("function q(x, y) { return x.split(y).map(String); }\n");
    assert_eq!(a[0].whole.hash, b[0].whole.hash);
    assert_ne!(b[0].whole.hash, c[0].whole.hash, "a global is content");
    assert_eq!(a[0].whole.hash.len(), 16);
}

#[test]
fn a_statement_lists_its_functions_with_their_spans() {
    let code = "var api = { get: function (u) { return fetch(u); }, put: (u, b) => send(u, b) };\n";
    let p = prints(code);
    let texts: Vec<&str> = p[0]
        .functions
        .iter()
        .map(|f| &code[f.span.start as usize..f.span.end as usize])
        .collect();
    assert_eq!(
        texts,
        vec!["function (u) { return fetch(u); }", "(u, b) => send(u, b)"]
    );
    assert_eq!(p[0].whole.span.start, 0);
}

#[test]
fn nodes_and_content_tokens_are_counted() {
    // Content: "a-b" (string), /x+/ (regex), split (property), Array
    // (global); `s` and `t` are locals, 1 is a number.
    let p = prints("var t = (s) => s.split(\"a-b\").concat(Array.from(/x+/.exec(s) || [], 1));\n");
    let w = &p[0].whole;
    assert_eq!(
        w.content, 7,
        "a-b, x+, split, concat, Array, from, exec are content; got {}",
        w.content
    );
    assert!(w.nodes > 15);
}

#[test]
fn a_bundles_units_include_module_bodies_and_skip_containers() {
    use crate::cut::units::UnitKind;
    use crate::libindex::units::bundle_unit_prints;
    use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let code = "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\nvar b=(e,t)=>()=>(e&&(t=e(e=0)),t);\nvar mod_a=x((exports,module)=>{\n  function helper(){return \"a\"}\n  module.exports=helper;\n});\nvar N;\nvar I=b(()=>{\n  N=mod_a();\n});\nvar main=()=>(I(),N);\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    let units = bundle_unit_prints(
        code,
        &ingest,
        BundleLayout::SingleWrapperFunction,
        ModuleWrapperGrammar::BunAndEsbuild,
        InteropHelpers::Bun,
    );
    let rows: Vec<(String, &str)> = units
        .iter()
        .map(|(u, _)| {
            let kind = match u.kind {
                UnitKind::Statement => "stmt",
                UnitKind::LazyBody { .. } => "lazy-body",
                UnitKind::FactoryBody { .. } => "factory-body",
                _ => "container",
            };
            (
                code[u.span.start as usize..u.span.end as usize]
                    .chars()
                    .take(12)
                    .collect(),
                kind,
            )
        })
        .collect();
    let rows: Vec<(&str, &str)> = rows.iter().map(|(t, k)| (t.as_str(), *k)).collect();
    assert_eq!(
        rows,
        vec![
            ("var x=(I,A)=", "stmt"),
            ("var b=(e,t)=", "stmt"),
            ("function hel", "factory-body"),
            ("module.expor", "factory-body"),
            ("var N;", "stmt"),
            ("N=mod_a();", "lazy-body"),
            ("var main=()=", "stmt"),
        ]
    );
}

#[test]
fn private_names_are_minified_names_not_content() {
    let a = prints("class Q { #e; #t; get(k) { return this.#e.get(k) ?? this.#t; } }\n");
    let b = prints("class W { #a; #b; get(k) { return this.#a.get(k) ?? this.#b; } }\n");
    assert_eq!(a[0].whole.hash, b[0].whole.hash);
    assert_eq!(a[0].functions[0].hash, b[0].functions[0].hash);
    assert_eq!(
        a[0].whole.content, 1,
        "only `get`; the private names are the minifier's"
    );
}

/// The unit model's pins: (`LIBRARY_UNITS.version`, digest of every unit print of
/// [`PIN_FIXTURES`]). A change to WHAT a unit is or how it hashes — the
/// unit cut, the container rules, the serializer — moves the digest, and
/// an index built before it holds units a run can no longer produce: the
/// form's version (`hash::forms::LIBRARY_UNITS`) MUST be bumped with it (main's 68b090e3 split esbuild 0.28's
/// guarded `__esm` wrappers into lazy-body statements under a constant
/// `units/2`, and every stale index shard was reused — exp096
/// `lazy_modules.py`). Add the new (version, digest) row; never edit one.
const UNIT_MODEL_PINS: &[(u64, &str)] = &[
    // index v2's builder (a537a92b, before 68b090e3)
    (
        2,
        "5b0dc0e84270ec06a75422d08e00b8a4b909db75653b26c78451322529c590fe",
    ),
    // 68b090e3: esbuild 0.28's guarded `__esm` is a lazy module (only the
    // two esbuild 0.28 minified fixtures move; Bun and esbuild <= 0.27
    // print byte-identically)
    (
        3,
        "3e6f96ad7285928182214b208ff61b84aa50c679b7951585c4d14772524978f7",
    ),
];

/// One esbuild 0.28 minified bundle (guarded `__esm`), the esbuild ESM
/// and minified-ESM forms, and three Bun forms (the slice each is
/// fingerprinted under: the builder's own path).
const PIN_FIXTURES: &[(&str, &str, crate::libindex::recipe::RecipeBundler)] = {
    use crate::libindex::recipe::RecipeBundler::{Bun, Esbuild};
    &[
        (
            "esbuild-minified-0.28",
            include_str!(
                "../../../../test/e2e/fixtures/esbuild-minified-0.28/build/v1.0.0/build/index.js"
            ),
            Esbuild,
        ),
        (
            "esbuild-minified-esm",
            include_str!(
                "../../../../test/e2e/fixtures/esbuild-minified-esm/build/v1.0.0/build/index.js"
            ),
            Esbuild,
        ),
        (
            "esbuild-esm",
            include_str!("../../../../test/e2e/fixtures/esbuild-esm/build/v1.0.0/build/index.js"),
            Esbuild,
        ),
        (
            "bun-lazy",
            include_str!("../../../../test/e2e/fixtures/bun-lazy/build/v1.0.0/build/index.js"),
            Bun,
        ),
        (
            "bun-esm-minified",
            include_str!(
                "../../../../test/e2e/fixtures/bun-esm-minified/build/v1.0.0/build/index.js"
            ),
            Bun,
        ),
        (
            "bun-bundle",
            include_str!("../../../../test/e2e/fixtures/bun-bundle/build/v1.0.0/build/index.js"),
            Bun,
        ),
    ]
};

fn unit_model_digest() -> String {
    use crate::libindex::units::bundle_unit_prints;
    use sha2::{Digest, Sha256};
    let mut lines = String::new();
    for (name, code, slice) in PIN_FIXTURES {
        let toolchain = crate::toolchain::resolve_toolchain(
            &crate::detect::detect_bundle(code),
            Some(slice.bundler_type()),
            None,
        );
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, code);
        assert!(ingest.errors.is_empty(), "{name} must parse");
        for (unit, prints) in bundle_unit_prints(
            code,
            &ingest,
            toolchain.layout.piece,
            toolchain.module_wrappers.piece,
            toolchain.interop.piece,
        ) {
            for p in std::iter::once(&prints.whole).chain(&prints.functions) {
                lines.push_str(&format!(
                    "{name} {:?} {}..{} {} {} {}\n",
                    unit.kind, p.span.start, p.span.end, p.hash, p.nodes, p.content
                ));
            }
        }
    }
    Sha256::digest(lines.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn the_unit_model_cannot_change_under_a_constant_version() {
    let version = crate::hash::forms::LIBRARY_UNITS.version;
    let digest = unit_model_digest();
    let pinned: Vec<&str> = UNIT_MODEL_PINS
        .iter()
        .filter(|(v, _)| *v == version)
        .map(|(_, d)| *d)
        .collect();
    assert_eq!(
        pinned,
        vec![digest.as_str()],
        "the unit model's digest is {digest}; units/{version} pins {pinned:?}. \
         A changed model needs a NEW `LIBRARY_UNITS` version and a new UNIT_MODEL_PINS row"
    );
}

/// exp105: the unit rows of a real bundle under the toolchain its bundler
/// resolves (the builder's path): (first 12 chars, kind).
fn bundler_unit_rows(
    code: &str,
    bundler: humanify_model::detection::BundlerType,
) -> Vec<(String, &'static str)> {
    use crate::cut::units::UnitKind;
    use crate::libindex::units::bundle_unit_prints;
    let toolchain = crate::toolchain::resolve_toolchain(
        &crate::detect::detect_bundle(code),
        Some(bundler),
        None,
    );
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    assert!(ingest.errors.is_empty());
    bundle_unit_prints(
        code,
        &ingest,
        toolchain.layout.piece,
        toolchain.module_wrappers.piece,
        toolchain.interop.piece,
    )
    .iter()
    .map(|(u, _)| {
        let kind = match u.kind {
            UnitKind::Statement => "stmt",
            UnitKind::LazyBody { .. } => "lazy-body",
            UnitKind::FactoryBody { .. } => "factory-body",
            _ => "container",
        };
        (
            code[u.span.start as usize..u.span.end as usize]
                .chars()
                .take(12)
                .collect(),
            kind,
        )
    })
    .collect()
}

const WEBPACK5_FIXTURE: &str =
    include_str!("../../../../test/fixtures/bundlers/webpack5-terser.js.fixture");
const ROLLUP4_FIXTURE: &str =
    include_str!("../../../../test/fixtures/bundlers/rollup4-terser.js.fixture");

#[test]
fn a_webpack_bundle_s_modules_are_lazy_modules_whose_statements_are_units() {
    let rows = bundler_unit_rows(
        WEBPACK5_FIXTURE,
        humanify_model::detection::BundlerType::Webpack,
    );
    let has = |text: &str, kind: &str| rows.iter().any(|(t, k)| t == text && *k == kind);
    // the module table's entry (ms): its body's statements
    assert!(has("var s=1e3,r=", "lazy-body"), "{rows:?}");
    assert!(has("function i(e", "lazy-body"), "{rows:?}");
    assert!(has("e.exports=fu", "lazy-body"), "{rows:?}");
    // the CommonJS wrapper (escape-html) inside the concatenated entry
    assert!(has("var r=/[\"'&<", "lazy-body"), "{rows:?}");
    // the concatenated entry IIFE (p-limit, the app) is a scope: its
    // statements are cut like hoisted ones (as Bun's and rollup's are)
    assert!(has("class a{valu", "stmt"), "{rows:?}");
    assert!(has("async functi", "stmt"), "{rows:?}");
    // the split declarator beside the wrapper keeps its own unit
    assert!(has("s=r(585)", "stmt"), "{rows:?}");
    // the bootstrap's own statements (the runtime)
    assert!(has("function r(t", "stmt"), "{rows:?}");
    // nothing is one giant statement any more
    assert!(
        !rows.iter().any(|(t, _)| t.starts_with("(()=>{var e=")),
        "{rows:?}"
    );
}

#[test]
fn a_rollup_bundle_s_guarded_require_functions_are_lazy_modules() {
    let rows = bundler_unit_rows(
        ROLLUP4_FIXTURE,
        humanify_model::detection::BundlerType::Rollup,
    );
    let has = |text: &str, kind: &str| rows.iter().any(|(t, k)| t == text && *k == kind);
    // ms, an IIFE in a declarator; escape-html, an IIFE as a call's argument
    assert!(has("var e=1e3,s=", "lazy-body"), "{rows:?}");
    assert!(has("var e=/[\"'&<", "lazy-body"), "{rows:?}");
    // the hoisted ES modules: plain statements
    assert!(has("class u{valu", "stmt"), "{rows:?}");
    assert!(has("function e(e", "stmt"), "{rows:?}");
}

/// exp105's pins for the webpack and rollup unit models (their own slices:
/// a change to what a webpack or rollup unit is moves this digest and
/// needs a new `LIBRARY_UNITS` version, exactly like the rows above).
/// Kept apart so adding the two grammars moved no Bun or esbuild pin.
const BUNDLER_UNIT_MODEL_PINS: &[(u64, &str)] = &[(
    3,
    "bb93c851e60169807ad859bc95d2c1b7fa1f224b43a1dc7a8568c9f467c204cb",
)];

#[test]
fn the_webpack_and_rollup_unit_models_cannot_change_under_a_constant_version() {
    use crate::libindex::units::bundle_unit_prints;
    use humanify_model::detection::BundlerType;
    use sha2::{Digest, Sha256};
    let mut lines = String::new();
    for (name, code, bundler) in [
        ("webpack5-terser", WEBPACK5_FIXTURE, BundlerType::Webpack),
        ("rollup4-terser", ROLLUP4_FIXTURE, BundlerType::Rollup),
    ] {
        let toolchain = crate::toolchain::resolve_toolchain(
            &crate::detect::detect_bundle(code),
            Some(bundler),
            None,
        );
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, code);
        for (unit, prints) in bundle_unit_prints(
            code,
            &ingest,
            toolchain.layout.piece,
            toolchain.module_wrappers.piece,
            toolchain.interop.piece,
        ) {
            for p in std::iter::once(&prints.whole).chain(&prints.functions) {
                lines.push_str(&format!(
                    "{name} {:?} {}..{} {} {} {}\n",
                    unit.kind, p.span.start, p.span.end, p.hash, p.nodes, p.content
                ));
            }
        }
    }
    let digest: String = Sha256::digest(lines.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let version = crate::hash::forms::LIBRARY_UNITS.version;
    let pinned: Vec<&str> = BUNDLER_UNIT_MODEL_PINS
        .iter()
        .filter(|(v, _)| *v == version)
        .map(|(_, d)| *d)
        .collect();
    assert_eq!(
        pinned,
        vec![digest.as_str()],
        "the webpack/rollup unit model's digest is {digest}; units/{version} pins {pinned:?}"
    );
}
