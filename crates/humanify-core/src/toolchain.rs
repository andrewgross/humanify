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
//! - P14 per-bundler tuning — [`BundlerTuning`];
//! - P17 what the bundler kept about each module's identity —
//!   [`ModuleIdentitySource`] (finding #95).
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
//! decide first; a DEFINITIVE bundler detection next (a `likely` verdict —
//! a weak token alone — is never acted on, review R19); otherwise the Bun
//! pieces / today's defaults. The bundler verdict is decided by signal
//! strength (`detect::pick_bundler`); when its signals named another
//! bundler too, every piece the bundler decided records it
//! (`detected-by-strength`, or `fallback-on-tie` when two bundlers tied
//! and detection picked none — review R3). The minifier DETECTION verdict is measured
//! unreliable and nothing newly reads it: the only piece that reads it is
//! the never-rename lists (swc's helper names), exactly as the naming
//! stage always has. Every choice is recorded with its [`Reason`]
//! ([`Toolchain::record`], the `--stats-json` `toolchain` block).

use std::collections::HashSet;

use humanify_model::detection::{
    BundlerDetectionResult, BundlerType, ConflictResolution, DetectionTier, MinifierType,
};

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
    /// A definitive bundler verdict whose signals named another bundler
    /// too, settled by signal strength (the verdict's `conflict`; e.g. a
    /// Bun build's banner over the esbuild helper names Bun also writes).
    DetectedByStrength,
    /// Nothing confident was known: the default (Bun's piece, or the
    /// adapter registry's do-nothing last entry).
    Fallback,
    /// The fallback, because two bundlers tied at the strongest signal
    /// rank and detection refused to pick one (the verdict's `conflict`).
    FallbackOnTie,
    /// The slot has one implementation today; every input gets it.
    OnlyImplementation,
}

