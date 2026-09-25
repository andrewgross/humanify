//! The batch-until-done loop of ONE lane — TS `runBatchRenameLoop`
//! (processor.ts): batch windows with per-identifier retries, free retries
//! for names claimed mid-call, adaptive batch sizing, the straggler pass,
//! the algorithmic resolution of what remains, and the identity records of
//! what stays unrenamed.
//!
//! A lane is a STATE MACHINE: [`Lane::next_call`] yields the next request's
//! (batch, round, prev, failures), the driver builds and dispatches the
//! request, and [`Lane::feed`] consumes the response. Within a wave round
//! every lane reads only FROZEN shared state (the barrier applies renames
//! between rounds) plus its own claims, so a lane's sequence of requests is
//! independent of how lanes interleave — the TS's concurrent dispatch
//! order is not a decision input; the barrier's apply order is.
//!
//! What the lane decides lands in [`Lane::effects`] — the wave-collected
//! renames and identity records, in collection order.

use std::collections::{HashMap, HashSet, VecDeque};

use humanify_model::llm::{RenameFailures, Renames};

use super::jsset::JsRecord;
use crate::naming::report::{
    AttemptResult, ContentionEvent, IdentifierOutcome, Outcomes, RoundAttempt, Status,
};
use crate::naming::validation::{resolve_conflict, sanitize_identifier};
use crate::rename::validated::target::is_valid_rename_target;

/// Maximum identifiers per batch (halved on truncation).
pub const DEFAULT_BATCH_SIZE: usize = 10;
/// The initial call plus ONE retry per identifier.
pub const DEFAULT_MAX_RETRIES_PER_ID: u32 = 2;
const DEFAULT_MAX_FREE_RETRIES: u32 = 100;
/// Minimum bindings before a function's batch splits into lanes.
pub const DEFAULT_LANE_THRESHOLD: usize = 25;

/// `computeLaneCount`.
pub fn compute_lane_count(binding_count: usize, threshold: usize) -> usize {
    if binding_count <= threshold {
        0
    } else if binding_count <= 200 {
        4
    } else if binding_count <= 1000 {
        8
    } else {
        16
    }
}

/// `computeMaxFreeRetries` (no configured override).
fn compute_max_free_retries(binding_count: usize) -> u32 {
    DEFAULT_MAX_FREE_RETRIES.max((binding_count / 4) as u32)
}

/// `splitByPosition`: contiguous chunks of ceil(n / lanes).
pub fn split_by_position(ids: &[String], lanes: usize) -> Vec<Vec<String>> {
    let chunk = ids.len().div_ceil(lanes);
    ids.chunks(chunk.max(1)).map(<[String]>::to_vec).collect()
}

/// Why an identifier's last attempt failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FailureReason {
    Duplicate,
    Invalid,
    Missing,
    Unchanged,
}

/// `IdentifierAttemptState` (the decision fields + the round trail the
/// report carries).
#[derive(Clone, Debug, Default)]
struct IdState {
    attempts: u32,
    free_retries: u32,
    last_suggestion: Option<String>,
    last_failure: Option<FailureReason>,
    trail: Option<Vec<RoundAttempt>>,
}

impl IdState {
    /// `recordAttempt`: the round is the attempt's position.
    fn record(&mut self, proposed: Option<&str>, result: AttemptResult) {
        let trail = self.trail.get_or_insert_with(Vec::new);
        trail.push(RoundAttempt {
            round: trail.len() as u64 + 1,
            proposed: proposed.map(str::to_string),
            result,
        });
    }
}

/// What a finished lane reports (`runBatchRenameLoop`'s result).
#[derive(Clone, Debug, Default)]
pub struct LaneReport {
    pub outcomes: Outcomes,
    pub finish_reasons: Vec<Option<String>>,
    /// The identifiers still unrenamed after the resolution tail.
    pub remaining: Vec<String>,
    /// `resolveRemaining`'s collision decorations (contention events).
    pub contention: Vec<ContentionEvent>,
}

/// What a lane collects for the barrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaneEffect {
    /// `applyRename(old, new)` — a deferred wave rename.
    Rename { old: String, new: String },
    /// `onUnrenamed(name)` — a deferred identity record.
    Identity { name: String },
}

/// A strategy's `transformSuggestion(oldName, suggestion)`.
pub type Transform<'e> = dyn Fn(&str, &str) -> String + 'e;

