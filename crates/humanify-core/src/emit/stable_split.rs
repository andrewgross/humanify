//! The split stage as the pipeline runs it — TS `stableSplitFromCode`
//! (src/split/stable-split.ts) + `tryEmitRunnableCjs` (unified.ts): ONE
//! owner of the order parse → statement hashes → placement regime →
//! emission alignment → review tree → ledger → runnable emit. The
//! pipeline (`humanify <input> --split`) and the WP5.3 gate verb
//! (`humanify emit`, [`super::emit_dump`]) both call [`stable_split`];
//! neither keeps its own copy.
//!
//! The one seam (lesson 16, 00-control §3): statement-hash BYTES differ
//! from the TS's by design. With [`SplitOptions::ts_hashes`] the TS's
//! `statementHash` partition is substituted after
//! [`inject_ts_statement_hashes`] PROVES the two partitions are one — the
//! blessed structuralSignature exemption, migration-only.

use humanify_model::dump::PartitionsFile;
use humanify_model::js::{JsObject, JsValue};
use oxc_allocator::Allocator;

use crate::ingest::Ingest;
use crate::modules::wrapper::find_wrapper_function;
use crate::place::assign::namer::{SplitNamer, TreeReviser};
use crate::place::declared::declared_names;
use crate::place::input::{SplitInput, split_input};
use crate::place::ledger::{FossilLedgerModule, StableSplitLedger};
use crate::place::placement_dump::{
    Placed, PlacementGate, Regime, assign_regime, inject_ts_statement_hashes,
};
use crate::place::tiers::{PlacementSwitches, PriorCarry, TierStats};
use crate::place::trail::PlacementTrail;
use crate::rename::validated::scopes::BabelScopes;

use super::align::AlignSwitches;
use super::cjs::{RunnableInput, emit_runnable_cjs, wrapper_view};
use super::load_order::{LoadOrderFacts, bundle_load_order_facts};
use super::review::{review_split, statement_align_name};

/// `STATEMENT_HASH_VERSION` (split/statement-hash.ts).
pub const STATEMENT_HASH_VERSION: u64 = 1;

/// What the split is asked to do (`StableSplitOptions` + the runnable
/// emit's switches).
pub struct SplitOptions<'a, 'n> {
    pub regime: Regime,
    pub prior: Option<&'a StableSplitLedger>,
    /// The tiers regime's carry (`renameResult.priorCarry`).
    pub carry: Option<PriorCarry>,
    /// The fossil regime's mint namer / the fresh regime's file namer.
    pub namer: Option<&'n mut dyn SplitNamer>,
    pub reviser: Option<&'n mut dyn TreeReviser>,
    /// The TS dump's partitions (the blessed hash-byte injection).
    pub ts_hashes: Option<&'a PartitionsFile>,
    pub placement: PlacementSwitches,
    pub align: AlignSwitches,
    pub registrar_exemption_disabled: bool,
    /// `--split-pure`: the review tree, never the runnable emit.
    pub split_pure: bool,
    /// The placement trail (`--diagnostics`).
    pub trail: Option<&'a mut PlacementTrail>,
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
    /// What `writeSplitTree` writes: the runnable tree, or the review tree
    /// when `--split-pure` or the runnable emit declined.
    pub files: Vec<(String, String)>,
    /// The runnable emit's file map keys (the finishing stage's split-file
    /// list); None when the review tree was written.
    pub runnable: Option<Vec<String>>,
    /// The runnable emit's decline reason.
    pub declined: Option<String>,
    /// The persisted ledger, in the TS's key order (`buildLedger`, then
    /// `fossilModules`, then the runnable emit's `aliases` / layout).
    pub ledger: JsValue,
    pub stats: SplitStats,
    /// The placement regime's summary line.
    pub summary: String,
    /// The emitted layout per file (the dump's `emit.json`): the runnable
    /// tree's when it won, else the review tree's.
    pub layout: Vec<(String, Vec<usize>)>,
    /// file → alias (runnable only).
    pub aliases: Vec<(String, String)>,
    /// Per statement: span in the shipped text, hash (possibly injected).
    pub spans: Vec<(u32, u32)>,
    pub facts: Vec<LoadOrderFacts>,
    /// (statements, classes) proven bijective when the TS bytes were
    /// injected.
    pub injected: Option<(usize, usize)>,
}

