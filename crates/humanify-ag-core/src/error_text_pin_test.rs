//! The typed errors print the text the stages printed when their errors were
//! strings. An error's text reaches the run's warnings, the stats (a split
//! decline, a refused prior ledger) and the crash line, so a changed word
//! is a changed output. Each expected string below is the format string the
//! site used before it was typed, filled with the same sample values.

use std::path::PathBuf;

use oxc_span::Span;

use crate::error::{FileError, FileOp};
use crate::finish::driver::FinishError;
use crate::finish::relink::RelinkError;
use crate::finish::using::{UsingError, UsingTreeError};
use crate::format::FormatError;
use crate::format::ast::Field;
use crate::ingest::ParseError;
use crate::libdetect::function_carry::CarryError;
use crate::libindex::format::{ManifestError, SliceRecipeError};
use crate::libindex::lookup::{IndexError, LoadError};
use crate::libindex::module_answers::AnswersError;
use crate::libindex::recipe::{NoSlice, Recipe, RecipeError, RecipeFault};
use crate::libindex::table::{TableError, TableWriteError};
use crate::place::input::SplitInputError;
use crate::place::ledger::RederiveError;
use crate::twins::InventoryError;
use crate::unpack::chunk_link::{DecodeError, LinkError};

fn io(why: &str) -> std::io::Error {
    std::io::Error::other(why.to_string())
}

fn json_error() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

fn same(got: impl std::fmt::Display, want: &str) {
    assert_eq!(got.to_string(), want);
}

#[test]
fn file_errors_name_the_operation_and_the_path() {
    let p = PathBuf::from("out/a.js");
    for (op, verb) in [
        (FileOp::Read, "read"),
        (FileOp::CannotRead, "cannot read"),
        (FileOp::Write, "write"),
        (FileOp::Mkdir, "mkdir"),
        (FileOp::Readdir, "readdir"),
        (FileOp::Stat, "stat"),
    ] {
        same(
            FileError::at(op, &p)(io("gone")),
            &format!("{verb} out/a.js: gone"),
        );
    }
}

#[test]
fn formatter_errors_keep_their_words() {
    same(
        FormatError::Parse("Unexpected token".into()),
        "parse error: Unexpected token",
    );
    same(FormatError::Unsupported("decorators"), "decorators");
    same(
        FormatError::UnsupportedAt {
            what: "statement",
            start: 3,
            end: 9,
        },
        "unsupported statement at 3..9",
    );
    same(
        FormatError::AutoAccessor { at: 7 },
        "Support for the experimental syntax 'decoratorAutoAccessors' isn't currently enabled at 7",
    );
    same(
        FormatError::NoField {
            kind: "Identifier",
            field: Field::Body,
        },
        "Identifier has no field Body",
    );
    same(FormatError::NoListField(Field::Body), "no list field Body");
    same(
        FormatError::ExpectedArray {
            field: Field::Body,
            kind: "BlockStatement",
            got_node: false,
        },
        "Property Body of BlockStatement expected type of array but got null",
    );
    same(
        FormatError::ExpectedNode {
            field: Field::Body,
            kind: "IfStatement",
        },
        "Property Body of IfStatement expected a node but instead got null",
    );
    same(
        FormatError::UndefinedScopes {
            wanted: 2,
            matched: 1,
        },
        "undefined-binding scopes: 2 Babel scope(s), 1 matched in the tree",
    );
}

#[test]
fn carry_errors_keep_their_words() {
    same(
        CarryError::Synthesized {
            babel_type: "ArrowFunctionExpression".into(),
        },
        "library carry: a ArrowFunctionExpression in the beautified tree has no raw start — beautify synthesized a function, so raw offsets no longer identify it",
    );
    same(
        CarryError::CountMismatch {
            carried: 2,
            found: 1,
        },
        "library carry: 2 functions carried across beautify, 1 found in the re-parsed text",
    );
    same(
        CarryError::NoFunctionAt {
            span: Span::new(1, 4),
            session_id: Some("f1".into()),
        },
        "library classification: no function at fresh span 1..4 (f1)",
    );
    same(
        CarryError::SessionMismatch {
            span: Span::new(1, 4),
            graph: "f1".into(),
            classified: "f2".into(),
        },
        "library classification: fresh span 1..4 is f1 in the graph, f2 in the classification",
    );
}