/// The frozen reads a lane makes (the callbacks' non-mutating half).
pub struct LaneEnv<'e> {
    /// Membership in the lane's base used-name set (the phase context's
    /// used identifiers + the module-level used names, frozen this round).
    pub used: &'e dyn Fn(&str) -> bool,
    /// `wouldReject`: the full scope-safety check.
    pub would_reject: &'e dyn Fn(&str, &str) -> bool,
    /// `transformSuggestion` (the prior-name snap), when the strategy has
    /// one.
    pub transform: Option<&'e Transform<'e>>,
}

/// One call the lane wants made.
#[derive(Clone, Debug)]
pub struct LaneCall {
    pub batch: Vec<String>,
    /// 1 for a first-round request, 2 for a retry-shaped one.
    pub round: u8,
    pub prev: JsRecord,
    pub failures: RenameFailures,
}

#[derive(Clone, Debug)]
enum Stage {
    /// Take the next window from the queue.
    NextWindow,
    /// Inside a window: the identifiers of the next call.
    Window(Vec<String>),
    /// The straggler batches.
    Straggler {
        list: Vec<String>,
        next: usize,
    },
    Done,
}

/// The pending call's bookkeeping.
#[derive(Clone, Debug)]
struct Pending {
    batch: Vec<String>,
    straggler: bool,
    claimed_before: HashSet<String>,
}

/// One lane's loop.
pub struct Lane {
    names: Vec<String>,
    states: HashMap<String, IdState>,
    queue: VecDeque<String>,
    exhausted: Vec<String>,
    /// `outcomes[name]` set (renamed during the loop).
    renamed: HashSet<String>,
    adaptive: usize,
    max_retries: u32,
    max_free: u32,
    claimed: HashSet<String>,
    stage: Stage,
    pending: Option<Pending>,
    /// `finishReasons.length`.
    pub calls: usize,
    /// Whether the strategy records identity outcomes (function nodes).
    identity: bool,
    pub effects: Vec<LaneEffect>,
    finished: bool,
    /// `totalLLMCalls` during the loop: every window call ATTEMPTED (a
    /// provider throw counts) — the straggler's round reads it.
    attempted_calls: u64,
    pub report: LaneReport,
}

impl Lane {
    pub fn new(names: Vec<String>, identity: bool) -> Lane {
        let states = names
            .iter()
            .map(|n| (n.clone(), IdState::default()))
            .collect();
        let max_free = compute_max_free_retries(names.len());
        Lane {
            queue: names.iter().cloned().collect(),
            names,
            states,
            exhausted: Vec::new(),
            renamed: HashSet::new(),
            adaptive: DEFAULT_BATCH_SIZE,
            max_retries: DEFAULT_MAX_RETRIES_PER_ID,
            max_free,
            claimed: HashSet::new(),
            stage: Stage::NextWindow,
            pending: None,
            calls: 0,
            identity,
            effects: Vec::new(),
            finished: false,
            attempted_calls: 0,
            report: LaneReport::default(),
        }
    }

    /// The lane has run its whole loop (including the resolution tail).
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn prev_and_failures(&self, batch: &[String]) -> (JsRecord, RenameFailures) {
        let mut prev = JsRecord::default();
        let mut f = RenameFailures::default();
        for name in batch {
            let s = &self.states[name];
            if let Some(sug) = &s.last_suggestion {
                prev.set(name, sug);
            }
            match s.last_failure {
                Some(FailureReason::Duplicate) => f.duplicates.push(name.clone()),
                Some(FailureReason::Invalid) => f.invalid.push(name.clone()),
                Some(FailureReason::Missing) => f.missing.push(name.clone()),
                Some(FailureReason::Unchanged) => f.unchanged.push(name.clone()),
                None => {}
            }
        }
        (prev, f)
    }

