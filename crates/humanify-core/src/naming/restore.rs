//! The restoration rule (exp160, library-names plan step 7): for a
//! claimed unit whose captured answer is a single name list, the
//! pipeline's names for that unit's slots come from the index, never the
//! model — the index's answer is what the package's own source declares
//! where the minified print anchors, the one naming evidence that is not
//! a guess.
//!
//! The rule's shape, everywhere it applies (the naming era's claimed
//! statements here — Site A, a re-derivation over the era's own walk —
//! and in the unpack — Site B: the vendored factory bodies AND the
//! container statements, applied from the capture's carried coordinates
//! before the format can reshape anything):
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
    /// Per visited statement, what the pass saw and decided (exp164) —
    /// the diagnostics dump's evidence, never a decision input.
    pub sites: Vec<RestorationSite>,
}

/// Why a wanted name was not applied, as the diagnostics spell it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteSkip {
    /// A binding the validated path already settled (the transfer, or the
    /// library freeze's carried mark) keeps its name.
    Renamed,
    /// Two prints name the binding differently — abstained.
    Conflict,
    /// The validated path refused the rename.
    Failed,
}

impl SiteSkip {
    pub fn as_str(self) -> &'static str {
        match self {
            SiteSkip::Renamed => "renamed",
            SiteSkip::Conflict => "conflict",
            SiteSkip::Failed => "failed",
        }
    }
}

impl serde::Serialize for SiteSkip {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One print's answer state at a visited statement (exp164).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePrint {
    pub hash: String,
    /// The walk's slot mapping, one entry per slot in slot order:
    /// `(the walk's `$slot` spelling, the symbol id, the name as the
    /// walked AST spells it)` — how an answered slot joins the binding.
    pub slots: Vec<(String, Option<u32>, String)>,
    /// The hash is one of the run's captured answers.
    pub captured: bool,
    /// The captured answer is ambiguous (abstain by policy).
    pub ambiguous: bool,
    /// `(slot, name)` per entry a `Single` answer names.
    pub answered: Vec<(u32, String)>,
}

/// One visited statement's restoration evidence (exp164, diagnostics
/// only): every print's answer state and every apply decision. A
/// statement visited with no captured print is itself a finding — the
/// claim existed, the index's answer did not.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorationSite {
    /// The statement's ordinal in the era's wrapper body.
    pub statement: usize,
    /// The statement's span (the whole print's), era coordinates.
    pub span: (u32, u32),
    pub prints: Vec<SitePrint>,
    /// `(symbol, name)` per applied rename.
    pub applied: Vec<(u32, String)>,
    /// `(symbol, name, cause)` per wanted name not applied.
    pub skipped: Vec<(u32, String, SiteSkip)>,
}