fn str_list(items: &[String]) -> JsValue {
    JsValue::Array(items.iter().map(|s| JsValue::str(s.as_str())).collect())
}

fn opt_str_list(items: &[Option<String>]) -> JsValue {
    JsValue::Array(
        items
            .iter()
            .map(|s| s.as_deref().map_or(JsValue::Null, JsValue::str))
            .collect(),
    )
}

fn fossil_module_js(m: &FossilLedgerModule) -> JsValue {
    let mut o = JsObject::new();
    o.insert("file", JsValue::str(m.file.as_str()));
    o.insert("hashes", str_list(&m.hashes));
    o.insert(
        "imports",
        JsValue::Array(
            m.imports
                .iter()
                .map(|&i| JsValue::Number(i as f64))
                .collect(),
        ),
    );
    o.insert_opt("declared", m.declared.as_deref().map(str_list));
    o.insert_opt("tokens", m.tokens.as_deref().map(str_list));
    JsValue::Object(o)
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
) -> JsObject {
    let mut order: Vec<String> = Vec::new();
    let mut homes: std::collections::HashMap<String, Vec<JsValue>> =
        std::collections::HashMap::new();
    for (i, stmt) in input.body.iter().enumerate() {
        for n in declared_names(stmt) {
            let list = homes.entry(n.clone()).or_insert_with(|| {
                order.push(n);
                Vec::new()
            });
            list.push(JsValue::str(assignment[i].as_str()));
        }
    }
    let name_to_files = JsObject::from_entries(
        order
            .into_iter()
            .map(|n| {
                let list = homes.remove(&n).unwrap_or_default();
                (n, JsValue::Array(list))
            })
            .collect(),
    );
    let mut ledger = JsObject::new();
    ledger.insert("version", JsValue::Number(1.0));
    ledger.insert("files", str_list(files));
    ledger.insert("nameToFiles", JsValue::Object(name_to_files));
    ledger.insert("order", str_list(assignment));
    ledger.insert("hashes", str_list(&input.hashes));
    ledger.insert("emitHashes", str_list(emit_hashes));
    ledger.insert("emitNames", opt_str_list(emit_names));
    ledger.insert(
        "hashVersion",
        JsValue::Number(STATEMENT_HASH_VERSION as f64),
    );
    ledger
}

