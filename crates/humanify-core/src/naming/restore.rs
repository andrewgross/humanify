//! The restoration rule (exp160, library-names plan step 7): for a
//! claimed unit whose captured answer is a single name list, the
//! pipeline's names for that unit's slots come from the index, never the
//! model — the index's answer is what the package's own source declares
//! where the minified print anchors, the one naming evidence that is not
//! a guess.
//!
//! The rule's shape, everywhere it applies (the naming era's claimed
//! statements here, the vendor factory bodies in the unpack):
//!
//! 1. walk the local unit with the canonical walk
//!    ([`crate::libindex::units::statement_named_prints`]) — the print's
//!    hash is format- and rename-invariant, so a captured answer keyed by
//!    hash answers this walk's slots;
//! 2. the HASH GUARD: the local walk's whole-print hash must be the
//!    captured key itself (it is, by construction, when the answer came
//!    from the same statement) — a miss is silence, never corruption;
//! 3. per restored slot, the local mapping's entry: slots without a
//!    binding (free identifiers) are skipped — the index never renames a
//!    global;
//! 4. a binding two prints of the same unit name differently is ABSTAINED
//!    — conflicting evidence is silence, per the precision rule;
//! 5. every application goes through
//!    [`crate::rename::validated::RenameState::attempt_restored_rename`]
//!    — the validated path only, with the library hold lifted for the
//!    one writer that owns placed modules' names.
//!
//! Two abstentions are policy, recorded here because both are load-bearing:
//!
//! - **Ambiguous answers abstain.** Settling one list needs the
//!   version-range evidence the `names/1` table does not carry (it lives
//!   in the provenance JSONL the runs never read); preferring a list
//!   without it would guess. The fix is a `names/2` format carrying per
//!   list's ranges, not a run-side heuristic.
//! - **A binding the validated path already settled keeps its name.** A
//!   prior release carried a name for it through the transfer; the
//!   transfer's evidence is at least as fresh as the index's.

use std::collections::{BTreeMap, BTreeSet};

use oxc_semantic::{Semantic, SymbolId};

use crate::hash::serialize::SymbolTables;
use crate::libindex::table::{RestorationRow, RestoreAnswer};
use crate::libindex::units::statement_named_prints;
use crate::naming::module_names::{ClaimedRegion, ModuleSource};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec, scopes::BindingId};

/// What one restoration pass applied, for the run's records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestorationOutcome {
    /// Bindings renamed from the index's answers.
    pub bindings: usize,
    /// Bindings two prints of the same unit named differently — abstained.
    pub conflicts: usize,
    /// Statements whose whole print had a captured answer.
    pub statements: usize,
    /// `(binding, name)` per applied rename, in statement order — the
    /// caller records them carried and keeps the waves off them.
    pub applied: Vec<(BindingId, String)>,
    /// The ERA-absolute spans of the statements a rename was applied in,
    /// statement order — the caller's protected-span set for the passes
    /// that re-parse the generated text (the prior-diff reconcile: the
    /// prior's names must not overwrite the index's).
    pub spans: Vec<(u32, u32)>,
}

/// The era's restoration over its claimed statements: per statement, the
/// index's names for the slots its canonical walk spells. The moved
/// modules' statements and the claimed-but-unmoved ones are the same
/// question — the layout only decides where they land, not what they are
/// called.
pub fn apply_era_restoration(
    state: &mut RenameState,
    semantic: &Semantic<'_>,
    modules: &ModuleSource<'_>,
    regions: &[ClaimedRegion],
    restorations: &BTreeMap<String, RestorationRow>,
) -> RestorationOutcome {
    let mut outcome = RestorationOutcome::default();
    if restorations.is_empty() {
        return outcome;
    }
    let tables = SymbolTables::build(semantic);
    let mut visited: BTreeSet<usize> = BTreeSet::new();
    for region in regions {
        for &s in &region.statements {
            if !visited.insert(s) {
                continue;
            }
            let Some(node) = modules.body.get(s) else {
                continue;
            };
            let prints = statement_named_prints(node, &tables);
            let before = outcome.bindings;
            let (wants, _) = fold_answers(&prints, restorations, &mut outcome);
            apply_wants(state, wants, &mut outcome);
            if outcome.bindings > before {
                let span = prints.whole.print.span;
                outcome.spans.push((span.start, span.end));
            }
        }
    }
    outcome
}

