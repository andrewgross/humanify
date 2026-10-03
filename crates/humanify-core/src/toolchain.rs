//! The run's TOOLCHAIN: which plugin pieces this run uses, decided ONCE at
//! the start of the run from the input's detection and the
//! `--bundler`/`--minifier` flags, then handed down to every stage as
//! values (docs/plugin-spec.md, Part 4 steps 1 and 3; Andrew, 2026-10-04:
//! "building a pipeline based on the inputs and our detection, and then
//! passing those plugin pieces for the relevant tooling to the rest of the
//! pipeline dynamically").
//!
//! Before this, each stage re-derived its own answer from the bundler and
//! minifier NAMES (`== "esbuild"`, `Some("bun" | "esbuild")`), and two of
//! them answered differently (the post-split reconcile hard-coded Bun's
//! never-rename set — spec I21; the finish re-linked only the two stamps it
//! knew — spec I14). Now [`resolve_toolchain`] is the one place a plugin
//! piece is chosen, and nothing downstream names a bundler or a minifier.
//!
//! The pieces (spec numbering):
//!
//! - P2 the unpack adapter — `unpack::UnpackAdapter`, run from ONE
//!   dispatch site (`unpack::run_adapter`);
//! - P5 the vendor record — whether the adapter writes and reads
//!   `vendor/_bun-modules.json`, and its stamp
//!   (`UnpackAdapter::vendor_record_stamp`);
//! - P6 the library detector — `libdetect::LibraryDetector`, the one the
//!   adapter calls for;
//! - P7 the never-rename helper lists — `rename::eligibility::NeverRename`;
//! - P10 the name profile — `rename::name_profile::NameProfile`;
//! - P11 the module-layout record ("fossils") — the adapter's
//!   `provides_module_fossils`;
//! - P14 per-bundler tuning — [`BundlerTuning`].
//!
//! And four SLOTS whose only implementation today is the Bun/esbuild
//! behaviour, moved behind a named value without being redesigned (a
//! second implementation is where a new plugin lands):
//!
//! - P3 the module wrapper grammar — [`ModuleWrapperGrammar`];
//! - P8 the interop helpers for vendored code — [`InteropHelpers`];
//! - P9 the bundle layout — [`BundleLayout`];
//! - P13 which unpacked file is the app — [`AppFile`].
//!
//! SELECTION RULES (the name-profile precedent, finding #75): the flags
//! decide first; a DEFINITIVE bundler detection next (the bundler verdict
//! is never anything else — `detect::pick_bundler`); otherwise the Bun
//! pieces / today's defaults. The minifier DETECTION verdict is measured
//! unreliable and nothing newly reads it: the only piece that reads it is
//! the never-rename lists (swc's helper names), exactly as the naming
//! stage always has. Every choice is recorded with its [`Reason`]
//! ([`Toolchain::record`], the `--stats-json` `toolchain` block).

use humanify_model::detection::{BundlerDetectionResult, BundlerType, DetectionTier, MinifierType};

use crate::libdetect::LibraryDetector;
use crate::rename::eligibility::NeverRename;
use crate::rename::name_profile::{NameProfile, select_name_profile};
use crate::unpack::{UnpackAdapter, choose_adapter};

#[cfg(test)]
mod toolchain_test;

/// Why a piece was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A `--bundler` / `--minifier` flag decided it.
    Flag,
    /// Detection decided it (for a bundler: a definitive verdict).
    Detected,
    /// Nothing confident was known: the default (Bun's piece, or the
    /// adapter registry's do-nothing last entry).
    Fallback,
    /// The slot has one implementation today; every input gets it.
    OnlyImplementation,
}

impl Reason {
    /// The recorded spelling.
    pub fn name(self) -> &'static str {
        match self {
            Reason::Flag => "flag",
            Reason::Detected => "detected",
            Reason::Fallback => "fallback",
            Reason::OnlyImplementation => "only-implementation",
        }
    }
}

/// A selected piece and why it was selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chosen<T> {
    pub piece: T,
    pub reason: Reason,
}

/// P3 — where the bundled third-party modules are and what each module's
/// body is. ONE grammar today: Bun's `{exports:{}}` factory marker, then
/// esbuild's declared `__commonJS` (`modules::identify_cjs_factory`,
/// `modules::factory_arg_function`). Its consumers (the unpack's
/// classification and the naming stage's third-party skip, spec I10) still
/// call those owners directly; routing them through this value is spec
/// Part 4 step 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleWrapperGrammar {
    BunAndEsbuild,
}