/// The naming era's restoration evidence for the diagnostics dump
/// (exp164, measurement only — no decision reads it): the era's own
/// statement coordinates, the library freeze's bindings, the claimed
/// regions as converted, and Site A's per-statement rows. The offline
/// census joins the shipped minted bindings against this by statement
/// ordinal.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraDump {
    /// The era's wrapper-body statement count.
    pub body_len: usize,
    /// Each statement's span, era coordinates, statement order.
    pub body_spans: Vec<(u32, u32)>,
    /// The library freeze's bindings: `(statement, symbol, name at
    /// freeze time)` — a frozen binding is never renamed after this, so
    /// the name is the shipped one.
    pub frozen: Vec<(usize, u32, String)>,
    /// The claimed regions as the era converted them (exp166: through the
    /// exact bridge), with the statements' raw bundle spans.
    pub regions: Vec<ClaimedRegion>,
    /// Site A's rows.
    pub sites: Vec<RestorationSite>,
    /// exp166's placement-precision inputs, from the exact raw-to-era
    /// bridge: each era statement's RAW bundle span (`body_spans` order;
    /// None when the run had no bridge), the moved statements' era spans
    /// (the freeze), and every vendored factory's raw span and file.
    pub body_raw_spans: Vec<Option<(u32, u32)>>,
    pub moved_spans: Vec<(u32, u32)>,
    pub factory_files: Vec<((u32, u32), String)>,
    /// What the exact conversion did (None without a bridge).
    pub conversion: Option<crate::naming::module_names::RegionConversion>,
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
            let mut site = RestorationSite {
                statement: s,
                span: (prints.whole.print.span.start, prints.whole.print.span.end),
                ..RestorationSite::default()
            };
            let (wants, _) = fold_answers(&prints, restorations, &mut outcome, &mut site);
            apply_wants(state, wants, &mut outcome, Some(&mut site));
            outcome.sites.push(site);
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
/// ambiguous one is silence (see the module docs). `site` collects the
/// per-print evidence (exp164).
fn fold_answers(
    prints: &crate::libindex::units::NamedStatementPrints,
    restorations: &BTreeMap<String, RestorationRow>,
    outcome: &mut RestorationOutcome,
    site: &mut RestorationSite,
) -> (Vec<Want>, BTreeSet<SymbolId>) {
    let mut wants: BTreeMap<SymbolId, String> = BTreeMap::new();
    let mut conflicted: BTreeSet<SymbolId> = BTreeSet::new();
    let mut answered = false;
    let prints_of = std::iter::once(&prints.whole).chain(prints.functions.iter());
    for named in prints_of {
        let mut row = SitePrint {
            hash: named.print.hash.clone(),
            slots: named
                .mapping
                .iter()
                .map(|(spelling, symbol, name)| {
                    (
                        spelling.clone(),
                        symbol.map(|s| s.index() as u32),
                        name.clone(),
                    )
                })
                .collect(),
            ..SitePrint::default()
        };
        if let Some(captured) = restorations.get(&named.print.hash) {
            row.captured = true;
            if let RestoreAnswer::Single(names) = &captured.answer {
                answered = true;
                row.answered = names
                    .iter()
                    .map(|restored| (restored.slot, restored.name.clone()))
                    .collect();
                for restored in names {
                    let Some((_, Some(symbol), _)) = named.mapping.get(restored.slot as usize)
                    else {
                        continue; // a free identifier: never renamed
                    };
                    match wants.entry(*symbol) {
                        std::collections::btree_map::Entry::Vacant(e) => {
                            e.insert(restored.name.clone());
                        }
                        std::collections::btree_map::Entry::Occupied(e) => {
                            if e.get() != &restored.name {
                                conflicted.insert(*symbol);
                                site.skipped.push((
                                    symbol.index() as u32,
                                    e.get().clone(),
                                    SiteSkip::Conflict,
                                ));
                                e.remove();
                                outcome.conflicts += 1;
                            }
                        }
                    }
                }
            } else {
                row.ambiguous = true;
            }
        }
        site.prints.push(row);
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
/// already-restored keep what they have. `site` (Site A only) records the
/// per-decision evidence (exp164).
fn apply_wants(
    state: &mut RenameState,
    wants: Vec<Want>,
    outcome: &mut RestorationOutcome,
    mut site: Option<&mut RestorationSite>,
) {
    for (symbol, name) in wants {
        let Some(binding) = state.view().binding_of_symbol(symbol) else {
            continue;
        };
        if state.is_renamed(binding) {
            if let Some(site) = site.as_deref_mut() {
                site.skipped
                    .push((symbol.index() as u32, name, SiteSkip::Renamed));
            }
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
            outcome.applied.push((binding, name.clone()));
            if let Some(site) = site.as_deref_mut() {
                site.applied.push((symbol.index() as u32, name));
            }
        } else if let Some(site) = site.as_deref_mut() {
            site.skipped
                .push((symbol.index() as u32, name, SiteSkip::Failed));
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

/// Site B: the vendored bodies' AND the container statements' identifiers
/// restored from the index's own answers — the era never sees a moved
/// module (the vendor-record detector takes those files out of the
/// per-file naming loop), so the unpack is the only site that can name
/// them. The container statements (the unspliced lazy-init wrappers) are
/// the same question: the library layout freezes their bindings and the
/// waves never ask them, so the unpack names them too (exp164 — the mint
/// premium).
///
/// The application runs on the BUNDLE, not on the vendored text: the
/// capture joined the index by print hash, and the prints are the
/// bundle's statements — the vendored TEXT's statements cannot join,
/// because the vendor rewrite touches free identifiers (cross-factory
/// references become `require` calls, the factory helper becomes the
/// shim's import) and free identifiers are content tokens. The
/// application rows carry each captured print's span and slots, so this
/// never re-walks: the names go through the validated path on the
/// bundle's own ingest, and the resulting edits ride the bodies' edit
/// list into the sliced bodies verbatim — the caller also extends them
/// into the container's edit list, where a body-range edit sits under the
/// declaration splice and is dropped by the slice's overlap rule.
///
/// Renaming a slot binding cannot disturb the capture's own joins — a
/// local is slotted in the print, so the hash is rename-invariant — and
/// the validated path settles every rename, so a container binding's
/// occurrences inside the bodies are renamed by the same edit list (the
/// bodies' slices carry it, the container's drops it). `modules` are the
/// extracted bodies' spans (bundle coordinates), counted into `files`. A
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
    let mut state = RenameState::new(semantic, crate::trail::Anchor::Fresh, NameProfile::Bun);
    for row in applications {
        let crate::libindex::table::RestoreAnswer::Single(names) = &row.answer else {
            continue; // ambiguous: the caller's policy is to abstain
        };
        outcome.statements += 1;
        let before = outcome.bindings;
        let (wants, _) = fold_row(names, &row.slots, &mut outcome);
        apply_wants(&mut state, wants, &mut outcome, None);
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