/// One binding's wanted name: the symbol and the single name the prints
/// agree on.
type Want = (SymbolId, String);

/// Fold a statement's prints' answers into per-symbol wanted names;
/// conflicts abstain. Only `RestoreAnswer::Single` answers — an
/// ambiguous one is silence (see the module docs).
fn fold_answers(
    prints: &crate::libindex::units::NamedStatementPrints,
    restorations: &BTreeMap<String, RestorationRow>,
    outcome: &mut RestorationOutcome,
) -> (Vec<Want>, BTreeSet<SymbolId>) {
    let mut wants: BTreeMap<SymbolId, String> = BTreeMap::new();
    let mut conflicted: BTreeSet<SymbolId> = BTreeSet::new();
    let mut answered = false;
    let prints_of = std::iter::once(&prints.whole).chain(prints.functions.iter());
    for named in prints_of {
        let Some(row) = restorations.get(&named.print.hash) else {
            continue;
        };
        let RestoreAnswer::Single(names) = &row.answer else {
            continue; // ambiguous: the caller's policy is to abstain
        };
        answered = true;
        for restored in names {
            let Some((_, Some(symbol), _)) = named.mapping.get(restored.slot as usize) else {
                continue; // a free identifier: never renamed
            };
            match wants.entry(*symbol) {
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(restored.name.clone());
                }
                std::collections::btree_map::Entry::Occupied(e) => {
                    if e.get() != &restored.name {
                        conflicted.insert(*symbol);
                        e.remove();
                        outcome.conflicts += 1;
                    }
                }
            }
        }
    }
    if answered {
        outcome.statements += 1;
    }
    (
        wants
            .into_iter()
            .filter(|(symbol, _)| !conflicted.contains(symbol))
            .collect(),
        conflicted,
    )
}

/// Apply the wanted names through the validated path; the settled and the
/// already-restored keep what they have.
fn apply_wants(state: &mut RenameState, wants: Vec<Want>, outcome: &mut RestorationOutcome) {
    for (symbol, name) in wants {
        let Some(binding) = state.view().binding_of_symbol(symbol) else {
            continue;
        };
        if state.is_renamed(binding) {
            continue; // the validated path settled this binding already
        }
        if state.name_of(binding) == name {
            continue; // already the restored name
        }
        let scope = state.scope_of_binding(binding);
        let old_name = state.name_of(binding).to_string();
        let request = RenameRequest {
            scope,
            old_name: &old_name,
            new_name: &name,
            expected: Some(binding),
        };
        if state.attempt_restored_rename(request).applied {
            outcome.bindings += 1;
            outcome.applied.push((binding, name));
        }
    }
}

// ---------------------------------------------------------------------------
// Site B: the vendored bodies
// ---------------------------------------------------------------------------

use crate::ingest::Ingest;
use crate::naming::waves::render::Occurrences;
use crate::rename::name_profile::NameProfile;
use crate::unpack::bun::{TextEdit, slice_with_edits};

/// What one vendored body's restoration produced.
#[derive(Debug)]
pub struct BodyRestoration {
    /// The body text as the file ships it (verbatim when nothing applied).
    pub text: String,
    /// The pass's counters (the run's restored-identifier report).
    pub outcome: RestorationOutcome,
}

/// The vendored-body pass's totals for the run's records (Site B).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyRestorationReport {
    /// Bodies whose text was rewritten — the rest shipped verbatim.
    pub files: usize,
    /// Bindings, conflicts and statements summed over the bodies.
    pub totals: RestorationOutcome,
}