#[test]
fn finish_errors_keep_their_words() {
    same(
        FinishError::Relink {
            file: "vendor/x.js".into(),
            source: RelinkError::Parse(ParseError("Unexpected token".into())),
        },
        "vendor/x.js: Unexpected token",
    );
    same(
        RelinkError::ShadowedRequire { name: "lib".into() },
        "the bridged read of lib sits in a scope that shadows `require`",
    );
    same(
        FinishError::Ledger {
            path: PathBuf::from("out/.humanify/split-ledger.json"),
            source: json_error(),
        },
        &format!("out/.humanify/split-ledger.json: {}", json_error()),
    );
    same(
        UsingTreeError::InFile {
            path: PathBuf::from("out/a.js"),
            source: UsingError::OverlappingSplice { at: 4 },
        },
        "out/a.js: using desugar: overlapping splice at 4",
    );
    same(
        UsingError::ShadowedGlobal { name: "Symbol" },
        "using desugar: a program-scope binding shadows helper global Symbol",
    );
    same(
        UsingError::UnknownHelper { name: "x".into() },
        "unknown Babel helper x",
    );
    same(
        UsingError::HelperParse { name: "x".into() },
        "helper x does not parse",
    );
}

#[test]
fn split_errors_keep_their_words() {
    same(
        InventoryError::Parse {
            anchor: "shipped",
            diagnostics: 2,
        },
        "oxc failed to parse the shipped text: 2 diagnostic(s)",
    );
    same(
        InventoryError::WrapperBlockAbsent(Span::new(5, 9)),
        "wrapper detected (body span 5..9) but that block is absent from the estree json",
    );
    same(
        InventoryError::NoStatementBound("start"),
        "statement start missing from the estree json",
    );
    same(
        SplitInputError::NotSplittable("not stable-splittable: no recognizable bundle wrapper"),
        "not stable-splittable: no recognizable bundle wrapper",
    );
    same(
        RederiveError::Current { version: 3 },
        "hashVersion 3 is current — nothing to re-derive",
    );
    same(
        RederiveError::CountMismatch {
            prior: 4,
            ledger: 5,
        },
        "the prior text has 4 wrapper statements, the ledger 5",
    );
    same(
        RederiveError::NotBijection {
            statement: 2,
            recorded_version: 1,
        },
        &format!(
            "statement 2: the recorded (hashVersion 1) and current (hashVersion {}) hash partitions differ (not a bijection)",
            crate::hash::forms::STATEMENT.version
        ),
    );
    same(
        crate::modules::wrapper::InputGateError::NoWrapper,
        "the run's input bundle has no recognizable bundle wrapper (the frozen ≥50-binding gate is measured on the input)",
    );
    same(
        crate::emit::stable_split::SplitError::LibraryNeedsFossil {
            regime: crate::place::placement_dump::Regime::Cluster,
        },
        "library placement requires the fossil split, but this bundle chose Cluster (check the fossil-split disable, and the markers' coverage)",
    );
    same(
        crate::emit::stable_split::SplitError::Misaligned {
            what: "vendor bridge",
            fresh: 3,
            shipped: 4,
        },
        "vendor bridge: the fresh text has 3 wrapper statements, the shipped code has 4 — they no longer align",
    );
    same(
        crate::emit::stable_split::SplitError::BridgeAmbiguous {
            name: "x".into(),
            hits: vec![(1, 0), (2, 0)],
        },
        "vendor bridge: the app-scope read of 'x' matches [(1, 0), (2, 0)] fresh statements — expected exactly one",
    );
    same(
        crate::place::assign::fossil::FossilSplitError::NoFossils,
        "fossil split: the bundle records no module fossils (no __esm init definitions) — fix detection or run with --disable fossil-split",
    );
    same(
        crate::emit::review::ReconstructError::Beyond {
            file: "x.js".into(),
            extra: 1,
        },
        "reconstruct: x.js has 1 statement(s) beyond the ledger",
    );
    same(
        crate::emit::cjs::EmitError::UnsupportedRedeclaration(vec!["a".into(), "b".into()]),
        "runnable emit: cross-file var redeclaration of \"a, b\" in an unsupported position",
    );
    same(
        crate::emit::cjs::EmitError::Cycle(vec!["a.js".into(), "b.js".into(), "a.js".into()]),
        "runnable emit: load-time reference cycle: a.js -> b.js -> a.js",
    );
    same(
        crate::emit::cjs::EmitError::NoIdentifier(Span::new(2, 3)),
        &format!("runnable emit: no identifier node at {:?}", Span::new(2, 3)),
    );
}

