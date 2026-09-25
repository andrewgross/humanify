//! The per-identifier strategy trail (WP3.1) — TS original:
//! `src/rename/strategy-trail.ts`.
//!
//! Every naming strategy that CONSIDERS a binding records its attempt
//! (applied / rejected / abstained / vote-routed + reason). Recording for a
//! binding stops at the first applied entry — the name is settled — and
//! later attempts only bump `post_settle_attempts`, the clobber detector (a
//! tier renaming an already-settled binding is a phase-ordering bug).
//! Post passes (naming floor, reconcile, sweep) legitimately act on settled
//! bindings: `record_post_pass` keeps appending and moves `terminal_by`.
//!
//! Identity: the TS keys entries by the declaration IDENTIFIER NODE (epoch-
//! stable across Babel's scope re-crawls, exp059). The Rust key is the
//! declaration identifier's span in its anchored text — the same identity,
//! and the one the dump joins on. Entries keep FIRST-RECORD order (the TS
//! `Map` insertion order): the report and the funnel read it.
//!
//! The trail is owned by the rename state's applier (`rename::validated`):
//! tiers do not write it directly for renames — `attempt_validated_rename`
//! records the attempt it decided (02 §5: a tier cannot run untrailed; the
//! applier counts the calls that opted out).

use std::collections::BTreeMap;

use humanify_model::dump::{SpanKey, TransferAttempt, TransferRow};
use oxc_span::Span;

/// A naming strategy — the closed set of tiers the TS records (`strategy`
/// strings, pinned against the oracle's transfers.json tiers).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Tier {
    ExactMatch,
    CloseMatch,
    BindingCascade,
    StatementTwin,
    ClosureCapture,
    FnNameVote,
    FnNamePin,
    ModuleVote,
    ModulePin,
    VoteSuggest,
    Retry,
    Llm,
    ClassIdFloor,
    DecorationRetry,
    CoverageSweep,
    Reconcile,
    ReconcileAsymmetric,
    ReconcileDescriptive,
    ReconcileConsumer,
    ReconcileLastResort,
}

impl Tier {
    /// The TS strategy string.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::ExactMatch => "exact-match",
            Tier::CloseMatch => "close-match",
            Tier::BindingCascade => "binding-cascade",
            Tier::StatementTwin => "statement-twin",
            Tier::ClosureCapture => "closure-capture",
            Tier::FnNameVote => "fn-name-vote",
            Tier::FnNamePin => "fn-name-pin",
            Tier::ModuleVote => "module-vote",
            Tier::ModulePin => "module-pin",
            Tier::VoteSuggest => "vote-suggest",
            Tier::Retry => "retry",
            Tier::Llm => "llm",
            Tier::ClassIdFloor => "class-id-floor",
            Tier::DecorationRetry => "decoration-retry",
            Tier::CoverageSweep => "coverage-sweep",
            Tier::Reconcile => "reconcile",
            Tier::ReconcileAsymmetric => "reconcile-asymmetric",
            Tier::ReconcileDescriptive => "reconcile-descriptive",
            Tier::ReconcileConsumer => "reconcile-consumer",
            Tier::ReconcileLastResort => "reconcile-last-resort",
        }
    }
}

/// An attempt's outcome.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Outcome {
    Applied,
    Rejected,
    Abstained,
    Vote,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Applied => "applied",
            Outcome::Rejected => "rejected",
            Outcome::Abstained => "abstained",
            Outcome::Vote => "vote",
        }
    }
}

/// Which anchored text a declaration span indexes into (07 §1): naming-era
/// records anchor `fresh`; the reconcile pass parses the generated output;
/// the deferred sweep the reconciled output (else the generated).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Anchor {
    Fresh,
    Generated,
    Reconciled,
    Shipped,
}

impl Anchor {
    pub fn as_str(self) -> &'static str {
        match self {
            Anchor::Fresh => "fresh",
            Anchor::Generated => "generated",
            Anchor::Reconciled => "reconciled",
            Anchor::Shipped => "shipped",
        }
    }
}

/// One recorded attempt (`StrategyAttempt`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Attempt {
    pub tier: Tier,
    pub outcome: Outcome,
    pub reason: Option<String>,
    /// `newName` — the proposed name.
    pub proposed_name: Option<String>,
    /// References the binding had WHEN THIS ATTEMPT RAN; None = not
    /// measured (never collapsed into zero — the exp059 smoking gun).
    pub ref_count: Option<u32>,
    /// The scope block the rename went through (`scopeBlock`, the byte
    /// span; the diagnostics print it as UTF-16 `start:end`) — the llm
    /// tier records it.
    pub scope_block: Option<oxc_span::Span>,
}

impl Attempt {
    /// An attempt with no reason, name or count.
    pub fn new(tier: Tier, outcome: Outcome) -> Attempt {
        Attempt {
            tier,
            outcome,
            reason: None,
            proposed_name: None,
            ref_count: None,
            scope_block: None,
        }
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Attempt {
        self.reason = Some(reason.into());
        self
    }

    pub fn proposed(mut self, name: impl Into<String>) -> Attempt {
        self.proposed_name = Some(name.into());
        self
    }

    pub fn scope_block(mut self, block: oxc_span::Span) -> Attempt {
        self.scope_block = Some(block);
        self
    }

    pub fn ref_count(mut self, count: u32) -> Attempt {
        self.ref_count = Some(count);
        self
    }
}

/// The binding a trail row is about: its declaration identifier's span in
/// an anchored text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TrailTarget {
    pub anchor: Anchor,
    pub decl_span: Span,
}