impl Reason {
    /// The recorded spelling.
    pub fn name(self) -> &'static str {
        match self {
            Reason::Flag => "flag",
            Reason::Detected => "detected",
            Reason::DetectedByStrength => "detected-by-strength",
            Reason::Fallback => "fallback",
            Reason::FallbackOnTie => "fallback-on-tie",
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
/// body is. ONE grammar today, shared by the Bun and esbuild adapters
/// (esbuild's flow IS Bun's — `unpack::run_adapter` hands both to
/// `unpack::bun::unpack_bun`): the module helper by its shape, in every
/// form either bundler writes (`modules::identify_cjs_factory`); the
/// factory argument in either form (`modules::factory_arg_function`); the
/// classification of every `var X = HELPER(factory)` in the container
/// (`modules::classify_bun_modules`). Every pipeline caller asks the run's
/// value (toolchain review R11/R7, 2026-10-04): the unpack's
/// classification and extraction, the artifact dump's classification
/// sites, and the fresh-grouping split's vendor bucket
/// (`place::assign::cluster`, which used to tally its own "most-used
/// higher-order callee"). The UNPACK is the one place that decides which
/// code is a bundled module: the naming stage no longer re-runs the
/// grammar per file (finding #80, review R2, spec I10) — a factory the
/// unpack kept in the app is app code. A second grammar (webpack's module
/// table, rollup's) is a second variant here.
///
/// exp105 added two: webpack's (`Webpack`: the module table, the CommonJS
/// wrapper `.cw(fn)`, the entry IIFEs) and rollup's (`Rollup`: the
/// commonjs plugin's guarded require functions) — module bodies written
/// INLINE, with no module helper (`modules::inline_modules`). Neither has
/// a helper's factories nor `__esm` inits; their modules reach the stages
/// only through the statement cut ([`Self::statement_modules`],
/// [`Self::declarator_modules`]), which makes each body a lazy module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleWrapperGrammar {
    BunAndEsbuild,
    Webpack,
    Rollup,
}

impl ModuleWrapperGrammar {
    pub fn name(self) -> &'static str {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => "bun+esbuild",
            ModuleWrapperGrammar::Webpack => "webpack",
            ModuleWrapperGrammar::Rollup => "rollup",
        }
    }

    /// The module helper this grammar recognises in `source` (its binding
    /// name and where its declaration starts), or None: the text holds no
    /// bundled modules under this grammar.
    pub fn identify_factory_helper(self, source: &str) -> Option<crate::modules::IdentifiedHelper> {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => crate::modules::identify_cjs_factory(source),
            ModuleWrapperGrammar::Webpack | ModuleWrapperGrammar::Rollup => None,
        }
    }

    /// The module bodies this grammar writes INLINE in one container
    /// statement (rollup's guarded require declarations): each with the
    /// span of the node that wraps it.
    pub fn statement_modules<'a>(
        self,
        stmt: &'a oxc_ast::ast::Statement<'a>,
    ) -> Vec<crate::modules::inline_modules::InlineModule<'a>> {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild | ModuleWrapperGrammar::Webpack => Vec::new(),
            ModuleWrapperGrammar::Rollup => {
                crate::modules::inline_modules::rollup_statement_modules(stmt)
            }
        }
    }

    /// The number literals the unit prints read as one number (their
    /// start offsets): webpack's module ids
    /// (`modules::inline_modules::webpack_module_id_literals`); none for
    /// the other grammars.
    pub fn blurred_literals(
        self,
        statements: &[oxc_ast::ast::Statement<'_>],
        semantic: &oxc_semantic::Semantic<'_>,
    ) -> HashSet<u32> {
        match self {
            ModuleWrapperGrammar::Webpack => {
                crate::modules::inline_modules::webpack_module_id_literals(statements, semantic)
            }
            ModuleWrapperGrammar::BunAndEsbuild | ModuleWrapperGrammar::Rollup => HashSet::new(),
        }
    }

    /// The SCOPES this grammar writes inline in one container statement —
    /// not modules: wrappers whose statements are cut like the
    /// container's own (webpack's entry IIFEs, which hold concatenated,
    /// scope-hoisted modules).
    pub fn statement_scopes<'a>(
        self,
        stmt: &'a oxc_ast::ast::Statement<'a>,
    ) -> Vec<crate::modules::inline_modules::InlineModule<'a>> {
        match self {
            ModuleWrapperGrammar::Webpack => {
                crate::modules::inline_modules::webpack_statement_scopes(stmt)
            }
            ModuleWrapperGrammar::BunAndEsbuild | ModuleWrapperGrammar::Rollup => Vec::new(),
        }
    }

    /// The module bodies this grammar writes INLINE in one declarator
    /// (webpack's module table and CommonJS wrapper, rollup's guarded
    /// require IIFEs).
    pub fn declarator_modules<'a>(
        self,
        d: &'a oxc_ast::ast::VariableDeclarator<'a>,
    ) -> Vec<crate::modules::inline_modules::InlineModule<'a>> {
        use crate::modules::inline_modules as inline;
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => Vec::new(),
            ModuleWrapperGrammar::Webpack => inline::webpack_declarator_modules(d),
            ModuleWrapperGrammar::Rollup => inline::rollup_declarator_modules(d),
        }
    }

    /// Every bundled module (`var X = HELPER(factory)`) among the
    /// container's statements — `wrapper_body` is the layout's container
    /// (None: the program's own body). None when the text has no helper.
    pub fn classify_factories<'a>(
        self,
        source: &'a str,
        program: &'a oxc_ast::ast::Program<'a>,
        semantic: &'a oxc_semantic::Semantic<'a>,
        wrapper_body: Option<oxc_span::Span>,
        tables: &crate::hash::serialize::SymbolTables,
    ) -> Option<crate::modules::BunModuleClassification> {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => crate::modules::classify_bun_modules(
                source,
                program,
                semantic,
                wrapper_body,
                tables,
            ),
            ModuleWrapperGrammar::Webpack | ModuleWrapperGrammar::Rollup => None,
        }
    }

    /// The module's factory function inside a helper call's first
    /// argument, in the forms this grammar accepts (Bun's function,
    /// esbuild's single-key object whose key is the source path).
    pub fn factory_function<'a>(
        self,
        arg: &'a oxc_ast::ast::Expression<'a>,
    ) -> Option<crate::modules::FactoryArg<'a>> {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => crate::modules::factory_arg_function(arg),
            ModuleWrapperGrammar::Webpack | ModuleWrapperGrammar::Rollup => None,
        }
    }

    /// The lazy-init (ES-module init) helpers declared among the
    /// container's top-level statements (their ESTree JSON): a call to one
    /// does nothing until the module is first used. ONE recogniser for the
    /// split's load order (spec P12, I27) and the fossil layout (P11) —
    /// toolchain review R12: Bun's and esbuild's forms, raw and formatted
    /// (`twins::fossil::lazy_init_helper_names`). The load order used to
    /// read Bun's TEXT `x && (y = x(x = 0))` on its own; on the real Bun
    /// pair both recognisers named the same binding.
    pub fn lazy_init_helpers(self, body: &[serde_json::Value]) -> HashSet<String> {
        match self {
            ModuleWrapperGrammar::BunAndEsbuild => {
                crate::twins::fossil::lazy_init_helper_names(body)
            }
            ModuleWrapperGrammar::Webpack | ModuleWrapperGrammar::Rollup => HashSet::new(),
        }
    }

    /// [`Self::lazy_init_helpers`] over a container's typed statements
    /// (the raw bundle's, where no ESTree JSON of the program exists):
    /// every top-level declarator short enough to be a helper is handed to
    /// the same shape check as a one-declarator `var`.
    pub fn lazy_init_helpers_in(
        self,
        statements: &[oxc_ast::ast::Statement<'_>],
    ) -> HashSet<String> {
        use oxc_estree::{CompactSerializer, ESTree};
        /// Longer than any helper's text: Bun's is 37 bytes minified,
        /// esbuild's ~150 unminified.
        const HELPER_TEXT_MAX: u32 = 512;
        let body: Vec<serde_json::Value> = statements
            .iter()
            .filter_map(|s| match s {
                oxc_ast::ast::Statement::VariableDeclaration(d) => Some(d),
                _ => None,
            })
            .flat_map(|d| d.declarations.iter())
            .filter(|d| d.span.size() <= HELPER_TEXT_MAX)
            .map(|d| {
                let mut ser = CompactSerializer::new(false, false);
                d.serialize(&mut ser);
                serde_json::json!({
                    "type": "VariableDeclaration",
                    "declarations": [crate::ingest::parse_estree_json(&ser.into_string())],
                })
            })
            .collect();
        self.lazy_init_helpers(&body)
    }
}