    /// The next call, or None when the loop needs no more calls (then
    /// [`Lane::finish`] runs the resolution tail).
    pub fn next_call(&mut self) -> Option<LaneCall> {
        loop {
            match &self.stage {
                Stage::Done => return None,
                Stage::NextWindow => {
                    if self.queue.is_empty() {
                        let list: Vec<String> = self
                            .exhausted
                            .iter()
                            .filter(|n| {
                                !self.renamed.contains(*n)
                                    && self.states[*n].last_suggestion.is_none()
                            })
                            .cloned()
                            .collect();
                        self.stage = if list.is_empty() {
                            Stage::Done
                        } else {
                            Stage::Straggler { list, next: 0 }
                        };
                        continue;
                    }
                    let take = self.adaptive.min(self.queue.len());
                    let batch: Vec<String> = self.queue.drain(..take).collect();
                    self.stage = Stage::Window(batch);
                }
                Stage::Window(batch) => {
                    let batch = batch.clone();
                    let (prev, failures) = self.prev_and_failures(&batch);
                    let round = if prev.is_empty() { 1 } else { 2 };
                    self.pending = Some(Pending {
                        batch: batch.clone(),
                        straggler: false,
                        claimed_before: self.claimed.clone(),
                    });
                    return Some(LaneCall {
                        batch,
                        round,
                        prev,
                        failures,
                    });
                }
                Stage::Straggler { list, next } => {
                    if *next >= list.len() {
                        self.stage = Stage::Done;
                        continue;
                    }
                    let end = (*next + self.adaptive).min(list.len());
                    let batch = list[*next..end].to_vec();
                    let (prev, failures) = self.prev_and_failures(&batch);
                    self.pending = Some(Pending {
                        batch: batch.clone(),
                        straggler: true,
                        claimed_before: self.claimed.clone(),
                    });
                    return Some(LaneCall {
                        batch,
                        round: 2,
                        prev,
                        failures,
                    });
                }
            }
        }
    }

    /// Consume the pending call's response (`Err` = the provider threw).
    pub fn feed(&mut self, response: Result<(Renames, Option<String>), ()>, env: &LaneEnv<'_>) {
        let pending = self.pending.take().expect("feed without a pending call");
        if pending.straggler {
            self.feed_straggler(&pending, response, env);
        } else {
            self.feed_window(pending, response, env);
        }
    }

    fn feed_window(
        &mut self,
        pending: Pending,
        response: Result<(Renames, Option<String>), ()>,
        env: &LaneEnv<'_>,
    ) {
        let batch = pending.batch;
        self.attempted_calls += 1;
        let Ok((raw, finish)) = response else {
            self.exhausted.extend(batch);
            self.stage = Stage::NextWindow;
            return;
        };
        self.calls += 1;
        // `callNum`: this call's 1-based position among the answered ones.
        let round = self.report.finish_reasons.len() as u64 + 1;
        let renames = match env.transform {
            Some(t) => transform(&raw, t),
            None => raw,
        };
        if finish.as_deref() == Some("length") && self.adaptive > 2 {
            self.adaptive = 2.max(self.adaptive / 2);
        }
        self.report.finish_reasons.push(finish);
        let mut v = self.validate(&renames, &batch, env);
        let (applied, late) = self.apply_valid(&v, env, round);
        v.duplicates.extend(late);
        let (next, exhausted) = self.classify(&batch, &v, &renames, &pending.claimed_before, env);
        self.exhausted.extend(exhausted);
        if applied == 0 && next.len() == batch.len() {
            self.exhausted.extend(next);
            self.stage = Stage::NextWindow;
        } else if next.is_empty() {
            self.stage = Stage::NextWindow;
        } else {
            self.stage = Stage::Window(next);
        }
    }

    fn feed_straggler(
        &mut self,
        pending: &Pending,
        response: Result<(Renames, Option<String>), ()>,
        env: &LaneEnv<'_>,
    ) {
        if let Stage::Straggler { next, .. } = &mut self.stage {
            *next += pending.batch.len().max(1);
        }
        // `callNum = priorLLMCalls + finishReasons.length + 1`: the window
        // calls are counted TWICE (attempted, then answered) — the TS's.
        let round = self.attempted_calls + self.report.finish_reasons.len() as u64 + 1;
        let Ok((renames, finish)) = response else {
            return;
        };
        self.calls += 1;
        self.report.finish_reasons.push(finish);
        let v = self.validate(&renames, &pending.batch, env);
        self.apply_valid(&v, env, round);
        for name in &pending.batch {
            if let Some(s) = renames.get(name).filter(|s| !s.is_empty())
                && let Some(state) = self.states.get_mut(name)
            {
                state.last_suggestion = Some(s.to_string());
            }
        }
    }

    fn is_used(&self, name: &str, env: &LaneEnv<'_>) -> bool {
        (env.used)(name) || self.claimed.contains(name)
    }

