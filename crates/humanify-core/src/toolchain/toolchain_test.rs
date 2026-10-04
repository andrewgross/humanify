//! The toolchain is resolved once from detection + flags, and every piece
//! says why it was chosen.

use humanify_model::detection::{BundlerType, DetectionTier, MinifierType};

use super::{
    AppFile, BundleLayout, BundlerTuning, COMMONJS_CONTEXT, Chosen, InteropHelpers,
    ModuleWrapperGrammar, Reason, is_commonjs_context_name, resolve_toolchain,
};
use crate::detect::detect_bundle;
use crate::libdetect::LibraryDetector;
use crate::rename::eligibility::NeverRename;
use crate::rename::name_profile::NameProfile;
use crate::unpack::UnpackAdapter;

const BUN_HEAD: &str = "// @bun\nvar x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);";
const ESBUILD: &str = "\nvar __defProp = Object.defineProperty;\nvar __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };\nvar __toESM = (mod) => __defProp(mod, \"__esModule\", { value: true });\n";
const PLAIN: &str = "\nfunction greet(name) {\n  console.log(\"Hello, \" + name + \"!\");\n}\n";

#[test]
fn a_bun_bundle_gets_the_bun_pieces_by_detection() {
    let t = resolve_toolchain(&detect_bundle(BUN_HEAD), None, None);
    assert_eq!(t.bundler, BundlerType::Bun);
    assert_eq!(t.bundler_tier, DetectionTier::Definitive);
    assert_eq!(
        t.unpack,
        Chosen {
            piece: UnpackAdapter::Bun,
            reason: Reason::Detected
        }
    );
    assert_eq!(t.library_detector.piece, LibraryDetector::VendorRecord);
    assert_eq!(t.name_profile.piece, NameProfile::Bun);
    assert_eq!(t.name_profile.reason, Reason::Detected);
    assert_eq!(t.tuning.piece, BundlerTuning::Default);
    assert_eq!(t.tuning.piece.module_group_size(), 10);
    assert_eq!(t.unpack.piece.vendor_record_stamp(), Some("bun"));
}

#[test]
fn an_esbuild_bundle_gets_the_esbuild_pieces_by_detection() {
    let t = resolve_toolchain(&detect_bundle(ESBUILD), None, None);
    assert_eq!(t.unpack.piece, UnpackAdapter::Esbuild);
    assert_eq!(t.library_detector.piece, LibraryDetector::VendorRecord);
    assert_eq!(t.name_profile.piece, NameProfile::Esbuild);
    assert_eq!(
        t.tuning,
        Chosen {
            piece: BundlerTuning::Esbuild,
            reason: Reason::Detected
        }
    );
    assert_eq!(t.tuning.piece.module_group_size(), 15);
    // esbuild's runtime helper names join the never-rename lists.
    assert_eq!(t.never_rename.piece.name(), "esbuild");
    assert_eq!(t.unpack.piece.vendor_record_stamp(), Some("esbuild"));
}

#[test]
fn unknown_input_falls_back_to_the_do_nothing_adapter_and_the_bun_profile() {
    let t = resolve_toolchain(&detect_bundle(PLAIN), None, None);
    assert_eq!(
        t.unpack,
        Chosen {
            piece: UnpackAdapter::Passthrough,
            reason: Reason::Fallback
        }
    );
    assert_eq!(t.library_detector.piece, LibraryDetector::Default);
    assert_eq!(
        t.name_profile,
        Chosen {
            piece: NameProfile::Bun,
            reason: Reason::Fallback
        }
    );
    assert_eq!(t.tuning.reason, Reason::Fallback);
    assert_eq!(t.unpack.piece.vendor_record_stamp(), None);
}

#[test]
fn flags_decide_and_say_so() {
    let t = resolve_toolchain(
        &detect_bundle(PLAIN),
        Some(BundlerType::Esbuild),
        Some(MinifierType::None),
    );
    assert_eq!(
        t.unpack,
        Chosen {
            piece: UnpackAdapter::Esbuild,
            reason: Reason::Flag
        }
    );
    assert_eq!(
        t.name_profile,
        Chosen {
            piece: NameProfile::NotMinified,
            reason: Reason::Flag
        }
    );
    assert_eq!(t.tuning.reason, Reason::Flag);
    assert_eq!(t.never_rename.reason, Reason::Flag);
    assert_eq!(t.minifier, MinifierType::None);
    // `unknown` is the CLI's no-override sentinel, never a flag.
    let t = resolve_toolchain(
        &detect_bundle(BUN_HEAD),
        Some(BundlerType::Unknown),
        Some(MinifierType::Unknown),
    );
    assert_eq!(t.unpack.reason, Reason::Detected);
    assert_eq!(t.name_profile.reason, Reason::Detected);
}

