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

/// The variants the tests above leave out: every variant of every error
/// type is pinned, so deriving a `Display` cannot move a word unseen.
mod every_variant {
    use std::path::PathBuf;

    use oxc_span::Span;

    use super::{io, json_error, same};
    use crate::error::{FileError, FileOp};

    fn file_error() -> FileError {
        FileError::at(FileOp::Read, std::path::Path::new("in/a.js"))(io("gone"))
    }

    #[test]
    fn dump_errors() {
        use crate::artifact_dump::DumpError;
        same(DumpError::File(file_error()), "read in/a.js: gone");
        same(
            DumpError::Serialize {
                name: "modules.json".into(),
                source: json_error(),
            },
            &format!("serialize modules.json: {}", json_error()),
        );
        same(
            DumpError::Site(crate::modules::UnparsedSite { diagnostics: 3 }),
            "oxc: 3 diagnostic(s)",
        );
        same(DumpError::Part(io("full")), "full");
        same(
            DumpError::NamingPartLost,
            "the naming part writer failed mid-run",
        );
        same(
            DumpError::ReadPart {
                name: "naming.part",
                source: io("gone"),
            },
            "read naming.part: gone",
        );
        same(
            DumpError::AskRow(json_error()),
            &format!("ask row: {}", json_error()),
        );
    }

    #[test]
    fn emit_errors() {
        use crate::emit::cjs::EmitError;
        use crate::emit::review::ReconstructError;
        let cases = [
            (
                EmitError::NoAlias {
                    module: "m".into(),
                    reader: "r".into(),
                },
                "runnable emit: no alias for m in r (internal)",
            ),
            (
                EmitError::FunctionRedeclaration { name: "f".into() },
                "runnable emit: cross-file function redeclaration of \"f\" cannot preserve hoisting",
            ),
            (
                EmitError::PatternRedeclaration { name: "p".into() },
                "runnable emit: cross-file redeclaration of \"p\" through a destructuring declarator is not supported",
            ),
            (
                EmitError::ParamRedeclared { param: "e".into() },
                "runnable emit: wrapper parameter \"e\" is redeclared",
            ),
            (
                EmitError::LoadOrderHazard {
                    reader: "a.js".into(),
                    read: "b.js".into(),
                },
                "runnable emit: load-order hazard: a.js reads b.js at load before its body runs",
            ),
            (EmitError::Internal("internal step"), "internal step"),
            (
                EmitError::NoCrossRecord { name: "x".into() },
                "runnable emit: no cross record for x",
            ),
            (
                EmitError::OrderLength {
                    order: 2,
                    statements: 3,
                },
                "runnable emit: ledger.order has 2 entries but the wrapper has 3 statements",
            ),
            (
                EmitError::OrderMissing {
                    file: "a.js".into(),
                },
                "runnable emit: ledger.order references \"a.js\" missing from ledger.files",
            ),
            (
                EmitError::NoNamespaceVariable {
                    module: "m".into(),
                    importer: "i".into(),
                },
                "runnable emit: no free namespace variable for m in i",
            ),
            (
                EmitError::BridgeNoFile {
                    file: "v.js".into(),
                    name: "n".into(),
                },
                "vendor bridge: no emitted file v.js to export n from",
            ),
        ];
        for (e, want) in cases {
            same(e, want);
        }
        same(
            ReconstructError::Short {
                file: "x.js".into(),
                at: 4,
            },
            "reconstruct: x.js is short of statement 4",
        );
        same(
            ReconstructError::Mismatch,
            "stable split: emitted tree does not reconstruct the source statements (tree/ledger invariant violated)",
        );
    }

