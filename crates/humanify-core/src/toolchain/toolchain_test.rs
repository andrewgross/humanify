//! The toolchain is resolved once from detection + flags, and every piece
//! says why it was chosen.

use humanify_model::detection::{BundlerType, DetectionTier, MinifierType};

use super::{
    AppFile, BundleLayout, BundlerTuning, Chosen, InteropHelpers, ModuleWrapperGrammar, Reason,
    resolve_toolchain,
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
    assert_eq!(t.library_detector.piece, LibraryDetector::Bun);
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
    assert_eq!(t.library_detector.piece, LibraryDetector::Esbuild);
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
        ("libraryDetector", "bun", "detected"),
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
    assert_eq!(
        BundleLayout::SingleWrapperFunction.wrapper_parameter_roles(),
        &["exports", "require", "module", "filename", "dirname"]
    );
}
