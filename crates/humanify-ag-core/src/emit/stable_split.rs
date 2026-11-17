//! The split stage as the pipeline runs it — TS `stableSplitFromCode`
//! (src/split/stable-split.ts) + `tryEmitRunnableCjs` (unified.ts): ONE
//! owner of the order parse → statement hashes → placement regime →
//! emission alignment → review tree → ledger → runnable emit. The
//! pipeline (`humanify-ag <input> --split`) calls [`stable_split`].
//!
//! The statement hashes are the Rust's own (`hash::statement_hash`,
//! `hashVersion` = the [`STATEMENT`] form's version); the TS-byte injection that
//! bridged the migration ended at WP5.6e (00-control §3).

use oxc_allocator::Allocator;

use crate::hash::forms::STATEMENT;
use crate::ingest::Ingest;
use crate::modules::wrapper::InputGateError;
use crate::place::assign::namer::{SplitNamer, TreeReviser};
use crate::place::declared::declared_names;
use crate::place::input::{SplitInput, SplitInputError, split_input_with_original_bundle};
use crate::place::ledger::{SplitLedger, StableSplitLedger};
use crate::place::method::{MarkerOffer, SplitChoice, bundle_marker_coverage, choose_split_method};
use crate::place::placement_dump::{PlaceError, Placed, PlacementGate, Regime, assign_regime};
use crate::place::tiers::{PlacementSwitches, PriorCarry, TierStats};
use crate::place::trail::PlacementTrail;
use crate::rename::validated::scopes::BabelScopes;

use super::align::AlignSwitches;
use super::cjs::{
    EmitError, RunnableInput, WrapperView, emit_runnable_cjs, load_graph, wrapper_view,
};
use super::cycle_guard::{CycleRepair, break_load_time_cycles};
use super::entry_order::read_prior_entry_order;
use super::import_alias::read_prior_import_aliases;
use super::load_order::{LoadOrderFacts, bundle_load_order_facts};
use super::review::{ReconstructError, review_split, statement_align_name};
use crate::finish::relink::VendorBridge;
use crate::modules::vendor_names::ManifestCapture;
use crate::twins::fossil::declared_names as fossil_declared_names;
use serde_json::Value;

/// What the split is asked to do (`StableSplitOptions` + the runnable
/// emit's switches).
pub struct SplitOptions<'a, 'n> {
    /// Whether the run's toolchain offers the module markers (P11). The
    /// split method itself is chosen HERE, from what the bundle contains
    /// ([`crate::place::method::choose_split_method`]): the markers only
    /// when they describe the bundle, else the prior's layout or the
    /// fresh grouping.
    pub markers: MarkerOffer,
    pub prior: Option<&'a StableSplitLedger>,
    /// The prior TREE's files (`--prior-version`'s tree): each importer's
    /// require aliases are carried from its own prior file, and the entry
    /// order from the prior's entry file.
    pub prior_tree: Option<&'a crate::prior_tree::PriorFiles>,
    /// The tiers regime's carry (`renameResult.priorCarry`).
    pub carry: Option<PriorCarry>,
    /// The fresh regime's file namer — handed over whatever the method;
    /// the chosen method decides whether it is asked ([`namers_used`]).
    pub namer: Option<&'n mut dyn SplitNamer>,
    /// The fresh regime's holistic top-level reviser (same rule).
    pub reviser: Option<&'n mut dyn TreeReviser>,
    pub placement: PlacementSwitches,
    pub align: AlignSwitches,
    pub registrar_exemption_disabled: bool,
    /// `--split-pure`: the review tree, never the runnable emit.
    pub split_pure: bool,
    /// The placement trail (`--diagnostics`).
    pub trail: Option<&'a mut PlacementTrail>,
    /// The unpack's vendor capture records (finding #60), read back from
    /// the manifest: each names the raw binding a vendored body READS. The
    /// split resolves each against the FRESH text (below) and its own
    /// (renamed) statements and placement.
    pub vendor_captures: &'a [ManifestCapture],
    /// The FRESH text — the naming stage's input (the beautified,
    /// pre-rename runtime), whose wrapper-body statements align 1:1 with
    /// the shipped list renames only substitute names in. Required when
    /// `vendor_captures` is non-empty.
    pub vendor_fresh: Option<&'a str>,
    /// Where the ≥50 wrapper-binding gate ("is this really a bundled
    /// app?") is read — see [`InputGate`]. The wrapper GRAMMAR always
    /// reads the shipped text.
    pub input_gate: InputGate<'a>,
    /// The run's bundle layout (the toolchain's P9 piece): the input
    /// gate, the wrapper body every text here is sliced from, and the
    /// wrapper parameters' roles in the runnable emit.
    pub layout: crate::toolchain::BundleLayout,
    /// The run's module wrapper grammar (the toolchain's P3 piece): the
    /// module helper the fresh grouping's vendor bucket takes (review R7).
    pub module_wrappers: crate::toolchain::ModuleWrapperGrammar,
    /// The names mode's placed modules (library-names plan step 4,
    /// exp156): `(module index, vendor path)` per module the naming
    /// stage's layout moved — the fossil split pre-fills those paths
    /// before any match or fresh name. The indices are the era's own
    /// extraction's, which the wrapper-body 1:1 alignment makes valid
    /// here. Non-empty with a non-fossil regime is refused (below):
    /// the layout's model exclusion assumed the module split.
    pub library: &'a [(usize, String)],
    /// The factories the unpack kept in the app as the app's OWN CommonJS
    /// modules (exp172, `modules::app_factories`), by RAW name: resolved
    /// against the FRESH text like the capture records, their statements
    /// are grouped with the app instead of vendored.
    pub app_modules: &'a [String],
}