    #[test]
    fn split_errors() {
        use crate::emit::review::ReconstructError;
        use crate::emit::stable_split::SplitError;
        use crate::modules::wrapper::InputGateError;
        use crate::place::input::SplitInputError;
        use crate::place::placement_dump::PlaceError;
        same(
            SplitError::InputGate(InputGateError::Parse { diagnostics: 2 }),
            "oxc failed to parse the input bundle: 2 diagnostic(s)",
        );
        same(
            SplitError::Input(SplitInputError::NotSplittable("no wrapper")),
            "no wrapper",
        );
        same(
            SplitError::Place(PlaceError::NoPriorLedger),
            "the tiers regime needs --prior-ledger",
        );
        same(
            SplitError::Reconstruct(ReconstructError::Short {
                file: "x.js".into(),
                at: 1,
            }),
            "reconstruct: x.js is short of statement 1",
        );
        same(SplitError::Invariant("holds"), "holds");
        same(
            SplitError::BridgeNameCount {
                statement: 1,
                fresh: 2,
                shipped: 3,
            },
            "vendor bridge: statement 1 declares 2 names fresh but 3 shipped",
        );
        same(
            SplitError::BridgeLostSlot {
                statement: 1,
                name: "x".into(),
            },
            "vendor bridge: statement 1 lost the slot of 'x' across the rename",
        );
        same(
            SplitError::BridgeUnplaced {
                statement: 1,
                name: "x".into(),
            },
            "vendor bridge: statement 1 (the home of 'x') has no placement",
        );
        same(
            SplitInputError::Inventory(crate::twins::InventoryError::NullJson),
            "the estree json did not parse (read as null)",
        );
    }

    #[test]
    fn place_errors() {
        use crate::place::assign::fossil::FossilSplitError;
        use crate::place::ledger::RederiveError;
        use crate::place::placement_dump::PlaceError;
        use crate::place::tiers::Unclaimed;
        use crate::twins::fossil::FossilMapError;
        let map = FossilMapError {
            hashes: 2,
            statements: 3,
        };
        same(map, "fossil map: 2 hashes for 3 statements");
        same(
            FossilSplitError::Map(map),
            "fossil map: 2 hashes for 3 statements",
        );
        same(
            FossilSplitError::LibraryOutside { module: 4 },
            "library placement: module 4 is outside this bundle's fossils",
        );
        same(
            FossilSplitError::Unassigned { module: 4 },
            "fossil split: module 4 was never assigned a file",
        );
        same(
            PlaceError::Fossil(FossilSplitError::Unassigned { module: 1 }),
            "fossil split: module 1 was never assigned a file",
        );
        same(
            PlaceError::Unclaimed(Unclaimed),
            "stable split: no placement tier claimed the statement",
        );
        same(
            RederiveError::NoHashes,
            "the ledger records no per-statement hashes",
        );
        same(
            RederiveError::Input(crate::place::input::SplitInputError::NotSplittable("no")),
            "no",
        );
        same(
            RederiveError::UnknownHash { hash: "h1".into() },
            "hash h1 is not among the ledger's statements",
        );
    }

    #[test]
    fn finish_and_format_errors() {
        use crate::finish::driver::FinishError;
        use crate::finish::using::{UsingError, UsingTreeError};
        use crate::format::FormatError;
        use crate::format::ast::Field;
        use crate::ingest::ParseError;
        use crate::libdetect::function_carry::CarryError;
        same(FinishError::File(file_error()), "read in/a.js: gone");
        same(
            FinishError::Using(UsingTreeError::File(file_error())),
            "read in/a.js: gone",
        );
        same(UsingError::Parse(ParseError("bad".into())), "bad");
        same(
            UsingError::Format(FormatError::Parse("bad".into())),
            "parse error: bad",
        );
        same(UsingError::Refused("refused"), "refused");
        same(UsingTreeError::File(file_error()), "read in/a.js: gone");
        same(FormatError::Traverse("traverse"), "traverse");
        same(
            FormatError::ExpectedArray {
                field: Field::Body,
                kind: "BlockStatement",
                got_node: true,
            },
            "Property Body of BlockStatement expected type of array but got object",
        );
        same(
            FormatError::Carry(CarryError::Missing),
            "library regions reached the naming stage without a function classification — raw offsets cannot classify beautified functions (#32)",
        );
        same(
            crate::format::UnknownPlant::Visitor("v".into()),
            "unknown visitor v",
        );
        same(
            crate::format::UnknownPlant::Plant("p".into()),
            "unknown plant p",
        );
        same(crate::format::template_raw::InvalidRaw, "Invalid raw");
        same(
            crate::ingest::EstreeJsonError(json_error()),
            &format!("estree json: {}", json_error()),
        );
        same(
            CarryError::TypeMismatch {
                ordinal: 3,
                before: "FunctionDeclaration".into(),
                after: "ArrowFunctionExpression".into(),
            },
            "library carry: function #3 is a FunctionDeclaration before re-parse and a ArrowFunctionExpression after",
        );
        same(
            CarryError::NoFunctionAt {
                span: Span::new(1, 4),
                session_id: None,
            },
            "library classification: no function at fresh span 1..4",
        );
        same(
            crate::libdetect::DetectError::File(file_error()),
            "read in/a.js: gone",
        );
    }