#[test]
fn naming_errors_keep_their_words() {
    same(
        crate::rename::transfer::TransferError::PlaceholdersMisaligned {
            at: "f3".into(),
            prior: 2,
            new: 3,
        },
        "placeholder maps misaligned for matched pair at f3: prior has 2 slots, new has 3 — equal hashes guarantee equal slot sets, so a mapping is stale or corrupt",
    );
    same(
        crate::naming::driver::era::EraError::CloseNoSpan {
            prior_id: "f9".into(),
        },
        "close prior f9 has no span",
    );
    same(
        crate::naming::passes::ShippedParseError("Unexpected token".into()),
        "shipped text does not parse: Unexpected token",
    );
    same(
        crate::naming::reconcile::step::StepError::Parse("Unexpected token".into()),
        "generated text does not parse: Unexpected token",
    );
    same(
        crate::naming::vendor_bodies::SplitBodiesError::Count {
            expected: 2,
            found: 1,
        },
        "2 bodies expected, 1 found",
    );
    same(
        crate::naming::passes::family_permute::PermuteError::Absent { start: 1, end: 5 },
        "declaring statement 1..5 is absent from the estree json",
    );
    same(
        crate::matching::close_dump::CloseDumpError::NotAFunction {
            side: "prior",
            id: "f1".into(),
        },
        "close-dump: prior f1 is not a function index entry",
    );
}

#[test]
fn library_index_errors_keep_their_words() {
    same(
        TableError::BadHash {
            what: "callee hash",
            hash: "zz".into(),
        },
        "callee hash \"zz\" is not 16 lowercase hex digits",
    );
    same(
        TableError::NotATable {
            what: "unit table",
            magic: b"HXUNIT03",
        },
        "unit table: not a HXUNIT03 table",
    );
    same(
        TableError::SizeMismatch {
            what: "edge table",
            len: 10,
            want: 12,
        },
        "edge table is 10 bytes, its header says 12 bytes: a damaged or partly written table",
    );
    same(
        TableError::NameRowWithoutUnit {
            hash: "h".into(),
            owner: "o".into(),
            file: "f".into(),
        },
        "name row \"h\"/o/\"f\": no unit row holds it",
    );
    same(
        TableWriteError::<String>::Sink("disk full".into()),
        "disk full",
    );
    same(
        LoadError::Unusable(IndexError::Table {
            name: "literals.bin".into(),
            source: TableError::LiteralShort { len: 3 },
        }),
        "literals.bin: literal table is 3 bytes, shorter than its header says",
    );
    same(
        IndexError::File {
            name: "manifest.json".into(),
            source: io("gone"),
        },
        "manifest.json: gone",
    );
    same(
        ManifestError::Json(json_error()),
        &format!("manifest.json: {}", json_error()),
    );
    same(
        SliceRecipeError::Mismatch {
            slice: "bun".into(),
            recipe: "esbuild:target=node,format=cjs".into(),
            recipe_slice: "esbuild".into(),
        },
        "slice bun records recipe esbuild:target=node,format=cjs (whose slice is esbuild)",
    );
    same(
        RecipeError {
            text: "bun:target=bun".into(),
            why: RecipeFault::NoFormat,
        },
        "--recipe \"bun:target=bun\": no format (form: <bun|esbuild|webpack|rollup>:target=<bun|node>,format=<cjs|esm>[,minify][,production])",
    );
    assert_eq!(
        Recipe::parse("webpack:target=node,format=cjs,production")
            .unwrap_err()
            .to_string(),
        "--recipe \"webpack:target=node,format=cjs,production\": webpack's recipe script decides NODE_ENV (production is a Bun / esbuild setting) (form: <bun|esbuild|webpack|rollup>:target=<bun|node>,format=<cjs|esm>[,minify][,production])",
    );
    same(
        NoSlice {
            detected: "bun".into(),
            has: vec!["a".into(), "b".into()],
        },
        "no slice for recipe bun (has: a, b)",
    );
    same(
        AnswersError::Line {
            file: "modules.s.jsonl".into(),
            line: 3,
            source: json_error(),
        },
        &format!("modules.s.jsonl line 3: {}", json_error()),
    );
}

