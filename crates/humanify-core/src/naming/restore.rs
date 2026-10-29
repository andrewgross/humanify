//! The restoration rule (exp160, library-names plan step 7): for a
//! claimed unit whose captured answer is a single name list, the
//! pipeline's names for that unit's slots come from the index, never the
//! model — the index's answer is what the package's own source declares
//! where the minified print anchors, the one naming evidence that is not
//! a guess.
//!
//! The rule's shape, everywhere it applies (the naming era's claimed
//! statements here — Site A — and the vendored factory bodies in the
//! unpack — Site B):
//!
//! 1. the slots are read from the canonical walk's slot mapping
//!    ([`crate::libindex::units::statement_named_prints`]): Site A walks
//!    the era's own statement — the print's hash is format- and
//!    rename-invariant, so a captured answer keyed by hash answers the
//!    walk's slots; Site B reads the capture's carried coordinates (the
//!    vendored text cannot join: the vendor rewrite touches free
//!    identifiers, which are content tokens);
//! 2. the HASH GUARD: a print answers a statement only through the exact
//!    captured hash — Site A's local walk (it is the same statement when
//!    the answer came from it, a miss is silence, never corruption),
//!    Site B's carried span (the capture matched it);
//! 3. per restored slot, the mapping's entry: slots without a binding
//!    (free identifiers) are skipped — the index never renames a global;
//! 4. a binding two prints name differently is ABSTAINED — conflicting
//!    evidence is silence, per the precision rule;
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
use crate::rename::validated::{RenameRequest, RenameState, scopes::BindingId};

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
// Site B: the vendored bodies, applied on the bundle
// ---------------------------------------------------------------------------

use crate::libdetect::discovery::RestorationApplication;
use crate::naming::waves::render::Occurrences;
use crate::rename::name_profile::NameProfile;
use crate::unpack::bun::TextEdit;

/// The vendored-body application's output: the bundle-coordinate edits the
/// bodies' slices carry, how many modules an edit touched, and the pass's
/// counters.
#[derive(Default)]
pub struct RestorationEdits {
    pub edits: Vec<TextEdit>,
    /// Modules whose bytes an edit lands in — the rest ship verbatim.
    pub files: usize,
    pub outcome: RestorationOutcome,
}

/// Site B: the vendored bodies' identifiers restored from the index's own
/// answers — the era never sees a moved module (the vendor-record detector
/// takes those files out of the per-file naming loop), so the unpack is
/// the only site that can name them.
///
/// The application runs on the BUNDLE, not on the vendored text: the
/// capture joined the index by print hash, and the prints are the
/// bundle's factory-body statements — the vendored TEXT's statements
/// cannot join, because the vendor rewrite touches free identifiers
/// (cross-factory references become `require` calls, the factory helper
/// becomes the shim's import) and free identifiers are content tokens.
/// The application rows carry each captured print's span and slots, so
/// this never re-walks: the names go through the validated path on the
/// bundle's own ingest, and the resulting edits ride the bodies' edit
/// list into the sliced text verbatim.
///
/// Renaming a slot binding cannot disturb the capture's own joins — a
/// local is slotted in the print, so the hash is rename-invariant — and a
/// slot's scope never leaves its factory body, so every edit lands inside
/// the body the row was captured in. `modules` are the extracted bodies'
/// spans (bundle coordinates): only applications inside one are applied;
/// the rest are the container statements' answers, the era's (Site A). A
/// missing slot keeps current behaviour — the minified name stays.
pub fn apply_body_restoration(
    semantic: &Semantic<'_>,
    applications: &[RestorationApplication],
    modules: &[(usize, usize)],
) -> RestorationEdits {
    let mut outcome = RestorationOutcome::default();
    if applications.is_empty() || modules.is_empty() {
        return RestorationEdits {
            edits: Vec::new(),
            files: 0,
            outcome,
        };
    }
    let in_a_module = |start: u32, end: u32| {
        modules
            .iter()
            .any(|&(s, e)| s as u32 <= start && end <= e as u32)
    };
    let mut state = RenameState::new(semantic, crate::trail::Anchor::Fresh, NameProfile::Bun);
    for row in applications {
        if !in_a_module(row.start, row.end) {
            continue; // a container statement's answer: the era's (Site A)
        }
        let crate::libindex::table::RestoreAnswer::Single(names) = &row.answer else {
            continue; // ambiguous: the caller's policy is to abstain
        };
        outcome.statements += 1;
        let before = outcome.bindings;
        let (wants, _) = fold_row(names, &row.slots, &mut outcome);
        apply_wants(&mut state, wants, &mut outcome);
        if outcome.bindings > before {
            outcome.spans.push((row.start, row.end));
        }
    }
    if outcome.applied.is_empty() {
        return RestorationEdits {
            edits: Vec::new(),
            files: 0,
            outcome,
        };
    }
    let text = semantic.source_text();
    let occ = Occurrences::build(semantic, &state);
    let edits: Vec<TextEdit> = occ
        .edits(text, &state, oxc_span::Span::new(0, text.len() as u32))
        .into_iter()
        .map(|edit| TextEdit {
            start: edit.span.start as usize,
            end: edit.span.end as usize,
            replacement: edit.text,
        })
        .collect();
    let files = modules
        .iter()
        .filter(|&&(s, e)| {
            let (s, e) = (s as u32, e as u32);
            edits
                .iter()
                .any(|edit| edit.start as u32 >= s && edit.end as u32 <= e)
        })
        .count();
    RestorationEdits {
        edits,
        files,
        outcome,
    }
}

/// The vendored-body pass's totals for the run's records (Site B).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyRestorationReport {
    /// Modules whose bytes an edit touched — the rest shipped verbatim.
    pub files: usize,
    /// Bindings, conflicts and statements summed over the applications.
    pub totals: RestorationOutcome,
}

/// One application row's wanted names: the answer's slots resolved to
/// their symbols; two slots of one row naming the same symbol differently
/// abstain — the statement walk's conflict rule.
fn fold_row(
    names: &[crate::libindex::names::RestoredName],
    slots: &[Option<u32>],
    outcome: &mut RestorationOutcome,
) -> (Vec<Want>, BTreeSet<SymbolId>) {
    let mut wants: BTreeMap<SymbolId, String> = BTreeMap::new();
    let mut conflicted: BTreeSet<SymbolId> = BTreeSet::new();
    for restored in names {
        let Some(Some(symbol)) = slots.get(restored.slot as usize) else {
            continue; // a free identifier: never renamed
        };
        let symbol = SymbolId::from_usize(*symbol as usize);
        match wants.entry(symbol) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(restored.name.clone());
            }
            std::collections::btree_map::Entry::Occupied(e) => {
                if e.get() != &restored.name {
                    conflicted.insert(symbol);
                    e.remove();
                    outcome.conflicts += 1;
                }
            }
        }
    }
    (
        wants
            .into_iter()
            .filter(|(symbol, _)| !conflicted.contains(symbol))
            .collect(),
        conflicted,
    )
}