/// P8 — how a vendored module's references to the bundle's interop helpers
/// keep working in the runnable tree, end to end: which bundle bindings
/// ARE interop helpers (by shape — the unpack's scope plan,
/// `unpack::bun::scope`, spec I16), the standard names a vendored body's
/// references are rewritten to (and whose prior use in the bundle keeps a
/// factory in the app, spec I17), the helper file the finish writes beside
/// the vendor files and binds those names from, and the module helper
/// every vendored factory is wrapped in. ONE implementation: Bun's
/// (`__commonJS`, `__esm`, `__toESM`, `__toCommonJS`), which esbuild's
/// factories also run under — no part of the esbuild adapter differs here
/// today (esbuild's `__toCommonJS` does not match Bun's shape and falls to
/// the bridge, spec I16). A second implementation is where a new
/// bundler's helpers land.
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

    /// The helper file's text (written at [`InteropHelpers::runtime_file`]).
    pub fn relink_runtime(self) -> &'static str {
        match self {
            InteropHelpers::Bun => crate::finish::relink::BUN_RELINK_RUNTIME,
        }
    }

    /// The helper file's path in the output tree (`.humanify/__bun-runtime.js`).
    pub fn runtime_file(self) -> String {
        match self {
            InteropHelpers::Bun => format!(
                "{}/{}",
                crate::place::layout::METADATA_DIR,
                crate::finish::relink::BUN_RELINK_RUNTIME_FILE
            ),
        }
    }

    /// The helper every vendored factory is wrapped in (`exports.f =
    /// __commonJS(F)`), bound from the helper file.
    pub fn module_helper(self) -> &'static str {
        match self {
            InteropHelpers::Bun => "__commonJS",
        }
    }

    /// The standard names of the interop helpers a vendored body may call:
    /// the unpack rewrites a recognised helper reference to one of these,
    /// and the relink binds each one a body names free from the helper
    /// file. A bundle that already uses one of these names keeps the
    /// helpers' callers in the app (spec I17).
    pub fn canonical_names(self) -> &'static [&'static str] {
        match self {
            InteropHelpers::Bun => &crate::unpack::bun::scope::BUN_INTEROP_NAMES,
        }
    }

    /// Is `init` (the initializer of a never-written bundle binding) one
    /// of this bundler's interop helpers? Its standard name, by shape
    /// (spec I16).
    pub fn recognise(
        self,
        code: &str,
        init: &oxc_ast::ast::Expression<'_>,
    ) -> Option<&'static str> {
        match self {
            InteropHelpers::Bun => crate::unpack::bun::scope::bun_helper_shape(code, init),
        }
    }

    /// The bundler's other runtime helpers, by shape (library discovery's
    /// externals): esbuild's `__name`, `__publicField`, `__require`. Not
    /// [`Self::recognise`]'s helpers — nothing is rewritten to these.
    pub fn runtime_shape(
        self,
        code: &str,
        init: &oxc_ast::ast::Expression<'_>,
    ) -> Option<&'static str> {
        match self {
            InteropHelpers::Bun => crate::modules::helper_shape::esbuild_runtime_shape(code, init),
        }
    }

    /// The helpers that FILL a namespace object among a container's
    /// statements — a call defines its first argument (library discovery,
    /// docs/design/library-discovery.md §4): the export registrar
    /// (`__export`, by shape — the load order's owner,
    /// `emit::load_order::identify_export_registrar`) and the re-export
    /// copier (`__reExport`, by shape) and esbuild's `__name` (it sets its
    /// first argument's `name`).
    pub fn namespace_fillers(
        self,
        code: &str,
        statements: &[oxc_ast::ast::Statement<'_>],
    ) -> HashSet<String> {
        match self {
            InteropHelpers::Bun => {
                let mut out: HashSet<String> =
                    crate::emit::load_order::identify_export_registrar(statements)
                        .into_iter()
                        .collect();
                for s in statements {
                    let oxc_ast::ast::Statement::VariableDeclaration(d) = s else {
                        continue;
                    };
                    for d in &d.declarations {
                        if let (oxc_ast::ast::BindingPattern::BindingIdentifier(id), Some(init)) =
                            (&d.id, &d.init)
                            && (crate::modules::helper_shape::reexport_shape(code, init)
                                || crate::modules::helper_shape::esbuild_runtime_shape(code, init)
                                    == Some(crate::modules::helper_shape::NAME_SETTER))
                        {
                            out.insert(id.name.to_string());
                        }
                    }
                }
                out
            }
        }
    }
}