/// Where the split's ≥50 wrapper-binding gate is read. Being a bundled
/// app is a property of the run's INPUT (2026-10-02): the post-extraction
/// runtime this split is handed has lost one wrapper-scope binding per
/// vendored module, so a mid-size app whose vendor half dominates lands
/// UNDER the threshold on it and still splits on the strength of its
/// input.
#[derive(Clone, Copy, Debug)]
pub enum InputGate<'a> {
    /// The historical gate on the shipped text itself (the standalone
    /// owners and the tests).
    TextAtHand,
    /// The run's ORIGINAL input bundle (the text the unpack stage saw,
    /// before its vendor extraction spliced the CJS factories out).
    Original(&'a str),
    /// Already settled on the original by the caller — its container's
    /// binding count ([`crate::toolchain::BundleLayout::original_bundle_binding_count`]),
    /// so the original is parsed once per run.
    Settled(usize),
}

/// Which of the split's namers the chosen method asks: the marker method
/// none (its files take the module names the naming stage gave the
/// wrappers, docs/design/module-naming.md), the prior's layout none, the
/// fresh grouping names files and folders and revises the top level.
pub fn namers_used(regime: Regime) -> (bool, bool) {
    match regime {
        Regime::Fossil | Regime::Tiers => (false, false),
        Regime::Cluster => (true, true),
    }
}

/// The module markers' boundaries the fresh grouping keeps as file ends:
/// when the toolchain offers the markers but they do not describe the
/// whole bundle (the mixed case), each lazily loaded module still ENDS a
/// file — the grouping decides everything else. Without it, a small mixed
/// app's lazy module shared a file with the code that loads its CommonJS
/// requirer: a load-time cycle through vendor/, and no runnable tree
/// (the `bun-bundle` fixture). None for the other methods.
fn module_ends_kept(choice: &SplitChoice, markers: MarkerOffer, body: &[Value]) -> Vec<usize> {
    if choice.regime == Regime::Cluster && markers == MarkerOffer::Offered {
        crate::twins::fossil::lazy_init_indices(body)
    } else {
        Vec::new()
    }
}

/// `StableSplitStats`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SplitStats {
    pub statements: usize,
    pub files: usize,
    pub folders: usize,
    pub tiers: TierStats,
}