#[test]
fn unpack_errors_keep_their_words() {
    same(
        LinkError::Missing {
            spec: "/$bunfs/root/a.js".into(),
            path: PathBuf::from("in/a.js"),
        },
        "/$bunfs/root/a.js is imported but in/a.js does not exist",
    );
    same(
        LinkError::Refused {
            path: PathBuf::from("in/a.js"),
            what: "`export *` is not linked",
        },
        "in/a.js: `export *` is not linked",
    );
    same(
        LinkError::EmbeddedAttributes {
            path: PathBuf::from("in/a.js"),
            rest: "db.sqlite".into(),
        },
        "in/a.js: /$bunfs/root/db.sqlite is required with import attributes (Bun's sqlite_embedded loader); not linked",
    );
    same(
        LinkError::EmbeddedText {
            rest: "a.md".into(),
            why: DecodeError::OddLength,
        },
        "/$bunfs/root/a.md is neither UTF-8 nor UTF-16LE (odd length)",
    );
    same(
        LinkError::ReExportExternal {
            path: PathBuf::from("in/a.js"),
            spec: "fs".into(),
        },
        "in/a.js: re-exports from \"fs\"; not linked",
    );
    same(
        crate::unpack::UnpackError::NoShim,
        "the webcrack adapter needs its subprocess shim",
    );
    same(
        crate::unpack::webcrack::WebcrackError::Outside {
            path: PathBuf::from("/etc/passwd"),
            out_dir: PathBuf::from("/run/out"),
        },
        "webcrack shim reported /etc/passwd, outside its output directory /run/out",
    );
    same(
        crate::artifact_dump::DumpError::Prompts {
            op: "assemble",
            source: io("gone"),
        },
        "assemble prompts.jsonl: gone",
    );
    same(
        crate::libdetect::DetectError::NoFactories,
        "bun manifest without a factories array",
    );
}

#[test]
fn prior_match_errors_keep_their_words() {
    use crate::prior::{PriorMatchError, SideParseError};
    same(
        PriorMatchError::NotSameProgram {
            present: 1,
            prior_functions: 100,
        },
        "prior version does not appear to be the same program: only 1 of 100 prior functions \
         have a matching structural hash in the new version. Check the --prior-version file; \
         drop the flag to run without transfer.",
    );
    same(
        PriorMatchError::Parse(SideParseError {
            name: "prior.js".into(),
            count: 2,
            first: "Unexpected token".into(),
        }),
        "oxc failed to parse prior.js: 2 diagnostic(s) — Unexpected token",
    );
    same(
        crate::modules::vendor_names::PriorManifest::parse("{").unwrap_err(),
        &serde_json::from_str::<serde_json::Value>("{")
            .unwrap_err()
            .to_string(),
    );
}