/// A Bun build carrying esbuild's helper names (every unminified `bun
/// build` does): the banner outranks them, the run gets Bun's pieces, and
/// every piece the bundler decided records that a conflict was settled by
/// strength (review R3).
#[test]
fn a_conflict_settled_by_strength_is_recorded_on_every_bundler_piece() {
    let code = format!("// @bun\n{ESBUILD}");
    let t = resolve_toolchain(&detect_bundle(&code), None, None);
    assert_eq!(t.bundler, BundlerType::Bun);
    assert_eq!(t.bundler_tier, DetectionTier::Definitive);
    assert_eq!(
        t.unpack,
        Chosen {
            piece: UnpackAdapter::Bun,
            reason: Reason::DetectedByStrength
        }
    );
    assert_eq!(t.library_detector.reason, Reason::DetectedByStrength);
    assert_eq!(
        t.name_profile,
        Chosen {
            piece: NameProfile::Bun,
            reason: Reason::DetectedByStrength
        }
    );
    assert_eq!(t.never_rename.reason, Reason::DetectedByStrength);
    assert_eq!(t.tuning.piece, BundlerTuning::Default);
    let reasons: Vec<&str> = t.record().iter().map(|r| r.reason.name()).collect();
    assert!(reasons.contains(&"detected-by-strength"));
    assert!(!reasons.contains(&"detected"));
    // A flag still decides, and says so.
    let t = resolve_toolchain(&detect_bundle(&code), Some(BundlerType::Esbuild), None);
    assert_eq!(t.unpack.reason, Reason::Flag);
}

/// Two bundlers tied at the strongest rank: no verdict, the do-nothing
/// adapter, and the record says the fallback came from a tie.
#[test]
fn a_tie_falls_back_and_says_why() {
    let t = resolve_toolchain(
        &detect_bundle("var parcelRequire; __webpack_require__(1);"),
        None,
        None,
    );
    assert_eq!(t.bundler, BundlerType::Unknown);
    assert_eq!(
        t.unpack,
        Chosen {
            piece: UnpackAdapter::Passthrough,
            reason: Reason::FallbackOnTie
        }
    );
    assert_eq!(t.name_profile.reason, Reason::FallbackOnTie);
    assert_eq!(Reason::FallbackOnTie.name(), "fallback-on-tie");
    assert_eq!(Reason::DetectedByStrength.name(), "detected-by-strength");
}

/// A `likely` bundler verdict (a weak token alone: `installedModules`) is
/// reported, never acted on — the toolchain takes definitive verdicts
/// only (review R19: it used to send such input to webcrack).
#[test]
fn a_likely_verdict_is_not_acted_on() {
    let d = detect_bundle("var installedModules = {};\nfunction f(){}\n");
    assert_eq!(d.bundler.kind, BundlerType::Browserify);
    assert_eq!(d.bundler.tier, DetectionTier::Likely);
    let t = resolve_toolchain(&d, None, None);
    assert_eq!(t.bundler, BundlerType::Unknown);
    assert_eq!(t.bundler_tier, DetectionTier::Unknown);
    assert_eq!(
        t.unpack,
        Chosen {
            piece: UnpackAdapter::Passthrough,
            reason: Reason::Fallback
        }
    );
    assert_eq!(t.name_profile.reason, Reason::Fallback);
}

/// The minifier DETECTION verdict is not newly trusted: it moves only the
/// never-rename lists (swc's helpers — what the naming stage always read),
/// never the name profile.
#[test]
fn a_detected_swc_minifier_moves_only_the_never_rename_lists() {
    let swc = "var a = _interop_require_default(b);\nfunction c(){}\n";
    let d = detect_bundle(swc);
    assert_eq!(d.minifier.kind, MinifierType::Swc);
    let t = resolve_toolchain(&d, None, None);
    assert_eq!(t.never_rename.piece.name(), "swc");
    assert_eq!(t.never_rename.reason, Reason::Detected);
    assert_eq!(t.name_profile.piece, NameProfile::Bun);
    assert_eq!(t.name_profile.reason, Reason::Fallback);
}

#[test]
fn the_slots_hold_their_only_implementation() {
    for code in [BUN_HEAD, ESBUILD, PLAIN] {
        let t = resolve_toolchain(&detect_bundle(code), None, None);
        let only = Reason::OnlyImplementation;
        assert_eq!(
            t.module_wrappers,
            Chosen {
                piece: ModuleWrapperGrammar::BunAndEsbuild,
                reason: only
            }
        );
        assert_eq!(
            t.interop,
            Chosen {
                piece: InteropHelpers::Bun,
                reason: only
            }
        );
        assert_eq!(
            t.layout,
            Chosen {
                piece: BundleLayout::SingleWrapperFunction,
                reason: only
            }
        );
        assert_eq!(
            t.app_file,
            Chosen {
                piece: AppFile::LastProcessed,
                reason: only
            }
        );
    }
    assert_eq!(
        InteropHelpers::Bun.relink_runtime(),
        crate::finish::relink::BUN_RELINK_RUNTIME
    );
    assert!(
        ModuleWrapperGrammar::BunAndEsbuild
            .identify_factory_helper(BUN_HEAD)
            .is_some()
    );
}