/// What the split stage hands on.
pub struct SplitOutcome {
    /// The split method the bundle got, and why.
    pub method: SplitChoice,
    /// What `writeSplitTree` writes: the runnable tree, or the review tree
    /// when `--split-pure` or the runnable emit declined.
    pub files: Vec<(String, String)>,
    /// The runnable emit's file map keys (the finishing stage's split-file
    /// list); None when the review tree was written.
    pub runnable: Option<Vec<String>>,
    /// The statements the load-time cycle guard moved: (statement, from
    /// file, to file).
    pub cycle_moves: Vec<(usize, String, String)>,
    /// The runnable emit's decline reason.
    pub declined: Option<super::cjs::EmitError>,
    /// The persisted ledger.
    pub ledger: SplitLedger,
    pub stats: SplitStats,
    /// The placement regime's summary line.
    pub summary: String,
    /// The emitted layout per file (the dump's `emit.json`): the runnable
    /// tree's when it won, else the review tree's.
    pub layout: Vec<(String, Vec<usize>)>,
    /// (importer, module, alias) for every require the runnable tree
    /// writes (runnable only).
    pub aliases: Vec<(String, String, String)>,
    /// Per statement: span in the shipped text.
    pub spans: Vec<(u32, u32)>,
    /// Where each runnable file's statements sit (empty: a review tree).
    pub lines: Vec<super::cjs::FileLines>,
    pub facts: Vec<LoadOrderFacts>,
    /// The vendor captures RESOLVED to (raw name, owner file, live
    /// accessor name) — what the finishing stage's vendor bridge splices
    /// (finding #60). Empty for review trees and declines.
    pub vendor_bridges: Vec<VendorBridge>,
}

/// `buildLedger(body, assignment, files, hashes, emitHashes, emitNames)`:
/// identity (`nameToFiles`) from the BUNDLE-ordered body, layout from the
/// emitted hashes.
fn build_ledger(
    input: &SplitInput,
    assignment: &[String],
    files: &[String],
    emit_hashes: &[String],
    emit_names: &[Option<String>],
) -> SplitLedger {
    let mut name_to_files: Vec<(String, Vec<String>)> = Vec::new();
    let mut slot_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, stmt) in input.body.iter().enumerate() {
        for n in declared_names(stmt) {
            let slot = *slot_of.entry(n.clone()).or_insert_with(|| {
                name_to_files.push((n, Vec::new()));
                name_to_files.len() - 1
            });
            name_to_files[slot].1.push(assignment[i].clone());
        }
    }
    SplitLedger {
        version: 1,
        files: files.to_vec(),
        // Declared names are identifiers, not array-index keys, so the
        // record keeps declaration order.
        name_to_files: name_to_files.into_iter().collect(),
        order: assignment.to_vec(),
        hashes: input.hashes.clone(),
        emit_hashes: emit_hashes.to_vec(),
        emit_names: emit_names.to_vec(),
        hash_version: STATEMENT.version,
        fossil_modules: None,
        emit_indexes: None,
    }
}

/// Why the stable split failed (before anything is written).
#[derive(Debug, thiserror::Error)]
pub enum SplitError {
    /// The run's input is not a bundled app.
    #[error(transparent)]
    InputGate(#[from] InputGateError),
    /// The shipped or fresh text gives no split input.
    #[error(transparent)]
    Input(#[from] SplitInputError),
    /// A library layout under a regime other than the fossil split.
    #[error(
        "library placement requires the fossil split, but this bundle chose {regime:?} (check the fossil-split disable, and the markers' coverage)"
    )]
    LibraryNeedsFossil { regime: Regime },
    /// The placement regime failed.
    #[error(transparent)]
    Place(#[from] PlaceError),
    /// The review tree does not reconstruct the bundle.
    #[error(transparent)]
    Reconstruct(#[from] ReconstructError),
    /// A step that holds by construction did not.
    #[error("{0}")]
    Invariant(&'static str),
    /// The fresh and shipped texts hold different statement counts.
    #[error(
        "{what}: the fresh text has {fresh} wrapper statements, the shipped code has {shipped} — they no longer align"
    )]
    Misaligned {
        what: &'static str,
        fresh: usize,
        shipped: usize,
    },
    /// A captured raw name is declared by zero or several fresh statements.
    #[error(
        "vendor bridge: the app-scope read of '{name}' matches {hits:?} fresh statements — expected exactly one"
    )]
    BridgeAmbiguous {
        name: String,
        hits: Vec<(usize, usize)>,
    },
    /// A statement declares different name counts fresh and shipped.
    #[error(
        "vendor bridge: statement {statement} declares {fresh} names fresh but {shipped} shipped"
    )]
    BridgeNameCount {
        statement: usize,
        fresh: usize,
        shipped: usize,
    },
    /// The captured name's slot is gone from the shipped statement.
    #[error("vendor bridge: statement {statement} lost the slot of '{name}' across the rename")]
    BridgeLostSlot { statement: usize, name: String },
    /// The captured name's statement has no file.
    #[error("vendor bridge: statement {statement} (the home of '{name}') has no placement")]
    BridgeUnplaced { statement: usize, name: String },
}