/// `stableSplitFromCode` + `tryEmitRunnableCjs` over the shipped text.
pub fn stable_split(shipped: &str, options: SplitOptions<'_, '_>) -> Result<SplitOutcome, String> {
    let mut input = split_input(shipped)?;
    let injected = match options.ts_hashes {
        Some(partitions) => {
            let (ts_hashes, classes) = inject_ts_statement_hashes(&input, partitions)?;
            let n = ts_hashes.len();
            input.hashes = ts_hashes;
            Some((n, classes))
        }
        None => None,
    };
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
            regime: options.regime,
            prior_ledger: None,
            prior_text: None,
            match_map: None,
            carry: options.carry,
            switches: options.placement,
            namer: options.namer,
            reviser: options.reviser,
            inject_ts_hashes: options.ts_hashes.is_some(),
        },
        options.prior,
        trail,
        None,
    )?;

    // The typed parse the emit walks (same text, same spans).
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, shipped, "shipped.js");
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic())
        .ok_or("no recognizable bundle wrapper")?;
    let view = wrapper_view(ingest.semantic(), wrapper.span).ok_or("wrapper node not found")?;
    let statements = &view.body.statements;
    let typed_spans: Vec<(u32, u32)> = statements
        .iter()
        .map(|s| {
            let sp = oxc_span::GetSpan::span(s);
            (sp.start, sp.end)
        })
        .collect();
    if typed_spans != input.spans {
        return Err("the typed wrapper body and the split input disagree on spans".into());
    }
    let names: Vec<Option<String>> = input
        .body
        .iter()
        .map(|s| statement_align_name(declared_names(s)))
        .collect();
    let facts = bundle_load_order_facts(statements, shipped, options.registrar_exemption_disabled);
    let review = review_split(
        shipped,
        &input.spans,
        &assignment,
        &input.hashes,
        &names,
        &facts,
        options.prior,
        options.align,
    );
    let mut ledger = build_ledger(
        &input,
        &assignment,
        &review.files,
        &review.emit_hashes,
        &review.emit_names,
    );
    if let Some(modules) = &fossil_modules {
        ledger.insert(
            "fossilModules",
            JsValue::Array(modules.iter().map(fossil_module_js).collect()),
        );
    }
    // `assertConcatEquivalence` (finding #41): the review tree must
    // reconstruct the bundle's statements — a pre-commit failure.
    super::review::assert_concat_equivalence(&review.contents, &assignment, &input.spans, shipped)?;
    let folders: std::collections::HashSet<&str> = review
        .files
        .iter()
        .map(|f| f.rfind('/').map_or("", |at| &f[..at]))
        .collect();
    let stats = SplitStats {
        statements: input.body.len(),
        files: review.files.len(),
        folders: folders.len(),
        tiers: tier_stats,
    };

    let runnable = if options.split_pure {
        None
    } else {
        let scopes = BabelScopes::build(ingest.semantic());
        Some(emit_runnable_cjs(&RunnableInput {
            code: shipped,
            semantic: ingest.semantic(),
            scopes: &scopes,
            wrapper: &view,
            files: &review.files,
            order: &assignment,
            emit_hashes: &review.emit_hashes,
            emit_names: &review.emit_names,
            prior_aliases: options.prior.and_then(|p| p.aliases.as_ref()),
            bundle_hashes: &input.hashes,
            bundle_names: &names,
            facts: &facts,
            switches: options.align,
        }))
    };
    let mut outcome = SplitOutcome {
        files: Vec::new(),
        runnable: None,
        declined: None,
        ledger: JsValue::Null,
        stats,
        summary,
        layout: Vec::new(),
        aliases: Vec::new(),
        spans: input.spans.clone(),
        facts: Vec::new(),
        injected,
    };
    match runnable {
        Some(Ok(tree)) => {
            let alias_obj = JsObject::from_entries(
                tree.aliases
                    .iter()
                    .map(|(f, a)| (f.clone(), JsValue::str(a.as_str())))
                    .collect(),
            );
            ledger.insert("aliases", JsValue::Object(alias_obj));
            // `recordEmittedLayout` returns early under `--disable
            // emit-align`: the review layout stays, no emitIndexes.
            if !options.align.emit_align_disabled {
                ledger.insert("emitHashes", str_list(&tree.emit_hashes));
                ledger.insert("emitNames", opt_str_list(&tree.emit_names));
                ledger.insert(
                    "emitIndexes",
                    JsValue::Array(
                        tree.emit_indexes
                            .iter()
                            .map(|&i| JsValue::Number(i as f64))
                            .collect(),
                    ),
                );
            }
            outcome.runnable = Some(tree.files.iter().map(|(p, _)| p.clone()).collect());
            outcome.files = tree.files;
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
            // threw (finding #40): the aliases, and the emitted layout (and
            // the dump's emit capture) once the assembly had begun.
            if let Some(aliases) = &decline.aliases {
                ledger.insert(
                    "aliases",
                    JsValue::Object(JsObject::from_entries(
                        aliases
                            .iter()
                            .map(|(f, a)| (f.clone(), JsValue::str(a.as_str())))
                            .collect(),
                    )),
                );
            }
            outcome.layout = review.layout;
            if let Some(layout) = decline
                .layout
                .filter(|_| !options.align.emit_align_disabled)
            {
                // `captureRunnableEmitLayout` ran: the dump's emit.json is
                // the runnable layout, each file with its alias.
                outcome.aliases = decline.aliases.clone().unwrap_or_default();
                ledger.insert("emitHashes", str_list(&layout.emit_hashes));
                ledger.insert("emitNames", opt_str_list(&layout.emit_names));
                ledger.insert(
                    "emitIndexes",
                    JsValue::Array(
                        layout
                            .emit_indexes
                            .iter()
                            .map(|&i| JsValue::Number(i as f64))
                            .collect(),
                    ),
                );
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
    outcome.ledger = JsValue::Object(ledger);
    outcome.facts = facts;
    Ok(outcome)
}

#[cfg(test)]
mod stable_split_test;