impl BodyRestorationReport {
    /// Absorb one body's outcome into the run's totals.
    pub fn record(&mut self, outcome: &RestorationOutcome) {
        if !outcome.applied.is_empty() {
            self.files += 1;
        }
        self.totals.bindings += outcome.bindings;
        self.totals.conflicts += outcome.conflicts;
        self.totals.statements += outcome.statements;
    }
}

/// Site B: a vendored body's identifiers restored from the index's own
/// answers — the era's restoration (Sites A/C) never sees a moved module,
/// because the unpack extracts it before the naming era runs. The body is
/// parsed wrapped in a synthetic function (its statements are otherwise
/// already-valid statements, so the wrap is never printed), the walk is
/// the canonical one and the answer lookup is the same hash map the
/// bundle-side capture filled.
///
/// The prior release's same-path file is the SECOND evidence, never the
/// first: a slot the index answers gets the index's name (the original
/// source is the one evidence that is not a guess, and a stale prior
/// answer must never block a fresh one); a slot the index does not answer
/// carries the prior release's spelling for it — the validated transfer's
/// StatementTwin rule over identical statements. A prior that was never
/// restored spells its slots the minified way, which is this body's own
/// spelling: the name-equal skip makes that a no-op.
///
/// Every application goes through the validated path (never a raw
/// rename); the edits are applied to the body text VERBATIM — the
/// vendor file's bytes are never beautified.
pub fn restore_body_text(
    body: &str,
    restorations: &BTreeMap<String, RestorationRow>,
    prior_body: Option<&str>,
) -> BodyRestoration {
    if restorations.is_empty() && prior_body.is_none() {
        return BodyRestoration {
            text: body.to_string(),
            outcome: RestorationOutcome::default(),
        };
    }
    // The wrap: a named function declaration holding the body's
    // statements. Its own name is never asked for, and its span sits
    // outside the slice the edits are applied over.
    const WRAP_PREFIX: &str = "function restoreWrap() {\n";
    const WRAP_SUFFIX: &str = "\n}";
    let wrapped = format!("{WRAP_PREFIX}{body}{WRAP_SUFFIX}");
    let mut outcome = RestorationOutcome::default();
    let edited = {
        let allocator = oxc_allocator::Allocator::default();
        let ingest = Ingest::parse(&allocator, &wrapped, "vendor-body.js");
        if !ingest.errors.is_empty() {
            // A body the parser refuses is shipped verbatim — restoration
            // is never allowed to lose a file.
            return BodyRestoration {
                text: body.to_string(),
                outcome,
            };
        }
        let semantic = ingest.semantic();
        let tables = SymbolTables::build(semantic);
        let json = crate::ingest::program_estree_json(ingest.program);
        let Some(statements) = json["body"][0]["body"]["body"].as_array() else {
            return BodyRestoration {
                text: body.to_string(),
                outcome,
            };
        };
        let prior_names = prior_body.map(prior_slot_names).unwrap_or_default();
        let mut state = RenameState::new(semantic, crate::trail::Anchor::Fresh, NameProfile::Bun);
        for statement in statements {
            let prints = statement_named_prints(statement, &tables);
            // 1. the index's own answer — the original names.
            let (wants, _) = fold_answers(&prints, restorations, &mut outcome);
            apply_wants(&mut state, wants, &mut outcome);
            // 2. the prior release's spelling for the slots the index did
            //    not just answer.
            let carried = prior_wants(&prints, &prior_names);
            apply_carried(&mut state, carried, &mut outcome);
        }
        let occ = Occurrences::build(semantic, &state);
        let edits = occ.edits(
            &wrapped,
            &state,
            oxc_span::Span::new(0, wrapped.len() as u32),
        );
        if edits.is_empty() {
            return BodyRestoration {
                text: body.to_string(),
                outcome,
            };
        }
        let text_edits: Vec<TextEdit> = edits
            .into_iter()
            .map(|edit| TextEdit {
                start: edit.span.start as usize,
                end: edit.span.end as usize,
                replacement: edit.text,
            })
            .collect();
        slice_with_edits(
            &wrapped,
            &text_edits,
            WRAP_PREFIX.len(),
            wrapped.len() - WRAP_SUFFIX.len(),
        )
    };
    BodyRestoration {
        text: edited,
        outcome,
    }
}