/// `stableSplitFromCode` + `tryEmitRunnableCjs` over the shipped text.
pub fn stable_split(
    shipped: &str,
    options: SplitOptions<'_, '_>,
) -> Result<SplitOutcome, SplitError> {
    use crate::profiling::phase;
    let ph = phase("split:input");
    // The input-bundle gate, measured ONCE: being a bundled app is a
    // property of the run's INPUT (the text the unpack stage saw), so the
    // frozen ≥50 threshold reads the ORIGINAL — the post-extraction
    // runtime below has lost one wrapper-scope binding per vendored
    // module, and a mid-size app whose vendor half dominates is expected
    // to land under the threshold on it. Both the shipped and the fresh
    // (pre-rename) texts consume the same verdict.
    let layout = options.layout;
    let original_binding_count = match options.input_gate {
        InputGate::TextAtHand => None,
        InputGate::Original(original) => Some(layout.original_bundle_binding_count(original)?),
        InputGate::Settled(count) => Some(count),
    };
    let input = split_input_with_original_bundle(shipped, original_binding_count, layout)?;
    // The split method, from what the bundle CONTAINS (C1/C2): the markers'
    // coverage of the app code, module factories set aside.
    let coverage = bundle_marker_coverage(&input, shipped, options.module_wrappers);
    let method = choose_split_method(options.markers, options.prior.is_some(), Some(coverage));
    // A library layout only means something under the module split: the
    // naming era excluded the moved modules from the model on the premise
    // that the fossil split would place them. Any other regime here is a
    // half-state (the markers stopped describing the bundle between the
    // two stages, or a fossil-split disable) — refuse it rather than
    // move library code without its exclusion.
    if !options.library.is_empty() && method.regime != Regime::Fossil {
        return Err(SplitError::LibraryNeedsFossil {
            regime: method.regime,
        });
    }
    let (use_namer, use_reviser) = namers_used(method.regime);
    let module_ends = module_ends_kept(&method, options.markers, &input.body);
    drop(ph);
    let ph = phase("split:assign");
    let mut own_trail = PlacementTrail::default();
    let trail = options.trail.unwrap_or(&mut own_trail);
    let Placed {
        assignment,
        summary,
        fossil_modules,
        tier_stats,
    } = assign_regime(
        &input,
        shipped,
        PlacementGate {
            regime: method.regime,
            carry: options.carry,
            switches: options.placement,
            namer: options.namer.filter(|_| use_namer),
            reviser: options.reviser.filter(|_| use_reviser),
            module_wrappers: options.module_wrappers,
            module_ends,
            library: options.library,
            app_factories: app_factory_statements(
                options.app_modules,
                options.vendor_fresh,
                input.body.len(),
                original_binding_count,
                layout,
            )?,
        },
        options.prior,
        trail,
    )?;

    drop(ph);
    let ph = phase("split:review");
    // The typed parse the emit walks (same text, same spans).
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, shipped, "shipped.js");
    // Grammar only: the ≥50 gate was already settled — on this text (the
    // historical split_input path) or on the run's ORIGINAL input — by
    // split_input_with_original_bundle above, over the same bytes.
    let wrapper = layout
        .recognize_wrapper(ingest.program, ingest.semantic())
        .ok_or(SplitError::Invariant("no recognizable bundle wrapper"))?;
    let view = wrapper_view(ingest.semantic(), wrapper.span)
        .ok_or(SplitError::Invariant("wrapper node not found"))?;
    let statements = &view.body.statements;
    let typed_spans: Vec<(u32, u32)> = statements
        .iter()
        .map(|s| {
            let sp = oxc_span::GetSpan::span(s);
            (sp.start, sp.end)
        })
        .collect();
    if typed_spans != input.spans {
        return Err(SplitError::Invariant(
            "the typed wrapper body and the split input disagree on spans",
        ));
    }
    let names: Vec<Option<String>> = input
        .body
        .iter()
        .map(|s| statement_align_name(declared_names(s)))
        .collect();
    let lazy_init = options.module_wrappers.lazy_init_helpers(&input.body);
    let facts =
        bundle_load_order_facts(statements, &lazy_init, options.registrar_exemption_disabled);
    let scopes = (!options.split_pure).then(|| BabelScopes::build(ingest.semantic()));
    let prior_entry = options
        .prior_tree
        .and_then(|tree| read_prior_entry_order(|f| tree.text(f).map(str::to_string)));
    let ctx = WriteContext {
        shipped,
        input: &input,
        names: &names,
        facts: &facts,
        prior: options.prior,
        prior_tree: options.prior_tree,
        align: options.align,
        vendor_captures: options.vendor_captures,
        vendor_fresh: options.vendor_fresh,
        layout,
        original_binding_count,
        semantic: ingest.semantic(),
        view: &view,
        scopes: scopes.as_ref(),
        prior_entry: prior_entry.as_deref(),
    };
    drop(ph);
    let mut written = write_split(&ctx, &assignment)?;
    let guarded = guard_load_time_cycles(&ctx, &assignment, &written)?;
    let cycle_moves = match guarded {
        Some((repair, rewritten)) => {
            for (s, _, to) in &repair.moved {
                trail.moved_by_cycle_guard(*s, to);
            }
            written = rewritten;
            repair.moved
        }
        None => Vec::new(),
    };
    let Written {
        review,
        mut ledger,
        stats,
        vendor_bridges,
        runnable,
    } = written;
    ledger.fossil_modules = fossil_modules;
    let stats = SplitStats {
        tiers: tier_stats,
        ..stats
    };
    // Only a runnable emit exports the accessors the bridges read; a
    // declined or pure-review tree keeps its vendor bodies raw either way
    // (the finish relinks nothing without a runnable list).
    let vendor_bridges = match &runnable {
        Some(Ok(_)) => vendor_bridges,
        _ => Vec::new(),
    };
    let mut outcome = SplitOutcome {
        method,
        files: Vec::new(),
        runnable: None,
        declined: None,
        ledger: SplitLedger::default(),
        stats,
        summary,
        layout: Vec::new(),
        aliases: Vec::new(),
        spans: input.spans.clone(),
        lines: Vec::new(),
        facts: Vec::new(),
        vendor_bridges,
        cycle_moves,
    };
    match runnable {
        Some(Ok(tree)) => {
            // `recordEmittedLayout` returns early under `--disable
            // emit-align`: the review layout stays, no emitIndexes.
            if !options.align.emit_align_disabled {
                ledger.emit_hashes = tree.emit_hashes.clone();
                ledger.emit_names = tree.emit_names.clone();
                ledger.emit_indexes = Some(tree.emit_indexes.clone());
            }
            outcome.runnable = Some(tree.files.iter().map(|(p, _)| p.clone()).collect());
            outcome.files = tree.files;
            outcome.lines = tree.lines;
            // The dump's emit.json is `captureRunnableEmitLayout`'s, which
            // runs inside `recordEmittedLayout` — past its `--disable
            // emit-align` return, so the review capture stays then.
            if options.align.emit_align_disabled {
                outcome.layout = review.layout;
            } else {
                outcome.layout = tree.layout;
                outcome.aliases = tree.aliases;
            }
        }
        Some(Err(decline)) => {
            // The byte-exact review tree is written — but the persisted
            // ledger keeps what the TS emit had already set on it when it
            // threw (finding #40): the emitted layout (and the dump's emit
            // capture) once the assembly had begun.
            outcome.layout = review.layout;
            if let Some(layout) = decline
                .layout
                .filter(|_| !options.align.emit_align_disabled)
            {
                // `captureRunnableEmitLayout` ran: the dump's emit.json is
                // the runnable layout.
                ledger.emit_hashes = layout.emit_hashes.clone();
                ledger.emit_names = layout.emit_names.clone();
                ledger.emit_indexes = Some(layout.emit_indexes.clone());
                outcome.layout = layout.by_file.clone();
            }
            outcome.declined = Some(decline.reason);
            outcome.files = review.contents;
        }
        None => {
            outcome.files = review.contents;
            outcome.layout = review.layout;
        }
    }
    outcome.ledger = ledger;
    outcome.facts = facts;
    Ok(outcome)
}