    #[test]
    fn library_index_errors() {
        use crate::libindex::format::{ManifestError, SliceRecipeError};
        use crate::libindex::lookup::{IndexError, LoadError};
        use crate::libindex::module_answers::AnswersError;
        use crate::libindex::recipe::{RecipeError, RecipeFault};
        use crate::libindex::table::{TableError, TableWriteError};
        same(
            ManifestError::Stamps(vec!["format 3 (this binary: 4)".into(), "units/4".into()]),
            "built with format 3 (this binary: 4), units/4; rebuild the index with this binary's `humanify-ag index build` (the shard store rebuilds only what changed)",
        );
        let recipe = || RecipeError {
            text: "x".into(),
            why: RecipeFault::NoColon,
        };
        let form = "(form: <bun|esbuild|webpack|rollup>:target=<bun|node>,format=<cjs|esm>[,minify][,production])";
        same(
            SliceRecipeError::Parse(recipe()),
            &format!("--recipe \"x\": no `:` {form}"),
        );
        for (why, text) in [
            (RecipeFault::UnknownBundler, "unknown bundler".to_string()),
            (
                RecipeFault::UnknownSetting("k=v".into()),
                "unknown setting \"k=v\"".into(),
            ),
            (RecipeFault::NoTarget, "no target".into()),
            (
                RecipeFault::NoBunPlatform("webpack"),
                "webpack has no bun platform".into(),
            ),
        ] {
            same(
                RecipeError {
                    text: "x".into(),
                    why,
                },
                &format!("--recipe \"x\": {text} {form}"),
            );
        }
        same(
            LoadError::NoSlice {
                id: "abc".into(),
                why: "no slice here".into(),
            },
            "no slice here",
        );
        same(
            IndexError::Json {
                name: "slices.json".into(),
                source: json_error(),
            },
            &format!("slices.json: {}", json_error()),
        );
        same(
            IndexError::Manifest(ManifestError::Json(json_error())),
            &format!("manifest.json: {}", json_error()),
        );
        same(
            IndexError::Recipes(SliceRecipeError::Parse(recipe())),
            &format!("--recipe \"x\": no `:` {form}"),
        );
        same(
            AnswersError::Io {
                file: "modules.s.jsonl".into(),
                source: io("gone"),
            },
            "modules.s.jsonl: gone",
        );
        let tables = [
            (
                TableError::ColumnOverrun {
                    n: 1,
                    at: 2,
                    len: 3,
                },
                "a key column of 1 keys at byte 2 runs past the table's 3 bytes",
            ),
            (
                TableError::EdgeWithoutUnit {
                    hash: "h".into(),
                    owner: "o".into(),
                    file: "f".into(),
                },
                "edge row h o f has no unit row",
            ),
            (TableError::TwoEdgeRows, "two edge rows for one unit row"),
            (
                TableError::NamesWithoutSlice { slice: "s".into() },
                "names rows for slice s: the slice has no unit rows",
            ),
            (
                TableError::NamesOutOfOrder { row: 2, prev: 3 },
                "names row 2 follows 3 — the rows must be in row-id order, one per row",
            ),
            (
                TableError::EntriesOverrun {
                    from: 1,
                    to: 5,
                    n: 4,
                },
                "names entries 1..5 run past the table's 4",
            ),
            (
                TableError::NameIdPast {
                    name: 9,
                    strings: 8,
                },
                "names entry's name id 9 past the 8 strings",
            ),
            (
                TableError::NoKind { kind: 7 },
                "names entry's kind ordinal 7 is no kind",
            ),
            (TableError::PackageCount, "literal table: package count"),
        ];
        for (e, want) in tables {
            same(e, want);
        }
        same(TableError::Json(json_error()), &json_error().to_string());
        same(
            TableWriteError::<String>::Table(TableError::TwoEdgeRows),
            "two edge rows for one unit row",
        );
    }