/// The prior release's spellings, keyed the same way: print hash →
/// `(slot, name as the prior text spells it)`, whole prints and function
/// prints alike. A file that does not parse contributes nothing.
fn prior_slot_names(prior_body: &str) -> BTreeMap<String, Vec<(u32, String)>> {
    let wrapped = format!("function restoreWrap() {{\n{prior_body}\n}}");
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, &wrapped, "prior-vendor.js");
    let mut out = BTreeMap::new();
    if !ingest.errors.is_empty() {
        return out;
    }
    let tables = SymbolTables::build(ingest.semantic());
    let json = crate::ingest::program_estree_json(ingest.program);
    let Some(statements) = json["body"][0]["body"]["body"].as_array() else {
        return out;
    };
    for statement in statements {
        let prints = statement_named_prints(statement, &tables);
        let prints_of = std::iter::once(&prints.whole).chain(prints.functions.iter());
        for named in prints_of {
            let hash = named.print.hash.clone();
            let pairs = out.entry(hash).or_insert_with(Vec::new);
            for (ordinal, (_, symbol, name)) in named.mapping.iter().enumerate() {
                // slot = the mapping's index (one entry per slot, in slot
                // order).
                let slot = ordinal as u32;
                if symbol.is_some() && !pairs.iter().any(|(s, _)| *s == slot) {
                    pairs.push((slot, name.clone()));
                }
            }
        }
    }
    out
}

/// The prior's names for one statement's slots: the hash-matched prior
/// print's `(slot, name)` pairs, keyed by the LOCAL symbol.
fn prior_wants(
    prints: &crate::libindex::units::NamedStatementPrints,
    prior_names: &BTreeMap<String, Vec<(u32, String)>>,
) -> Vec<(oxc_semantic::SymbolId, String)> {
    let mut wants: BTreeMap<oxc_semantic::SymbolId, String> = BTreeMap::new();
    let prints_of = std::iter::once(&prints.whole).chain(prints.functions.iter());
    for named in prints_of {
        let Some(pairs) = prior_names.get(named.print.hash.as_str()) else {
            continue;
        };
        for (slot, name) in pairs {
            let Some((_, Some(symbol), _)) = named.mapping.get(*slot as usize) else {
                continue;
            };
            wants.entry(*symbol).or_insert_with(|| name.clone());
        }
    }
    wants.into_iter().collect()
}

/// Apply the prior's names through the validated path (the transfer's own
/// writer, StatementTwin tier): a binding the restoration just named, or
/// whose spelling already matches, is left alone.
fn apply_carried(
    state: &mut RenameState,
    carried: Vec<(oxc_semantic::SymbolId, String)>,
    outcome: &mut RestorationOutcome,
) {
    for (symbol, name) in carried {
        let Some(binding) = state.view().binding_of_symbol(symbol) else {
            continue;
        };
        if state.is_renamed(binding) || state.name_of(binding) == name {
            continue;
        }
        let scope = state.scope_of_binding(binding);
        let old_name = state.name_of(binding).to_string();
        let request = RenameRequest {
            scope,
            old_name: &old_name,
            new_name: &name,
            expected: Some(binding),
        };
        if state
            .attempt_validated_rename(
                request,
                TrailSpec::settling(crate::trail::Tier::StatementTwin),
            )
            .applied
        {
            outcome.bindings += 1;
            outcome.applied.push((binding, name));
        }
    }
}