/// What every write of the split reads, whatever the assignment.
struct WriteContext<'c, 'a> {
    shipped: &'a str,
    input: &'c SplitInput,
    names: &'c [Option<String>],
    facts: &'c [LoadOrderFacts],
    prior: Option<&'c StableSplitLedger>,
    prior_tree: Option<&'c crate::prior_tree::PriorFiles>,
    align: AlignSwitches,
    vendor_captures: &'c [ManifestCapture],
    vendor_fresh: Option<&'c str>,
    layout: crate::toolchain::BundleLayout,
    original_binding_count: Option<usize>,
    semantic: &'c oxc_semantic::Semantic<'a>,
    view: &'c WrapperView<'a>,
    /// None under `--split-pure` (no runnable emit).
    scopes: Option<&'c BabelScopes>,
    prior_entry: Option<&'c [String]>,
}

/// One assignment written: the review tree, its ledger (no fossil
/// modules yet), its stats (no tier stats yet), the vendor bridges and
/// the runnable emit (None under `--split-pure`).
struct Written {
    review: super::review::ReviewSplit,
    ledger: SplitLedger,
    stats: SplitStats,
    vendor_bridges: Vec<VendorBridge>,
    runnable: Option<Result<super::cjs::RunnableTree, super::cjs::EmitDecline>>,
}