/// P9 — where the bundle's top-level statements are. ONE implementation:
/// the whole program inside a single wrapper function declaring at least
/// 50 names (Bun's CommonJS bytecode wrapper, esbuild's `--format=iife`),
/// whose grammar lives in `modules::wrapper`. Every pipeline reader asks
/// the run's value (the match sides, the graph's module scope, the twins'
/// statement inventory, the naming freezes, the family permute, the
/// unpack's classification, the split and its runnable emit, the finish's
/// bundle carry — review R1/R9/R5, 2026-10-04); a second layout (an ES
/// module's own top level) is a second variant here.
///
/// exp105 added webpack's: [`BundleLayout::WebpackBootstrap`], the
/// bootstrap IIFE that holds the module table and the runtime, whatever
/// its binding count (webpack's declares a handful).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleLayout {
    SingleWrapperFunction,
    WebpackBootstrap,
}

impl BundleLayout {
    pub fn name(self) -> &'static str {
        match self {
            BundleLayout::SingleWrapperFunction => "single-wrapper-function",
            BundleLayout::WebpackBootstrap => "webpack-bootstrap",
        }
    }

    /// The container holding the bundle's top-level statements, with the
    /// layout's "is this really a bundled app?" gate measured on the text
    /// `program` came from (≥50 names for the wrapper). None: the text has
    /// no container under this layout — readers fall back to the program
    /// scope / body.
    pub fn find_wrapper(
        self,
        program: &oxc_ast::ast::Program<'_>,
        semantic: &oxc_semantic::Semantic<'_>,
    ) -> Option<crate::modules::wrapper::WrapperFunction> {
        match self {
            BundleLayout::SingleWrapperFunction => {
                crate::modules::wrapper::find_wrapper_function(program, semantic)
            }
            BundleLayout::WebpackBootstrap => {
                crate::modules::wrapper::find_webpack_bootstrap(program, semantic)
                    .or_else(|| crate::modules::wrapper::find_wrapper_function(program, semantic))
            }
        }
    }

    /// The container by SHAPE only, no gate — for a text whose run already
    /// settled the gate on its ORIGINAL input
    /// ([`Self::original_bundle_binding_count`]): the split's
    /// post-extraction runtime and the finish's bundle.
    pub fn recognize_wrapper(
        self,
        program: &oxc_ast::ast::Program<'_>,
        semantic: &oxc_semantic::Semantic<'_>,
    ) -> Option<crate::modules::wrapper::WrapperFunction> {
        match self {
            BundleLayout::SingleWrapperFunction | BundleLayout::WebpackBootstrap => {
                crate::modules::wrapper::recognize_wrapper_function(program, semantic)
            }
        }
    }

    /// The run's input-bundle gate on the ORIGINAL text (what the unpack
    /// saw): Ok(the container's binding count) when it is a bundled app
    /// under this layout, else why not.
    pub fn original_bundle_binding_count(self, original: &str) -> Result<usize, String> {
        match self {
            BundleLayout::SingleWrapperFunction | BundleLayout::WebpackBootstrap => {
                crate::modules::wrapper::original_bundle_binding_count(original)
            }
        }
    }

    /// The wrapper's parameters in the bundle's entry context, BY
    /// POSITION (review R5): Node's/Bun's CommonJS wrapper
    /// `(exports, require, module, __filename, __dirname)` — the
    /// [`COMMONJS_CONTEXT`]. The runnable emit binds each named parameter
    /// to the shared context's property of that `role`.
    pub fn wrapper_parameter_roles(self) -> &'static [ContextName] {
        match self {
            BundleLayout::SingleWrapperFunction => &COMMONJS_CONTEXT,
            // the bootstrap IIFE takes no parameters
            BundleLayout::WebpackBootstrap => &[],
        }
    }
}