/// A row's own span key (its anchor, its UTF-8 byte span).
fn target_key(e: &TrailEntry) -> SpanKey {
    SpanKey {
        text: e.target.anchor.as_str().to_string(),
        start: i64::from(e.target.decl_span.start),
        end: i64::from(e.target.decl_span.end),
    }
}

/// One binding's trail (`StrategyTrailEntry`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrailEntry {
    pub target: TrailTarget,
    pub old_name: String,
    pub attempts: Vec<Attempt>,
    pub settled_by: Option<Tier>,
    /// The last applied tier across ALL passes (the terminal namer).
    pub terminal_by: Option<Tier>,
    /// The name the last applied attempt gave the binding.
    pub final_name: Option<String>,
    /// Rename attempts after settling — should be 0 (the clobber detector).
    pub post_settle_attempts: u32,
    /// Vote testimony after settling — expected, counted, never logged.
    pub post_settle_votes: u32,
}

/// `strategy → outcome → count`, in first-seen order (the TS object order).
pub type Funnel = Vec<(Tier, Vec<(Outcome, u64)>)>;

/// The recorder (`StrategyTrailRecorder`).
#[derive(Clone, Debug, Default)]
pub struct StrategyTrail {
    enabled: bool,
    entries: Vec<TrailEntry>,
    index: BTreeMap<TrailTarget, usize>,
    /// The run-wide validated-rename claim counters (TS: a module-level
    /// recorder like the trail): each pass state starts from the trail it
    /// continues and writes its total back on `finish`.
    pub claims: crate::rename::validated::RenameClaimStats,
}

impl TrailEntry {
    /// transfers.json's row for this entry under a given target key.
    pub fn transfer_row(&self, target: SpanKey) -> TransferRow {
        TransferRow {
            target,
            old_name: self.old_name.clone(),
            final_name: self.final_name.clone(),
            settled_by: self.settled_by.map(|t| t.as_str().to_string()),
            attempts: self
                .attempts
                .iter()
                .map(|a| TransferAttempt {
                    tier: a.tier.as_str().to_string(),
                    outcome: a.outcome.as_str().to_string(),
                    reason: a.reason.clone(),
                    proposed_name: a.proposed_name.clone(),
                })
                .collect(),
        }
    }
}

impl StrategyTrail {
    /// An armed (recording) trail.
    pub fn enabled() -> StrategyTrail {
        StrategyTrail {
            enabled: true,
            ..StrategyTrail::default()
        }
    }

    /// Clear state and set enablement for the coming run.
    pub fn reset(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.entries.clear();
        self.index.clear();
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Record a settling-tier attempt (`record`).
    pub fn record(&mut self, target: TrailTarget, old_name: &str, attempt: Attempt) {
        if !self.enabled {
            return;
        }
        let entry = self.entry_for(target, old_name);
        if entry.settled_by.is_some() {
            if attempt.outcome == Outcome::Vote {
                entry.post_settle_votes += 1;
            } else {
                entry.post_settle_attempts += 1;
            }
            return;
        }
        if attempt.outcome == Outcome::Applied {
            entry.settled_by = Some(attempt.tier);
            entry.terminal_by = Some(attempt.tier);
            entry.final_name = attempt.proposed_name.clone();
        }
        entry.attempts.push(attempt);
    }

    /// Record a POST-pass attempt (`recordPostPass`): no settled-stop, no
    /// clobber counting; an applied attempt moves `terminal_by`.
    pub fn record_post_pass(&mut self, target: TrailTarget, old_name: &str, attempt: Attempt) {
        if !self.enabled {
            return;
        }
        let entry = self.entry_for(target, old_name);
        if attempt.outcome == Outcome::Applied {
            entry.terminal_by = Some(attempt.tier);
            entry.final_name = attempt.proposed_name.clone();
        }
        entry.attempts.push(attempt);
    }

    fn entry_for(&mut self, target: TrailTarget, old_name: &str) -> &mut TrailEntry {
        let next = self.entries.len();
        let idx = *self.index.entry(target).or_insert(next);
        if idx == next {
            self.entries.push(TrailEntry {
                target,
                old_name: old_name.to_string(),
                attempts: Vec::new(),
                settled_by: None,
                terminal_by: None,
                final_name: None,
                post_settle_attempts: 0,
                post_settle_votes: 0,
            });
        }
        &mut self.entries[idx]
    }

    /// The entries, in first-record order.
    pub fn entries(&self) -> &[TrailEntry] {
        &self.entries
    }

    /// The per-strategy funnel over every recorded attempt.
    pub fn funnel(&self) -> Funnel {
        let mut funnel: Funnel = Vec::new();
        for attempt in self.entries.iter().flat_map(|e| &e.attempts) {
            let row = match funnel.iter().position(|(t, _)| *t == attempt.tier) {
                Some(i) => &mut funnel[i].1,
                None => {
                    funnel.push((attempt.tier, Vec::new()));
                    &mut funnel.last_mut().expect("just pushed").1
                }
            };
            match row.iter_mut().find(|(o, _)| *o == attempt.outcome) {
                Some((_, n)) => *n += 1,
                None => row.push((attempt.outcome, 1)),
            }
        }
        funnel
    }

    /// transfers.json rows (`writeTransfers`): every entry, span-keyed in
    /// its own anchored text, sorted by span key (stable — ties keep
    /// first-record order, as the TS `Array.sort` does). Spans are the
    /// text's UTF-8 byte offsets already (07 §1's decided unit).
    pub fn transfer_rows(&self) -> Vec<TransferRow> {
        let mut rows: Vec<TransferRow> = self
            .entries
            .iter()
            .map(|e| e.transfer_row(target_key(e)))
            .collect();
        rows.sort_by(|a, b| a.target.cmp(&b.target));
        rows
    }
}

#[cfg(test)]
mod trail_test;