/// Review tree → ledger → vendor bridges → runnable emit, for one
/// assignment.
fn write_split(ctx: &WriteContext<'_, '_>, assignment: &[String]) -> Result<Written, SplitError> {
    use crate::profiling::phase;
    let input = ctx.input;
    let ph = phase("split:review");
    let review = review_split(
        ctx.shipped,
        &input.spans,
        assignment,
        &input.hashes,
        ctx.names,
        ctx.facts,
        ctx.prior,
        ctx.align,
    );
    let ledger = build_ledger(
        input,
        assignment,
        &review.files,
        &review.emit_hashes,
        &review.emit_names,
    );
    // `assertConcatEquivalence` (finding #41): the review tree must
    // reconstruct the bundle's statements — a pre-commit failure.
    super::review::assert_concat_equivalence(
        &review.contents,
        assignment,
        &input.spans,
        ctx.shipped,
    )?;
    let folders: std::collections::HashSet<&str> = review
        .files
        .iter()
        .map(|f| f.rfind('/').map_or("", |at| &f[..at]))
        .collect();
    let stats = SplitStats {
        statements: input.body.len(),
        files: review.files.len(),
        folders: folders.len(),
        tiers: TierStats::default(),
    };
    drop(ph);
    let _ph = phase("split:runnable-cjs");
    // Vendor bridges (finding #60): resolve the unpack's capture records —
    // raw names read by vendored bodies — against the FRESH text (the
    // beautified pre-rename runtime, whose wrapper statements align 1:1
    // with this split's renamed ones) and this split's placement. The
    // owner files must export the accessors whether or not any app file
    // references the binding, so the emit gets them as forced exports.
    let vendor_bridges = resolve_vendor_bridges(
        ctx.vendor_captures,
        ctx.vendor_fresh,
        &input.body,
        assignment,
        ctx.original_binding_count,
        ctx.layout,
    )?;
    let mut forced_exports: Vec<(String, String)> = vendor_bridges
        .iter()
        .map(|b| (b.file.clone(), b.accessor.clone()))
        .collect();
    forced_exports.sort();
    forced_exports.dedup();
    let runnable = ctx.scopes.map(|scopes| {
        let prior_aliases = ctx.prior_tree.map(|tree| {
            read_prior_import_aliases(&review.files, |f| tree.text(f).map(str::to_string))
        });
        emit_runnable_cjs(&runnable_input(
            ctx,
            scopes,
            assignment,
            &review,
            prior_aliases.as_ref(),
            &forced_exports,
        ))
    });
    Ok(Written {
        review,
        ledger,
        stats,
        vendor_bridges,
        runnable,
    })
}