/// P8 end to end (toolchain review R4): the helper SHAPES the unpack
/// recognises, the standard names, the helper file's path and the module
/// helper are all answered by the run's interop piece — Bun's values,
/// byte-for-byte the constants they replaced.
#[test]
fn the_interop_piece_owns_shapes_names_and_the_helper_file() {
    let bun = InteropHelpers::Bun;
    assert_eq!(bun.runtime_file(), ".humanify/__bun-runtime.js");
    assert_eq!(bun.module_helper(), "__commonJS");
    assert_eq!(bun.canonical_names(), &["__toESM", "__toCommonJS"]);
    let shape_of = |src: &str| {
        let allocator = oxc_allocator::Allocator::default();
        let ingest = crate::ingest::Ingest::parse_unambiguous(&allocator, src);
        let oxc_ast::ast::Statement::VariableDeclaration(decl) = &ingest.program.body[0] else {
            panic!("a var statement");
        };
        bun.recognise(src, decl.declarations[0].init.as_ref().unwrap())
    };
    assert_eq!(
        shape_of(
            "var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,\"default\",{value:I}),I.__esModule);"
        ),
        Some("__toESM")
    );
    assert_eq!(
        shape_of(
            "var T=(I)=>{var A=new WeakMap;return Object.defineProperty({},\"__esModule\",{value:!0})};"
        ),
        Some("__toCommonJS")
    );
    assert_eq!(shape_of("var f=(a)=>a+1;"), None);
}

/// The record names every piece, in pipeline order, with its reason.
#[test]
fn the_record_lists_every_piece() {
    let t = resolve_toolchain(&detect_bundle(BUN_HEAD), None, None);
    let rows: Vec<(String, String, &str)> = t
        .record()
        .into_iter()
        .map(|r| (r.piece.to_string(), r.choice, r.reason.name()))
        .collect();
    let want = [
        ("unpackAdapter", "bun", "detected"),
        ("moduleWrappers", "bun+esbuild", "only-implementation"),
        ("vendorRecord", "bun", "detected"),
        ("libraryDetector", "vendor-record", "detected"),
        ("neverRename", "universal", "detected"),
        ("interopHelpers", "bun", "only-implementation"),
        (
            "bundleLayout",
            "single-wrapper-function",
            "only-implementation",
        ),
        ("nameProfile", "bun", "detected"),
        ("moduleFossils", "fossils", "detected"),
        ("appFile", "last-processed", "only-implementation"),
        ("bundlerTuning", "default", "fallback"),
    ];
    let want: Vec<(String, String, &str)> = want
        .iter()
        .map(|(p, c, r)| (p.to_string(), c.to_string(), *r))
        .collect();
    assert_eq!(rows, want);
}

/// The never-rename lists are the one value every eligibility question
/// reads: Bun's verdict adds nothing to the universal list (the finish's
/// post-split reconcile used to hard-code exactly that — spec I21).
#[test]
fn bun_verdicts_need_only_the_universal_list() {
    assert_eq!(
        NeverRename::for_verdicts(BundlerType::Bun, MinifierType::Bun),
        NeverRename::UNIVERSAL
    );
}

/// A wrapper IIFE declaring `n` names directly in its scope.
fn wrapper_with(n: usize) -> String {
    let decls: Vec<String> = (0..n).map(|i| format!("var v{i}={i};")).collect();
    format!("(function(){{{}return v0;}})();", decls.concat())
}

type WrapperSpans = Option<(oxc_span::Span, oxc_span::Span, usize)>;

fn spans_of(w: Option<crate::modules::wrapper::WrapperFunction>) -> WrapperSpans {
    w.map(|w| (w.span, w.body_span, w.binding_count))
}