impl ModuleWrapperGrammar {
    pub fn name(self) -> &'static str {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => "bun+esbuild",
        }
    }

    /// The module helper this grammar recognises in `source`.
    pub fn identify_factory_helper(self, source: &str) -> Option<crate::modules::IdentifiedHelper> {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => crate::modules::identify_cjs_factory(source),
        }
    }
}

/// P8 — how a vendored module's references to the bundle's interop helpers
/// keep working in the runnable tree: the helper file the finish writes
/// beside the vendor files. ONE implementation: Bun's (`__commonJS`,
/// `__esm`, `__toESM`, `__toCommonJS`), which esbuild's factories also
/// run under. The helper SHAPES the unpack recognises
/// (`unpack::bun::scope`, spec I16/I17) are not yet behind this slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteropHelpers {
    Bun,
}

impl InteropHelpers {
    pub fn name(self) -> &'static str {
        match self {
            InteropHelpers::Bun => "bun",
        }
    }

    /// The helper file's text (`.humanify/__bun-runtime.js`).
    pub fn relink_runtime(self) -> &'static str {
        match self {
            InteropHelpers::Bun => crate::finish::relink::BUN_RELINK_RUNTIME,
        }
    }
}

/// P9 — where the bundle's top-level statements are. ONE grammar: the
/// whole program inside a single wrapper function declaring at least 50
/// names (`modules::wrapper`). Its ten readers still call
/// `modules::wrapper` directly (spec I25 — the container refactor, Part 4
/// step 7); the slot names the rule the run is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleLayout {
    SingleWrapperFunction,
}

impl BundleLayout {
    pub fn name(self) -> &'static str {
        match self {
            BundleLayout::SingleWrapperFunction => "single-wrapper-function",
        }
    }
}

/// P13 — which of the unpacked files is the bundle's own code (what the
/// split cuts up and the next release's prior is made from). ONE rule: the
/// last file the naming stage processed — the adapters write the app's
/// file last (spec I26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppFile {
    LastProcessed,
}

impl AppFile {
    pub fn name(self) -> &'static str {
        match self {
            AppFile::LastProcessed => "last-processed",
        }
    }

    /// Whether a newly processed file replaces the app file chosen so far.
    pub fn replaces_earlier(self) -> bool {
        match self {
            AppFile::LastProcessed => true,
        }
    }
}

/// P14 — per-bundler tuning knobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundlerTuning {
    /// esbuild's: module-level names asked in groups of 15.
    Esbuild,
    /// Everything else: groups of 10.
    Default,
}

impl BundlerTuning {
    pub fn name(self) -> &'static str {
        match self {
            BundlerTuning::Esbuild => "esbuild",
            BundlerTuning::Default => "default",
        }
    }

    /// How many module-level names one naming request carries.
    pub fn module_group_size(self) -> usize {
        match self {
            BundlerTuning::Esbuild => 15,
            BundlerTuning::Default => 10,
        }
    }
}

/// Every piece this run uses, chosen once ([`resolve_toolchain`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toolchain {
    /// The bundler the run is treated as (a flag, else detection).
    pub bundler: BundlerType,
    pub bundler_tier: DetectionTier,
    /// The minifier verdict (a flag, else detection). Recorded; read only
    /// by the never-rename lists (module doc).
    pub minifier: MinifierType,
    pub unpack: Chosen<UnpackAdapter>,
    pub library_detector: Chosen<LibraryDetector>,
    pub never_rename: Chosen<NeverRename>,
    pub name_profile: Chosen<NameProfile>,
    pub tuning: Chosen<BundlerTuning>,
    pub module_wrappers: Chosen<ModuleWrapperGrammar>,
    pub interop: Chosen<InteropHelpers>,
    pub layout: Chosen<BundleLayout>,
    pub app_file: Chosen<AppFile>,
}

/// One row of the recorded toolchain: the piece, what was chosen, why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceRecord {
    pub piece: &'static str,
    pub choice: String,
    pub reason: Reason,
}

fn only<T>(piece: T) -> Chosen<T> {
    Chosen {
        piece,
        reason: Reason::OnlyImplementation,
    }
}