/// The runnable emit's input for one written assignment.
fn runnable_input<'s, 'a>(
    ctx: &WriteContext<'s, 'a>,
    scopes: &'s BabelScopes,
    assignment: &'s [String],
    review: &'s super::review::ReviewSplit,
    prior_aliases: Option<&'s super::import_alias::PriorImportAliases>,
    forced_exports: &'s [(String, String)],
) -> RunnableInput<'a, 's> {
    RunnableInput {
        code: ctx.shipped,
        semantic: ctx.semantic,
        scopes,
        wrapper: ctx.view,
        files: &review.files,
        order: assignment,
        emit_hashes: &review.emit_hashes,
        emit_names: &review.emit_names,
        prior_aliases,
        prior_entry: ctx.prior_entry,
        bundle_hashes: &ctx.input.hashes,
        bundle_names: ctx.names,
        facts: ctx.facts,
        switches: ctx.align,
        forced_exports,
        layout: ctx.layout,
    }
}

/// The load-time cycle guard ([`super::cycle_guard`]): when the runnable
/// emit declined on a load-time cycle between files, move the fewest app
/// statements that break it and write the split again. None: no cycle,
/// or none the guard can break (the emit's decline stands, and the run
/// fails on it).
fn guard_load_time_cycles(
    ctx: &WriteContext<'_, '_>,
    assignment: &[String],
    written: &Written,
) -> Result<Option<(CycleRepair, Written)>, SplitError> {
    let (Some(scopes), Some(Err(decline))) = (ctx.scopes, &written.runnable) else {
        return Ok(None);
    };
    if !matches!(decline.reason, EmitError::Cycle(_)) {
        return Ok(None);
    }
    let input = runnable_input(ctx, scopes, assignment, &written.review, None, &[]);
    let Ok(graph) = load_graph(&input) else {
        return Ok(None);
    };
    let vendor = format!("{}/", crate::place::layout::VENDOR_DIR);
    let app_only =
        |_: usize, from: &str, to: &str| !from.starts_with(&vendor) && !to.starts_with(&vendor);
    let Some(repair) = break_load_time_cycles(assignment, &graph, &app_only) else {
        return Ok(None);
    };
    let rewritten = write_split(ctx, &repair.assignment)?;
    Ok(Some((repair, rewritten)))
}