/// P9: the bundle layout ANSWERS the layout questions (where the wrapper
/// is, with and without the run's input gate, and the input gate itself)
/// by delegating to the one grammar owner, `modules::wrapper` — the same
/// answers, byte for byte.
#[test]
fn the_single_wrapper_layout_answers_through_the_wrapper_grammar() {
    use crate::modules::wrapper as owner;
    let layout = BundleLayout::SingleWrapperFunction;
    for n in [3usize, 49, 50, 60] {
        let src = wrapper_with(n);
        let allocator = oxc_allocator::Allocator::default();
        let ingest = crate::ingest::Ingest::parse(&allocator, &src, "input.js");
        assert!(ingest.errors.is_empty());
        let (p, s) = (ingest.program, ingest.semantic());
        assert_eq!(
            spans_of(layout.find_wrapper(p, s)),
            spans_of(owner::find_wrapper_function(p, s)),
            "find, n={n}"
        );
        assert_eq!(
            spans_of(layout.recognize_wrapper(p, s)),
            spans_of(owner::recognize_wrapper_function(p, s)),
            "recognize, n={n}"
        );
        // The threshold gate: under 50 names the grammar still recognises
        // the wrapper, the gate refuses it.
        assert_eq!(layout.find_wrapper(p, s).is_some(), n >= 50, "n={n}");
        assert!(layout.recognize_wrapper(p, s).is_some(), "n={n}");
        assert_eq!(
            layout.original_bundle_binding_count(&src),
            owner::original_bundle_binding_count(&src),
            "original gate, n={n}"
        );
    }
    // No wrapper at all: every answer is "none".
    let plain = "var a=1;console.log(a);";
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, plain, "input.js");
    let (p, s) = (ingest.program, ingest.semantic());
    assert!(layout.find_wrapper(p, s).is_none());
    assert!(layout.recognize_wrapper(p, s).is_none());
    assert!(layout.original_bundle_binding_count(plain).is_err());
}

/// P9 (review R5): the wrapper's parameters are the CommonJS entry
/// context, by POSITION — Node's/Bun's `(exports, require, module,
/// __filename, __dirname)`. The runnable emit reads the roles from the
/// layout, not from a list of its own.
#[test]
fn the_single_wrapper_layout_names_the_commonjs_parameter_roles_by_position() {
    let roles: Vec<&str> = BundleLayout::SingleWrapperFunction
        .wrapper_parameter_roles()
        .iter()
        .map(|c| c.role)
        .collect();
    assert_eq!(
        roles,
        ["exports", "require", "module", "filename", "dirname"]
    );
}

/// Toolchain review R21: "what the CommonJS module system hands the
/// bundle" is ONE list (`COMMONJS_CONTEXT`), and every reader asks it —
/// the never-rename set, the unpack's "resolved in the vendor file
/// anyway", the proximity window's always-kept names, the known-globals
/// report and the wrapper's parameter roles. They used to be five lists.
#[test]
fn the_commonjs_context_names_have_one_owner_and_every_reader_agrees() {
    let names: Vec<&str> = COMMONJS_CONTEXT.iter().map(|c| c.name).collect();
    assert_eq!(
        names,
        ["exports", "require", "module", "__filename", "__dirname"]
    );
    let by_position: Vec<&str> = BundleLayout::SingleWrapperFunction
        .wrapper_parameter_roles()
        .iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(by_position, names);
    for name in names {
        assert!(is_commonjs_context_name(name), "{name}");
        assert!(
            !crate::rename::eligibility::is_eligible(name, NeverRename::UNIVERSAL),
            "{name} is never renamed"
        );
        assert!(
            crate::rename::votes::proximity::is_well_known_name(name),
            "{name} is always kept by the proximity window"
        );
        assert!(
            crate::modules::known_globals::is_known_global(name),
            "{name} is a known global"
        );
    }
    assert!(!is_commonjs_context_name("process"));
    assert!(!is_commonjs_context_name("filename"));
}

/// P3 (toolchain review R11/R2/R7/R12): the module grammar answers which
/// helper wraps the bundled modules — Bun's tight marker, esbuild's
/// declared `__commonJS` — and which bindings are lazy-init helpers, in
/// Bun's formatted form and esbuild's; a look-alike is neither.
#[test]
fn the_module_grammar_names_the_module_helper_and_the_lazy_init_helpers() {
    let grammar = ModuleWrapperGrammar::BunAndEsbuild;
    let helper = |src: &str| grammar.identify_factory_helper(src).map(|h| h.name);
    assert_eq!(helper(BUN_HEAD).as_deref(), Some("x"));
    assert_eq!(helper(ESBUILD).as_deref(), Some("__commonJS"));
    assert_eq!(helper(PLAIN), None);
    let lazy = |src: &str| {
        let allocator = oxc_allocator::Allocator::default();
        let ingest = crate::ingest::Ingest::parse(&allocator, src, "t.js");
        let json = crate::ingest::program_estree_json(ingest.program);
        let mut names: Vec<String> = grammar
            .lazy_init_helpers(json["body"].as_array().expect("body"))
            .into_iter()
            .collect();
        names.sort();
        names
    };
    assert_eq!(
        lazy("var Z = (H, q) => () => (H && (q = H(H = 0)), q);"),
        vec!["Z".to_string()]
    );
    assert_eq!(
        lazy(
            "var __esm = (fn, res) => function __init() {\n  return fn && (res = (0, fn[Object.keys(fn)[0]])(fn = 0)), res;\n};"
        ),
        vec!["__esm".to_string()]
    );
    assert!(lazy("var memo = (f, r) => () => r;").is_empty());
}