/// `resolve_toolchain`: THE place a run's plugin pieces are chosen. An
/// override of `unknown` is no override (the CLI's sentinel).
pub fn resolve_toolchain(
    detection: &BundlerDetectionResult,
    bundler_override: Option<BundlerType>,
    minifier_override: Option<MinifierType>,
) -> Toolchain {
    let bundler_flag = bundler_override.filter(|b| *b != BundlerType::Unknown);
    let minifier_flag = minifier_override.filter(|m| *m != MinifierType::Unknown);
    let bundler = bundler_flag.unwrap_or(detection.bundler.kind);
    let minifier = minifier_flag.unwrap_or(detection.minifier.kind);
    // The bundler a piece keyed on it reads: a flag, else the (always
    // definitive) detected verdict; `unknown` falls back.
    let bundler_reason = if bundler_flag.is_some() {
        Reason::Flag
    } else if bundler != BundlerType::Unknown {
        Reason::Detected
    } else {
        Reason::Fallback
    };
    let unpack = choose_adapter(detection, bundler_override);
    let name_profile = {
        let piece = select_name_profile(detection, bundler_override, minifier_override);
        // `select_name_profile`'s own order: the minifier flag, then a
        // bundler that minifies with its own renamer, then the fallback.
        let reason = if minifier_flag.is_some() {
            Reason::Flag
        } else if NameProfile::of_bundler(bundler).is_some() {
            bundler_reason
        } else {
            Reason::Fallback
        };
        Chosen { piece, reason }
    };
    let never_rename = Chosen {
        piece: NeverRename::for_verdicts(bundler, minifier),
        reason: if bundler_flag.is_some() || minifier_flag.is_some() {
            Reason::Flag
        } else if bundler != BundlerType::Unknown || minifier != MinifierType::Unknown {
            Reason::Detected
        } else {
            Reason::Fallback
        },
    };
    let tuning = if bundler == BundlerType::Esbuild {
        Chosen {
            piece: BundlerTuning::Esbuild,
            reason: bundler_reason,
        }
    } else {
        Chosen {
            piece: BundlerTuning::Default,
            reason: Reason::Fallback,
        }
    };
    Toolchain {
        bundler,
        bundler_tier: if bundler_flag.is_some() {
            DetectionTier::Definitive
        } else {
            detection.bundler.tier
        },
        minifier,
        unpack,
        library_detector: Chosen {
            piece: LibraryDetector::for_adapter(unpack.piece),
            reason: unpack.reason,
        },
        never_rename,
        name_profile,
        tuning,
        module_wrappers: only(ModuleWrapperGrammar::BunAndEsbuild),
        interop: only(InteropHelpers::Bun),
        layout: only(BundleLayout::SingleWrapperFunction),
        app_file: only(AppFile::LastProcessed),
    }
}

impl Toolchain {
    /// Every piece in pipeline order, as the run records it.
    pub fn record(&self) -> Vec<PieceRecord> {
        let row = |piece: &'static str, choice: &str, reason: Reason| PieceRecord {
            piece,
            choice: choice.to_string(),
            reason,
        };
        let adapter = self.unpack.piece;
        vec![
            row("unpackAdapter", adapter.name(), self.unpack.reason),
            row(
                "moduleWrappers",
                self.module_wrappers.piece.name(),
                self.module_wrappers.reason,
            ),
            row(
                "vendorRecord",
                adapter.vendor_record_stamp().unwrap_or("none"),
                self.unpack.reason,
            ),
            row(
                "libraryDetector",
                self.library_detector.piece.name(),
                self.library_detector.reason,
            ),
            row(
                "neverRename",
                &self.never_rename.piece.name(),
                self.never_rename.reason,
            ),
            row(
                "interopHelpers",
                self.interop.piece.name(),
                self.interop.reason,
            ),
            row("bundleLayout", self.layout.piece.name(), self.layout.reason),
            row(
                "nameProfile",
                self.name_profile.piece.name(),
                self.name_profile.reason,
            ),
            row(
                "moduleFossils",
                if adapter.provides_module_fossils() {
                    "fossils"
                } else {
                    "none"
                },
                self.unpack.reason,
            ),
            row("appFile", self.app_file.piece.name(), self.app_file.reason),
            row(
                "bundlerTuning",
                self.tuning.piece.name(),
                self.tuning.reason,
            ),
        ]
    }
}