/// The statements (body indexes) holding the factories the unpack kept as
/// the app's own modules (exp172), resolved like the capture records: the
/// FRESH text still declares the RAW names and aligns statement-for-
/// statement with the shipped one. A statement counts only when every
/// name it declares is an app module's (a statement that also declares a
/// kept library factory stays vendored). Nothing to resolve, or no fresh
/// text: none — the factories are vendored as before, never lost.
fn app_factory_statements(
    app_modules: &[String],
    fresh: Option<&str>,
    body_len: usize,
    original_binding_count: Option<usize>,
    layout: crate::toolchain::BundleLayout,
) -> Result<std::collections::HashSet<usize>, SplitError> {
    let (false, Some(fresh)) = (app_modules.is_empty(), fresh) else {
        return Ok(std::collections::HashSet::new());
    };
    let fresh_body = split_input_with_original_bundle(fresh, original_binding_count, layout)?.body;
    if fresh_body.len() != body_len {
        return Err(SplitError::Misaligned {
            what: "app modules",
            fresh: fresh_body.len(),
            shipped: body_len,
        });
    }
    let wanted: std::collections::HashSet<&str> = app_modules.iter().map(String::as_str).collect();
    Ok(fresh_body
        .iter()
        .enumerate()
        .filter(|(_, stmt)| {
            let names = fossil_declared_names(stmt);
            !names.is_empty() && names.iter().all(|n| wanted.contains(n.as_str()))
        })
        .map(|(si, _)| si)
        .collect())
}

/// Resolve the unpack's vendor capture records (finding #60) against the
/// FRESH text — the naming stage's input, so the captured RAW name is still
/// there — and this split's own (renamed) statements and placement. The two
/// statement lists align index-for-index (renaming substitutes names
/// only), and renames preserve declaration order within a statement, so
/// wherever the fresh statement declares the raw name, the shipped
/// statement declares the accessor at the SAME position. The placement
/// names the owner file.
///
/// Anything that does not resolve is a HARD error: an unresolved capture
/// would leave a free name in a vendor file, the exact hole finding #51
/// closed — better a failed split than a tree that lies.
fn resolve_vendor_bridges(
    captures: &[ManifestCapture],
    fresh: Option<&str>,
    body: &[Value],
    assignment: &[String],
    // The run's input-bundle gate verdict and bundle layout — the FRESH
    // text is the same post-extraction runtime (pre-rename), so it owes
    // the same GRAMMAR and reads the same ORIGINAL threshold.
    original_binding_count: Option<usize>,
    layout: crate::toolchain::BundleLayout,
) -> Result<Vec<VendorBridge>, SplitError> {
    if captures.is_empty() {
        return Ok(Vec::new());
    }
    let fresh = fresh.ok_or(SplitError::Invariant(
        "vendor bridge: the manifest carries captures but the run has no fresh text",
    ))?;
    let fresh_input = split_input_with_original_bundle(fresh, original_binding_count, layout)?;
    let fresh_body = &fresh_input.body;
    if fresh_body.len() != body.len() {
        return Err(SplitError::Misaligned {
            what: "vendor bridge",
            fresh: fresh_body.len(),
            shipped: body.len(),
        });
    }
    let mut out = Vec::with_capacity(captures.len());
    for c in captures {
        // The raw name must belong to exactly ONE fresh wrapper statement:
        // the wrapper is one scope, and its bindings are bundle-generated —
        // a second declaration is a shape the bridge refuses.
        let mut hits: Vec<(usize, usize)> = Vec::new();
        for (si, stmt) in fresh_body.iter().enumerate() {
            let names = fossil_declared_names(stmt);
            if let Some(pos) = names.iter().position(|n| n == &c.name) {
                hits.push((si, pos));
            }
        }
        let [(si, pos)] = hits[..] else {
            return Err(SplitError::BridgeAmbiguous {
                name: c.name.clone(),
                hits,
            });
        };
        let fresh_names = fossil_declared_names(&fresh_body[si]);
        let shipped_names = fossil_declared_names(&body[si]);
        if shipped_names.len() != fresh_names.len() {
            return Err(SplitError::BridgeNameCount {
                statement: si,
                fresh: fresh_names.len(),
                shipped: shipped_names.len(),
            });
        }
        let Some(accessor) = shipped_names.get(pos) else {
            return Err(SplitError::BridgeLostSlot {
                statement: si,
                name: c.name.clone(),
            });
        };
        let Some(file) = assignment.get(si) else {
            return Err(SplitError::BridgeUnplaced {
                statement: si,
                name: c.name.clone(),
            });
        };
        out.push(VendorBridge {
            name: c.name.clone(),
            file: file.clone(),
            accessor: accessor.clone(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod stable_split_test;