    #[test]
    fn matching_and_naming_errors() {
        use crate::matching::close_dump::CloseDumpError;
        use crate::naming::driver::NamingError;
        use crate::naming::driver::era::EraError;
        use crate::naming::passes::ShippedParseError;
        use crate::naming::passes::family_permute::PermuteError;
        use crate::naming::reconcile::hunks::DiffError;
        use crate::naming::reconcile::step::StepError;
        use crate::naming::vendor_bodies::SplitBodiesError;
        use crate::prior::{PriorMatchError, SideParseError};
        use crate::rename::transfer::TransferError;
        same(
            CloseDumpError::Drift {
                won: 1,
                assigned: 2,
            },
            "close-dump drift: the outcome replay's won set (1) does not equal the assignment (2)",
        );
        same(
            CloseDumpError::RowJsonUnavailable {
                prior: "f1".into(),
                fresh: "f2".into(),
            },
            "close-dump: row JSON unavailable for f1 / f2",
        );
        same(
            crate::matching::rules::Refusal {
                profile: crate::matching::rules::Profile::Library,
                rule: "callers",
                why: "reads into edges".into(),
            },
            "Library profile refuses rule callers: reads into edges",
        );
        let side = SideParseError {
            name: "input.js".into(),
            count: 1,
            first: "bad".into(),
        };
        same(
            NamingError::Prior(PriorMatchError::Parse(side.clone())),
            "oxc failed to parse input.js: 1 diagnostic(s) — bad",
        );
        same(
            NamingError::Era(EraError::CloseNoSpan {
                prior_id: "f1".into(),
            }),
            "close prior f1 has no span",
        );
        same(
            NamingError::Census(ShippedParseError("bad".into())),
            "shipped text does not parse: bad",
        );
        same(
            EraError::Parse(side),
            "oxc failed to parse input.js: 1 diagnostic(s) — bad",
        );
        same(
            EraError::Carry(crate::libdetect::function_carry::CarryError::Missing),
            "library regions reached the naming stage without a function classification — raw offsets cannot classify beautified functions (#32)",
        );
        same(
            EraError::Transfer(TransferError::NoBabelBinding { name: "x".into() }),
            "binding rename x: no babel binding",
        );
        same(
            TransferError::Twins(crate::twins::fossil::FossilMapError {
                hashes: 1,
                statements: 2,
            }),
            "fossil map: 1 hashes for 2 statements",
        );
        same(
            TransferError::RowJsonUnavailable {
                prior: "f1".into(),
                fresh: "f2".into(),
            },
            "matched pair f1 / f2: row JSON unavailable",
        );
        same(
            PriorMatchError::CloseDump(CloseDumpError::Drift {
                won: 1,
                assigned: 2,
            }),
            "close-dump drift: the outcome replay's won set (1) does not equal the assignment (2)",
        );
        same(
            PriorMatchError::Inventory(crate::twins::InventoryError::NoProgramBody),
            "estree json program body missing",
        );
        same(
            PermuteError::PriorParse("bad".into()),
            "prior text does not parse: bad",
        );
        same(
            PermuteError::Parse("bad".into()),
            "text does not parse: bad",
        );
        same(
            crate::naming::passes::sweep::SweepParseError("bad".into()),
            "sweep input does not parse: bad",
        );
        same(DiffError("diff: exit 2".into()), "diff: exit 2");
        same(
            StepError::Diff(Box::new(DiffError("diff: exit 2".into()))),
            "diff: exit 2",
        );
        same(
            SplitBodiesError::Parse("bad".into()),
            "the joined bodies do not parse: bad",
        );
        same(
            SplitBodiesError::NotExpression,
            "a joined body is not an expression statement",
        );
        same(
            crate::rename::name_profile::UnknownProfile { name: "x".into() },
            "No name profile named \"x\"",
        );
        use crate::rename::validated::ledger::LedgerError;
        for (e, want) in [
            (
                LedgerError::SourceMismatch,
                "rename ledger: source does not match the ledger's sourceSha256",
            ),
            (
                LedgerError::Overlap,
                "rename ledger: overlapping edits (internal)",
            ),
            (
                LedgerError::OutOfRange,
                "rename ledger: an edit leaves its text (internal)",
            ),
            (
                LedgerError::OutputMismatch,
                "rename ledger: the replay does not reach the ledger's outputSha256",
            ),
        ] {
            same(e, want);
        }
    }