    /// `validateBatchRenames`.
    fn validate(&self, renames: &Renames, batch: &[String], env: &LaneEnv<'_>) -> Validation {
        let expected: HashSet<&str> = batch.iter().map(String::as_str).collect();
        let mut v = Validation::default();
        let mut seen: HashSet<String> = HashSet::new();
        for (old, new) in renames.entries() {
            if !expected.contains(old.as_str()) {
                continue;
            }
            let Some(new) = new else {
                v.invalid.push(old.clone());
                continue;
            };
            if old == new {
                v.unchanged.push(old.clone());
            } else if !is_valid_rename_target(new) {
                v.invalid.push(old.clone());
            } else if seen.contains(new) {
                if let Some(i) = v.valid.iter().position(|(_, n)| n == new) {
                    let (k, _) = v.valid.remove(i);
                    v.duplicates.push(k);
                }
                v.duplicates.push(old.clone());
            } else if self.is_used(new, env) {
                v.duplicates.push(old.clone());
            } else {
                v.set_valid(old, new);
                seen.insert(new.clone());
            }
        }
        v
    }

    /// `applyValidRenames`: the check-and-claim.
    fn apply_valid(
        &mut self,
        v: &Validation,
        env: &LaneEnv<'_>,
        round: u64,
    ) -> (usize, Vec<String>) {
        let mut applied = 0;
        let mut late = Vec::new();
        for (old, new) in &v.valid {
            if self.is_used(new, env) || (env.would_reject)(old, new) {
                late.push(old.clone());
                continue;
            }
            self.claim(old, new);
            let trail = self.states.get_mut(old).and_then(|s| {
                s.record(Some(new), AttemptResult::Applied);
                s.trail.clone()
            });
            self.report
                .outcomes
                .set(old, IdentifierOutcome::renamed(new, round, trail));
            applied += 1;
        }
        (applied, late)
    }

    fn claim(&mut self, old: &str, new: &str) {
        self.claimed.insert(new.to_string());
        self.effects.push(LaneEffect::Rename {
            old: old.to_string(),
            new: new.to_string(),
        });
        self.renamed.insert(old.to_string());
    }

    /// `classifyFailedIdentifiers`.
    fn classify(
        &mut self,
        batch: &[String],
        v: &Validation,
        renames: &Renames,
        claimed_before: &HashSet<String>,
        env: &LaneEnv<'_>,
    ) -> (Vec<String>, Vec<String>) {
        let successes: HashSet<&str> = v.valid.iter().map(|(k, _)| k.as_str()).collect();
        let dup: HashSet<&str> = v.duplicates.iter().map(String::as_str).collect();
        let inv: HashSet<&str> = v.invalid.iter().map(String::as_str).collect();
        let unch: HashSet<&str> = v.unchanged.iter().map(String::as_str).collect();
        let mut next = Vec::new();
        let mut exhausted = Vec::new();
        for name in batch {
            if successes.contains(name.as_str()) {
                continue;
            }
            let suggestion = renames.get(name).filter(|s| !s.is_empty());
            let is_free = dup.contains(name.as_str()) && {
                let s = sanitize_identifier(renames.get(name).unwrap_or(""));
                // `sanitizeIdentifier(x || "")` is never empty ("_unnamed").
                let now = self.is_used(&s, env);
                let before = (env.used)(&s) || claimed_before.contains(&s);
                if now && !before {
                    let state = self.states.get_mut(name).expect("state");
                    state.free_retries += 1;
                    state.free_retries < self.max_free
                } else {
                    false
                }
            };
            let state = self.states.get_mut(name).expect("state");
            if let Some(s) = suggestion {
                state.last_suggestion = Some(s.to_string());
            }
            let result = if dup.contains(name.as_str()) {
                AttemptResult::Duplicate
            } else if inv.contains(name.as_str()) {
                AttemptResult::Invalid
            } else if unch.contains(name.as_str()) {
                AttemptResult::Unchanged
            } else {
                AttemptResult::Missing
            };
            state.record(renames.get(name), result);
            if !is_free {
                state.last_failure = Some(if dup.contains(name.as_str()) {
                    FailureReason::Duplicate
                } else if inv.contains(name.as_str()) {
                    FailureReason::Invalid
                } else if unch.contains(name.as_str()) {
                    FailureReason::Unchanged
                } else {
                    FailureReason::Missing
                });
                state.attempts += 1;
                if state.attempts < self.max_retries {
                    next.push(name.clone());
                } else {
                    exhausted.push(name.clone());
                }
            } else if state.free_retries >= 2 && state.last_suggestion.is_some() {
                exhausted.push(name.clone());
            } else {
                next.push(name.clone());
            }
        }
        (next, exhausted)
    }