/// One name the module system hands a module's code, and its role in the
/// runnable tree's shared entry context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextName {
    /// The binding the code sees (`__filename`).
    pub name: &'static str,
    /// The shared context's property for it (`filename`).
    pub role: &'static str,
}

/// What Node's CommonJS module wrapper hands every module, in its
/// parameter order: `(exports, require, module, __filename, __dirname)`.
/// THE list (toolchain review R21 — it used to be five): the single
/// wrapper layout's parameters ([`BundleLayout::wrapper_parameter_roles`]),
/// the never-rename set (`rename::eligibility`), the unpack's "resolved in
/// the vendor file anyway" (`unpack::bun::scope`, a vendor file runs under
/// Node's wrapper), the proximity window's always-kept names
/// (`rename::votes::proximity`) and the known-globals report
/// (`modules::known_globals`, its `commonjs` environment). An ES module has
/// none of them (it has `import.meta`); never-renaming them there is the
/// safe direction. NOT read by the match fingerprint's vocabulary
/// (`matching::features::KNOWN_GLOBALS`): that is a frozen hash input that
/// names `require`, `__dirname` and `__filename` among ~100 globals, and
/// changing it changes match keys.
pub const COMMONJS_CONTEXT: [ContextName; 5] = [
    ContextName {
        name: "exports",
        role: "exports",
    },
    ContextName {
        name: "require",
        role: "require",
    },
    ContextName {
        name: "module",
        role: "module",
    },
    ContextName {
        name: "__filename",
        role: "filename",
    },
    ContextName {
        name: "__dirname",
        role: "dirname",
    },
];

/// [`COMMONJS_CONTEXT`]'s names, for the readers that hold plain name
/// lists.
pub const COMMONJS_CONTEXT_NAMES: [&str; 5] = {
    let mut names = [""; 5];
    let mut i = 0;
    while i < COMMONJS_CONTEXT.len() {
        names[i] = COMMONJS_CONTEXT[i].name;
        i += 1;
    }
    names
};

/// Is `name` one the CommonJS module system hands a module?
pub fn is_commonjs_context_name(name: &str) -> bool {
    COMMONJS_CONTEXT_NAMES.contains(&name)
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

/// P17 — what the bundler KEPT about each vendored module that says which
/// library it is (finding #95): its legal comments, the paths it wrote,
/// bundle-level legal blocks. One implementation per bundler
/// (`modules::identity::provenance`); the package readers that interpret
/// what it hands over are bundler-independent (`modules::identity::
/// readers`) and never see which bundler this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleIdentitySource {
    /// `/*!` comments kept in place; absolute paths inlined for
    /// `__dirname`, `__filename`, `require.resolve`, `import.meta.url`.
    Bun,
    /// The source path as each wrapper's key (unminified builds); legal
    /// comments in one end-of-file block keyed by `package/file`.
    Esbuild,
}