    #[test]
    fn prior_tree_and_inventory_errors() {
        use crate::prior_tree::{PriorTreeError, vendor_manifest_path};
        use crate::twins::InventoryError;
        same(
            PriorTreeError::VendorManifest { why: "bad".into() },
            &format!(
                "prior vendor manifest {} is not one this binary reads (bad); rebuild the prior release with this binary",
                vendor_manifest_path()
            ),
        );
        same(
            PriorTreeError::VendorNamingNotText { why: "bad".into() },
            "prior .humanify/vendor-humanified.js is not UTF-8 text (bad)",
        );
        same(
            InventoryError::EstreeJson(crate::ingest::EstreeJsonError(json_error())),
            &format!("estree json: {}", json_error()),
        );
    }

    #[test]
    fn unpack_errors() {
        use crate::libindex::module_answers::AnswersError;
        use crate::unpack::UnpackError;
        use crate::unpack::bun::BunError;
        use crate::unpack::chunk_link::{DecodeError, LinkError};
        use crate::unpack::input::InputError;
        use crate::unpack::webcrack::{UnpackerError, WebcrackError};
        let p = || PathBuf::from("in/a.js");
        same(UnpackError::Bun(BunError::Refused("refused")), "refused");
        same(
            UnpackError::Webcrack(WebcrackError::NoOutput),
            "webcrack shim: no output",
        );
        same(
            BunError::Answers(AnswersError::Io {
                file: "f".into(),
                source: io("gone"),
            }),
            "f: gone",
        );
        same(WebcrackError::Run(UnpackerError("exit 1".into())), "exit 1");
        same(
            WebcrackError::Json(json_error()),
            &format!("webcrack shim output: {}", json_error()),
        );
        same(UnpackerError("exit 1".into()), "exit 1");
        same(InputError::File(file_error()), "read in/a.js: gone");
        same(
            InputError::Link(LinkError::EmbeddedOutside {
                rest: "../x".into(),
            }),
            "/$bunfs/root/../x leaves the embedded root; not linked",
        );
        let utf16 = char::decode_utf16([0xD800u16]).next().unwrap().unwrap_err();
        same(
            DecodeError::Utf16(utf16.clone()),
            &format!("is neither UTF-8 nor UTF-16LE ({utf16})"),
        );
        let links = [
            (
                LinkError::File(file_error()),
                "read in/a.js: gone".to_string(),
            ),
            (
                LinkError::Parse {
                    path: p(),
                    diagnostic: "bad".into(),
                },
                "in/a.js does not parse as a module: bad".into(),
            ),
            (
                LinkError::MetaProperty {
                    path: p(),
                    prop: "env".into(),
                },
                "in/a.js: `import.meta.env` is not linked".into(),
            ),
            (
                LinkError::MetaShadowed {
                    path: p(),
                    prop: "dir".into(),
                    name: "__dirname",
                },
                "in/a.js: `import.meta.dir` sits where `__dirname` is bound; not linked".into(),
            ),
            (
                LinkError::NoExport {
                    path: p(),
                    name: "x".into(),
                },
                "in/a.js does not export `x`".into(),
            ),
            (
                LinkError::OverlappingEdits { path: p(), at: 5 },
                "in/a.js: overlapping edits at 5".into(),
            ),
            (
                LinkError::EmbeddedMissing {
                    rest: "a.md".into(),
                    path: p(),
                },
                "/$bunfs/root/a.md is read but in/a.js does not exist".into(),
            ),
            (
                LinkError::DirnameBound {
                    path: p(),
                    rest: "a.md".into(),
                },
                "in/a.js: /$bunfs/root/a.md sits where `__dirname` is bound; not linked".into(),
            ),
            (LinkError::Json(json_error()), json_error().to_string()),
        ];
        for (e, want) in links {
            same(e, &want);
        }
    }
}