    /// The loop's tail after the last call: resolve the remaining
    /// identifiers from their last suggestions (`resolveRemaining`, over a
    /// used-name SNAPSHOT), then record identity outcomes for the rest.
    pub fn finish(&mut self, env: &LaneEnv<'_>) {
        debug_assert!(matches!(self.stage, Stage::Done));
        let remaining: Vec<String> = self
            .names
            .iter()
            .filter(|n| !self.renamed.contains(*n))
            .cloned()
            .collect();
        let remaining = unique(remaining);
        let prev: HashMap<String, String> = remaining
            .iter()
            .filter_map(|n| {
                self.states[n]
                    .last_suggestion
                    .clone()
                    .map(|s| (n.clone(), s))
            })
            .collect();
        let snapshot = self.claimed.clone();
        let snap_used = |n: &str| (env.used)(n) || snapshot.contains(n);
        // `resolveOneRemaining`'s round: the answered calls + 1.
        let round = self.report.finish_reasons.len() as u64 + 1;
        let mut left: Vec<String> = Vec::new();
        for name in &remaining {
            let Some(suggested) = prev.get(name) else {
                left.push(name.clone());
                continue;
            };
            if !is_valid_rename_target(suggested) || suggested == name {
                left.push(name.clone());
                continue;
            }
            let scope_rejected = (env.would_reject)(name, suggested);
            if !snap_used(suggested) && !scope_rejected {
                self.claim(name, suggested);
                self.report
                    .outcomes
                    .set(name, IdentifierOutcome::renamed(suggested, round, None));
                continue;
            }
            let resolved = resolve_conflict(suggested, snap_used);
            if (env.would_reject)(name, &resolved) {
                left.push(name.clone());
                continue;
            }
            // Scope-unsafe repairs are not contention (nobody HOLDS the
            // name); only a genuine collision counts.
            if !scope_rejected {
                self.report.contention.push(ContentionEvent {
                    requested: suggested.clone(),
                    resolved_to: resolved.clone(),
                    old_name: name.clone(),
                    site: "remaining",
                });
            }
            self.claim(name, &resolved);
            self.report
                .outcomes
                .set(name, IdentifierOutcome::renamed(&resolved, round, None));
        }
        let last_finish = self.report.finish_reasons.last().cloned().flatten();
        for name in &left {
            if self.identity {
                self.effects
                    .push(LaneEffect::Identity { name: name.clone() });
            }
            let outcome = unrenamed_outcome(&self.states[name], last_finish.clone());
            self.report.outcomes.set(name, outcome);
        }
        self.report.remaining = left;
        self.finished = true;
    }
}

/// `buildUnrenamedOutcome`: by the last failure; `attempts` counts one
/// more when any free retry happened.
fn unrenamed_outcome(state: &IdState, last_finish: Option<String>) -> IdentifierOutcome {
    let attempts = u64::from(state.attempts) + u64::from(state.free_retries > 0);
    let suggestion = state.last_suggestion.clone();
    let status = match state.last_failure {
        Some(FailureReason::Duplicate) => Status::Duplicate {
            conflicted_with: suggestion.clone().unwrap_or_else(|| "unknown".to_string()),
            attempts,
            suggestion,
        },
        Some(FailureReason::Invalid) => Status::Invalid {
            attempts,
            suggestion,
        },
        Some(FailureReason::Unchanged) => Status::Unchanged {
            attempts,
            suggestion,
        },
        _ => Status::Missing {
            attempts,
            last_finish_reason: last_finish,
        },
    };
    IdentifierOutcome {
        status,
        trail: state.trail.clone(),
    }
}

/// First occurrence of each name, in order (a JS Set over the list).
fn unique(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|n| seen.insert(n.clone()))
        .collect()
}

/// `transformSuggestion` over every entry, keys and order kept.
fn transform(renames: &Renames, t: &Transform<'_>) -> Renames {
    Renames::from_entries(
        renames
            .entries()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_ref().map(|s| t(k, s)))),
    )
}

/// `BatchValidationResult` (the decision fields).
#[derive(Clone, Debug, Default)]
struct Validation {
    valid: Vec<(String, String)>,
    duplicates: Vec<String>,
    invalid: Vec<String>,
    unchanged: Vec<String>,
}

impl Validation {
    fn set_valid(&mut self, old: &str, new: &str) {
        match self.valid.iter_mut().find(|(k, _)| k == old) {
            Some(e) => e.1 = new.to_string(),
            None => self.valid.push((old.to_string(), new.to_string())),
        }
    }
}

#[cfg(test)]
mod batch_test;