impl ModuleIdentitySource {
    pub fn name(self) -> &'static str {
        match self {
            ModuleIdentitySource::Bun => "bun",
            ModuleIdentitySource::Esbuild => "esbuild",
        }
    }

    /// What the bundler kept about the whole bundle (read once per run).
    pub fn bundle_facts(self, code: &str) -> crate::modules::identity::BundleFacts {
        match self {
            ModuleIdentitySource::Bun => crate::modules::identity::BundleFacts::default(),
            ModuleIdentitySource::Esbuild => {
                crate::modules::identity::provenance::esbuild_bundle_facts(code)
            }
        }
    }

    /// What the bundler kept about one classified module (`code` is the
    /// RAW bundle the classification ran on — comments still in it).
    pub fn module_provenance(
        self,
        code: &str,
        record: &crate::modules::FactoryRecord,
        bundle: &crate::modules::identity::BundleFacts,
    ) -> crate::modules::identity::ModuleProvenance {
        use crate::modules::identity::provenance;
        let kept = record.banner_text.as_deref();
        match self {
            ModuleIdentitySource::Bun => provenance::bun_module_provenance(
                &code[record.body_span.start as usize..record.body_span.end as usize],
                kept,
            ),
            ModuleIdentitySource::Esbuild => {
                provenance::esbuild_module_provenance(kept, record.source_path.as_deref(), bundle)
            }
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
    pub module_identity: Chosen<ModuleIdentitySource>,
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
    let detection = &definitive_only(detection);
    let settled = settle_conflict(detection);
    let mut toolchain = resolve_pieces(detection, bundler_override, minifier_override);
    for reason in [
        &mut toolchain.unpack.reason,
        &mut toolchain.library_detector.reason,
        &mut toolchain.never_rename.reason,
        &mut toolchain.name_profile.reason,
        &mut toolchain.tuning.reason,
        &mut toolchain.module_identity.reason,
        &mut toolchain.module_wrappers.reason,
        &mut toolchain.layout.reason,
    ] {
        *reason = settled(*reason);
    }
    toolchain
}

/// The verdict the toolchain acts on: a DEFINITIVE bundler verdict, else
/// `unknown`. A `likely` verdict (a weak token alone, e.g.
/// `installedModules` — review R19) is reported by detection and never
/// picks a piece. The conflict, if any, is kept.
fn definitive_only(detection: &BundlerDetectionResult) -> BundlerDetectionResult {
    let mut acted = detection.clone();
    if acted.bundler.tier != DetectionTier::Definitive {
        acted.bundler.kind = BundlerType::Unknown;
        acted.bundler.tier = DetectionTier::Unknown;
    }
    acted
}

/// How a bundler-decided piece's reason reads when detection had to settle
/// a conflict between bundlers (review R3): `detected` becomes
/// `detected-by-strength`, and a fallback forced by a tie becomes
/// `fallback-on-tie`. A flag, or a verdict without a conflict, reads as
/// before.
fn settle_conflict(detection: &BundlerDetectionResult) -> impl Fn(Reason) -> Reason {
    let resolution = detection.bundler.conflict.as_ref().map(|c| c.resolution);
    move |reason| match (resolution, reason) {
        (Some(ConflictResolution::Strength), Reason::Detected) => Reason::DetectedByStrength,
        (Some(ConflictResolution::Tie), Reason::Fallback) => Reason::FallbackOnTie,
        _ => reason,
    }
}

/// Every piece from the (definitive-only) verdict and the flags.
fn resolve_pieces(
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
    // What the bundler kept about each module: the bundler's own reader; an
    // unsure input gets Bun's (the fallback rule every piece follows).
    let module_identity = match bundler {
        BundlerType::Esbuild => Chosen {
            piece: ModuleIdentitySource::Esbuild,
            reason: bundler_reason,
        },
        BundlerType::Bun => Chosen {
            piece: ModuleIdentitySource::Bun,
            reason: bundler_reason,
        },
        _ => Chosen {
            piece: ModuleIdentitySource::Bun,
            reason: Reason::Fallback,
        },
    };
    Toolchain {
        module_identity,
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
        module_wrappers: match bundler {
            BundlerType::Webpack => Chosen {
                piece: ModuleWrapperGrammar::Webpack,
                reason: bundler_reason,
            },
            BundlerType::Rollup => Chosen {
                piece: ModuleWrapperGrammar::Rollup,
                reason: bundler_reason,
            },
            _ => only(ModuleWrapperGrammar::BunAndEsbuild),
        },
        interop: only(InteropHelpers::Bun),
        layout: match bundler {
            BundlerType::Webpack => Chosen {
                piece: BundleLayout::WebpackBootstrap,
                reason: bundler_reason,
            },
            _ => only(BundleLayout::SingleWrapperFunction),
        },
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
                "moduleIdentity",
                self.module_identity.piece.name(),
                self.module_identity.reason,
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
