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

use humanify_model::llm::{PriorRejects, RenameFailures, Renames};

use super::jsset::JsRecord;
use crate::naming::reask::{ReaskClass, class_of, ladder};
use crate::naming::report::{
    AttemptResult, ContentionEvent, IdentifierOutcome, Outcomes, RoundAttempt, Status,
};
use crate::naming::validation::sanitize_identifier;
use crate::rename::floor::is_minified_echo;
use crate::rename::name_profile::NameProfile;
use crate::rename::validated::RejectionReason;
use crate::rename::validated::target::is_valid_rename_target;

/// Maximum identifiers per batch (halved on truncation). 25 since the
/// 2026-09-28 relaxed-default flip (docs/rust-port/20-fast-mode.md
/// §defaults); the conservative `--sequential` schedule recovers the old
/// 10 explicitly with `--batch-size 10` — the e2e gate pins that against
/// the committed legacy goldens (test/golden/legacy-default/).
pub const DEFAULT_BATCH_SIZE: usize = 25;
/// The initial call plus ONE retry per identifier.
pub const DEFAULT_MAX_RETRIES_PER_ID: u32 = 2;
const DEFAULT_MAX_FREE_RETRIES: u32 = 100;
/// Minimum bindings before a function's batch splits into lanes.
pub const DEFAULT_LANE_THRESHOLD: usize = 25;

/// The processor's tunables (`--batch-size`, `--max-retries`,
/// `--max-free-retries`, `--lane-threshold`, `--rename-retries`;
/// createRenamePlugin's `batchSize` / `maxRetriesPerIdentifier` /
/// `maxFreeRetries` / `laneThreshold`, plus the Rust-run re-ask budget).
/// The default is the TS's optionless run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveTunables {
    /// The lane's window size (halved on truncation from here).
    pub batch_size: usize,
    /// Real calls per identifier (initial + retries).
    pub max_retries: u32,
    /// The cross-lane collision retry cap; None scales it with the lane
    /// (`computeMaxFreeRetries`).
    pub max_free_retries: Option<u32>,
    /// Bindings a function needs before its batch splits into lanes.
    pub lane_threshold: usize,
    /// The name-conflict re-ask budget (`--rename-retries`): how many
    /// times a barrier/sweep collision rejection may be re-asked with the
    /// failed suggestions disclosed. NOT the lane's per-identifier call
    /// cap (`max_retries`) — a collision re-ask rides its own wave-step
    /// round and consumes no lane attempts.
    pub reask_limit: usize,
}

impl Default for WaveTunables {
    fn default() -> Self {
        WaveTunables {
            batch_size: DEFAULT_BATCH_SIZE,
            max_retries: DEFAULT_MAX_RETRIES_PER_ID,
            max_free_retries: None,
            lane_threshold: DEFAULT_LANE_THRESHOLD,
            reask_limit: crate::naming::reask::REASK_LIMIT,
        }
    }
}

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

/// `computeMaxFreeRetries`: the configured cap, else scaled to the lane.
fn compute_max_free_retries(binding_count: usize, configured: Option<u32>) -> u32 {
    configured.unwrap_or_else(|| DEFAULT_MAX_FREE_RETRIES.max((binding_count / 4) as u32))
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
    /// The scope-safety check's reason when THIS round's suggestion was
    /// claim-rejected (the `late` flow) — cleared per round, carried on the
    /// retry call for the ask trace. Recording only: the retry still rides
    /// the duplicate failure preamble, so the flow is byte-unchanged.
    last_rejection: Option<RejectionReason>,
    /// The model's OWN last word for this id, before the strategy's
    /// transform (the prior-name snap) — what a later disclosure must
    /// name when the applied name differs (Fix B, 2026-10-03).
    last_raw: Option<String>,
    /// The all-failed rule exhausted this id's window before its round-2
    /// (2026-10-04): a colliding answer here was never told it collided.
    cut_off: bool,
    /// The model's OWN answers that collided (a duplicate failure, free or
    /// not), oldest first — every round's, for the retry's disclosure and
    /// do-not list (2026-10-06).
    refused: Vec<String>,
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
    /// Suggestions the scope-safety check rejected at claim time that
    /// flowed into the retry lane (the repaired `late` drop — 2026-09-28).
    pub late_rejections: usize,
    /// Resolution-tail finishes whose last suggestion was invalid and got
    /// sanitized instead of identity.
    pub invalid_suggestion_finishes: usize,
    /// Windows whose every id failed (exhausted on the spot — the
    /// all-failed rule; an INVALID failure there now gets the one
    /// feedback straggler).
    pub all_failed_windows: usize,
    /// Colliding answers of all-failed windows handed to the barrier
    /// undecorated, for its disclosed re-ask (2026-10-04).
    pub collision_handoffs: usize,
    /// Answers whose lane round-2 collided AGAIN, handed to the barrier
    /// undecorated for its disclosed re-ask instead of the tail's suffix
    /// ladder (2026-10-06, Andrew: the silent `validatePathVal`).
    pub lane_end_handoffs: usize,
}

/// What a lane collects for the barrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaneEffect {
    /// `applyRename(old, new)` — a deferred wave rename.
    Rename { old: String, new: String },
    /// `onUnrenamed(name)` — a deferred identity record.
    Identity { name: String },
    /// The lane's last answer for `name` was the name itself and the name
    /// is a multi-letter minifier token (`rename::floor::is_minified_echo`):
    /// a refused answer the barrier re-asks — never an identity keep.
    Echo { name: String },
}

/// A strategy's `transformSuggestion(oldName, suggestion)`.
pub type Transform<'e> = dyn Fn(&str, &str) -> String + 'e;

/// The frozen reads a lane makes (the callbacks' non-mutating half).
pub struct LaneEnv<'e> {
    /// Membership in the lane's base used-name set (the phase context's
    /// used identifiers + the module-level used names, frozen this round).
    pub used: &'e dyn Fn(&str) -> bool,
    /// `wouldReject`: the full scope-safety check — as the REASON it would
    /// reject (None = the rename is safe; the lane bool-checks with
    /// `.is_some()`). The reason is recorded on the claim-rejected id and
    /// travels with its retry call (the 2026-09-29 reason thread), so the
    /// ask trace can name the class instead of the generic duplicate
    /// derivation; nothing in the flow reads it.
    pub would_reject: &'e dyn Fn(&str, &str) -> Option<RejectionReason>,
    /// `transformSuggestion` (the prior-name snap), when the strategy has
    /// one.
    pub transform: Option<&'e Transform<'e>>,
    /// The run's minifier name profile (the echo refusal's token shape).
    pub name_profile: NameProfile,
}

/// One call the lane wants made.
#[derive(Clone, Debug)]
pub struct LaneCall {
    pub batch: Vec<String>,
    /// 1 for a first-round request, 2 for a retry-shaped one.
    pub round: u8,
    pub prev: JsRecord,
    pub failures: RenameFailures,
    /// The call's per-id scope-safety rejections of the PREVIOUS round's
    /// suggestions (`(id, reason)` in batch order) — nonempty only for a
    /// retry seeded by the `late` flow. Carries the ask trace's true cause;
    /// the request's failure lists (the prompt bytes) are unaffected.
    pub rejections: Vec<(String, RejectionReason)>,
    /// The ACCUMULATED rejected suggestions per id (the barrier re-ask's
    /// disclosure; see [`PriorRejects`]) — None on every lane-driven call,
    /// set only by the barrier's retry seed. A lane round-2 carries its
    /// own history in `refused` instead (the processor adds the holders).
    pub prior_rejects: Option<PriorRejects>,
    /// The lane's own colliding answers per asked id, oldest first (ids
    /// with none are absent) — the lane round-2's disclosure (2026-10-06).
    pub refused: Vec<(String, Vec<String>)>,
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
    /// old name → the model's own word, for every claimed rename whose
    /// applied name differs from it (a prior-name snap, or the
    /// resolution tail's decoration): a barrier re-ask discloses THIS
    /// word, not ours — disclosing `requestTimeoutMsVal` when the model
    /// said `requestTimeoutMs` invited it to re-offer the same word.
    pub proposed: HashMap<String, String>,
    /// id → the model's own answer KEY, for every id whose latest answer
    /// the answer-key owner matched tolerantly (`naming::answer_keys`,
    /// finding #85: `y$$_` answering `y$_`) — the barrier's trail row
    /// records it.
    pub answer_keys: HashMap<String, String>,
    /// id → the latest answer's STRAY keys (keys that belonged to no asked
    /// id), for every id that answer left unanswered: its round-2
    /// discloses them beside the MISSING line.
    stray_for: HashMap<String, Vec<String>>,
    /// Whether a re-ask budget exists (`--rename-retries` > 0): a colliding
    /// answer the all-failed rule cut off goes to the barrier's disclosed
    /// re-ask instead of the tail's ladder (2026-10-04).
    handoff: bool,
    /// old name → the lane's EARLIER refused answers of every id handed to
    /// the barrier (its last answer travels as the entry's own word): the
    /// barrier's re-ask discloses them first (2026-10-06).
    pub earlier: HashMap<String, Vec<String>>,
}

impl Lane {
    /// The lane under the run's tunables (window size, retry caps).
    pub fn tuned(mut self, t: &WaveTunables) -> Lane {
        self.adaptive = t.batch_size;
        self.max_retries = t.max_retries;
        self.max_free = compute_max_free_retries(self.names.len(), t.max_free_retries);
        self.handoff = t.reask_limit > 0;
        self
    }

    pub fn new(names: Vec<String>, identity: bool) -> Lane {
        let states = names
            .iter()
            .map(|n| (n.clone(), IdState::default()))
            .collect();
        let max_free = compute_max_free_retries(names.len(), None);
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
            proposed: HashMap::new(),
            answer_keys: HashMap::new(),
            stray_for: HashMap::new(),
            handoff: crate::naming::reask::REASK_LIMIT > 0,
            earlier: HashMap::new(),
        }
    }

    /// The lane has run its whole loop (including the resolution tail).
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The lane's own colliding answers per id of `batch` (ids with none
    /// absent), oldest first.
    fn refused_of(&self, batch: &[String]) -> Vec<(String, Vec<String>)> {
        batch
            .iter()
            .filter_map(|n| {
                let words = &self.states[n].refused;
                (!words.is_empty()).then(|| (n.clone(), words.clone()))
            })
            .collect()
    }

    fn prev_and_failures(
        &self,
        batch: &[String],
    ) -> (JsRecord, RenameFailures, Vec<(String, RejectionReason)>) {
        let mut prev = JsRecord::default();
        let mut f = RenameFailures::default();
        let mut rejections = Vec::new();
        for name in batch {
            let s = &self.states[name];
            // The disclosure is the MODEL's own word (finding #73's module
            // leftover, 2026-10-03): the prior-name snap turned
            // `setupApplication` into the taken prior `setupApplication12`,
            // and the round-2 showed the snap. `last_suggestion` keeps the
            // transformed word — the resolution tail applies THAT.
            if let Some(sug) = s.last_suggestion.as_ref().and(s.last_raw.as_ref()) {
                prev.set(name, sug);
            }
            match s.last_failure {
                Some(FailureReason::Duplicate) => f.duplicates.push(name.clone()),
                Some(FailureReason::Invalid) => f.invalid.push(name.clone()),
                Some(FailureReason::Missing) => {
                    f.missing.push(name.clone());
                    // The answer that left it unanswered used keys that
                    // belong to no asked id: name them (finding #85).
                    for key in self.stray_for.get(name).into_iter().flatten() {
                        if !f.stray_keys.contains(key) {
                            f.stray_keys.push(key.clone());
                        }
                    }
                }
                Some(FailureReason::Unchanged) => f.unchanged.push(name.clone()),
                None => {}
            }
            if let Some(reason) = s.last_rejection {
                rejections.push((name.clone(), reason));
            }
        }
        (prev, f, rejections)
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
                            .filter(|n| !self.renamed.contains(*n) && self.straggler_eligible(n))
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
                    let (prev, failures, rejections) = self.prev_and_failures(&batch);
                    let round = if prev.is_empty() { 1 } else { 2 };
                    self.pending = Some(Pending {
                        batch: batch.clone(),
                        straggler: false,
                        claimed_before: self.claimed.clone(),
                    });
                    let refused = self.refused_of(&batch);
                    return Some(LaneCall {
                        batch,
                        round,
                        prev,
                        failures,
                        rejections,
                        prior_rejects: None,
                        refused,
                    });
                }
                Stage::Straggler { list, next } => {
                    if *next >= list.len() {
                        self.stage = Stage::Done;
                        continue;
                    }
                    let end = (*next + self.adaptive).min(list.len());
                    let batch = list[*next..end].to_vec();
                    let (prev, failures, rejections) = self.prev_and_failures(&batch);
                    self.pending = Some(Pending {
                        batch: batch.clone(),
                        straggler: true,
                        claimed_before: self.claimed.clone(),
                    });
                    let refused = self.refused_of(&batch);
                    return Some(LaneCall {
                        batch,
                        round: 2,
                        prev,
                        failures,
                        rejections,
                        prior_rejects: None,
                        refused,
                    });
                }
            }
        }
    }

    /// Consume the pending call's response (`Err` = the provider threw).
    pub fn feed(&mut self, response: Result<(Renames, Option<String>), ()>, env: &LaneEnv<'_>) {
        let pending = self.pending.take().expect("feed without a pending call");
        // A fresh round: each id's `last_rejection` describes THIS round's
        // claim rejection or nothing (apply_valid re-records below), so a
        // later genuine duplicate cannot inherit a stale reason.
        for name in &pending.batch {
            if let Some(s) = self.states.get_mut(name) {
                s.last_rejection = None;
            }
        }
        let response =
            response.map(|(raw, finish)| (self.key_answer(&raw, &pending.batch, env), finish));
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
        self.record_raw(&raw, &batch);
        let renames = match env.transform {
            Some(t) => transform(&raw, t),
            None => raw,
        };
        if finish.as_deref() == Some("length") && self.adaptive > 2 {
            self.adaptive = 2.max(self.adaptive / 2);
        }
        self.report.finish_reasons.push(finish);
        let mut v = self.validate(&renames, &batch, env);
        let (applied, late) = self.apply_valid(&mut v, env, round);
        let late_rejected = !late.is_empty();
        v.duplicates.extend(late);
        let (next, exhausted) = self.classify(&batch, &v, &renames, &pending.claimed_before, env);
        self.exhausted.extend(exhausted);
        // The all-failed exhaustion (`validThisCall === 0 && nextRetry.length
        // === batchSizeBefore`): burn the window on the spot rather than
        // re-asking — UNLESS the window produced `late` rejections
        // (2026-09-28): those ids were never TOLD their name was taken, so
        // they keep their disclosed round-2, bounded by `max_retries` (the
        // second all-failed round exhausts them through attempts).
        if applied == 0 && next.len() == batch.len() && !late_rejected {
            self.report.all_failed_windows += 1;
            for name in &next {
                if let Some(s) = self.states.get_mut(name) {
                    s.cut_off = true;
                }
            }
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
        self.record_raw(&renames, &pending.batch);
        let mut v = self.validate(&renames, &pending.batch, env);
        self.apply_valid(&mut v, env, round);
        for name in &pending.batch {
            if let Some(s) = renames.get(name).filter(|s| !s.is_empty())
                && let Some(state) = self.states.get_mut(name)
            {
                state.last_suggestion = Some(s.to_string());
            }
        }
    }

    /// Read a response through the ONE answer-key owner
    /// (`naming::answer_keys`, finding #85): a mangled key lands on its one
    /// asked id (remembered in [`Lane::answer_keys`] for the trail); the
    /// ids the answer left unanswered remember its stray keys for their
    /// round-2's disclosure. A key that is a name already in use is never
    /// read as a misspelling.
    fn key_answer(&mut self, raw: &Renames, batch: &[String], env: &LaneEnv<'_>) -> Renames {
        let keyed = crate::naming::answer_keys::key_answer(raw, batch, &|k| self.is_used(k, env));
        for id in batch {
            self.answer_keys.remove(id);
            self.stray_for.remove(id);
            if let Some(key) = keyed.answer_key(id) {
                self.answer_keys.insert(id.clone(), key.to_string());
            } else if !keyed.answered(id) && !keyed.stray.is_empty() {
                self.stray_for.insert(id.clone(), keyed.stray.clone());
            }
        }
        keyed.renames
    }

    fn is_used(&self, name: &str, env: &LaneEnv<'_>) -> bool {
        (env.used)(name) || self.claimed.contains(name)
    }

    /// The straggler pass's admission rule: an id the model never answered
    /// (the TS rule), plus — the 2026-09-28 fix — an INVALID-failed id
    /// whose window died under the all-failed rule BEFORE its round-2
    /// (`attempts < max_retries`): it never got the feedback ask
    /// "that name is not allowed", and the tail can only sanitize it.
    /// An id that exhausted THROUGH round-2 is not re-admitted.
    fn straggler_eligible(&self, name: &str) -> bool {
        let s = &self.states[name];
        s.last_suggestion.is_none()
            || (s.last_failure == Some(FailureReason::Invalid) && s.attempts < self.max_retries)
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

    /// `applyValidRenames`: the check-and-claim. A suggestion the
    /// scope-safety check rejects (`late`) is REMOVED from `valid` — the
    /// 2026-09-28 fix: it used to stay in, and `classify`'s success
    /// short-circuit then skipped the id (no retry, no exhaustion, no last
    /// suggestion — a silent identity settle, log-proven 4/18). Removed,
    /// it flows into the retry lane through `duplicates` with the
    /// duplicate failure preamble, like any other rejected suggestion.
    fn apply_valid(
        &mut self,
        v: &mut Validation,
        env: &LaneEnv<'_>,
        round: u64,
    ) -> (usize, Vec<String>) {
        let mut applied = 0;
        let mut late = Vec::new();
        let mut i = 0;
        while i < v.valid.len() {
            let (old, new) = v.valid[i].clone();
            if self.is_used(&new, env) {
                v.valid.remove(i);
                late.push(old);
                continue;
            }
            if let Some(reason) = (env.would_reject)(&old, &new) {
                // The scope-safety REASON travels with the id into its
                // round-2 (the ask trace names the class); the retry itself
                // still rides the duplicate preamble — flow unchanged.
                if let Some(s) = self.states.get_mut(&old) {
                    s.last_rejection = Some(reason);
                }
                v.valid.remove(i);
                late.push(old);
                continue;
            }
            self.claim(&old, &new);
            let trail = self.states.get_mut(&old).and_then(|s| {
                s.record(Some(&new), AttemptResult::Applied);
                s.trail.clone()
            });
            self.report
                .outcomes
                .set(&old, IdentifierOutcome::renamed(&new, round, trail));
            applied += 1;
            i += 1;
        }
        if !late.is_empty() {
            self.report.late_rejections += late.len();
        }
        (applied, late)
    }

    /// The model's own words for this call's ids (before any transform).
    fn record_raw(&mut self, raw: &Renames, batch: &[String]) {
        for name in batch {
            if let Some(w) = raw.get(name).filter(|w| !w.is_empty())
                && let Some(s) = self.states.get_mut(name)
            {
                s.last_raw = Some(w.to_string());
            }
        }
    }

    fn claim(&mut self, old: &str, new: &str) {
        if let Some(w) = self
            .states
            .get(old)
            .and_then(|s| s.last_raw.as_deref())
            .filter(|w| *w != new)
        {
            self.proposed.insert(old.to_string(), w.to_string());
        }
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
            // The MODEL's word (before any transform) joins the id's
            // refused history when it collided.
            if dup.contains(name.as_str())
                && let Some(w) = state.last_raw.clone()
                && !state.refused.contains(&w)
            {
                state.refused.push(w);
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

    /// A colliding last answer goes to the barrier's disclosed re-ask, not
    /// the tail's suffix ladder. Two shapes: the lone-collision gap
    /// (finding #74's open item, 2026-10-04 — an answer the all-failed rule
    /// cut off before its round-2, never TOLD it collided) and, since
    /// 2026-10-06 (Andrew: the silent `validatePathVal`, ~2,420 names per
    /// fresh run), an answer whose lane round-2 collided AGAIN. With a
    /// re-ask budget the tail hands the model's answer to the barrier
    /// UNDECORATED: the barrier sees the collision and gives it the
    /// disclosed re-ask every other conflict gets (`--rename-retries`,
    /// accumulating do-not list led by every answer the model already gave
    /// — the lane's earlier ones travel in [`Lane::earlier`] — the holder
    /// named); the barrier's ladder is the last resort once that budget is
    /// spent. Never for a rejection no name escapes (`rejection` of another
    /// class): nothing to re-ask.
    fn hands_off(&self, name: &str, rejection: Option<RejectionReason>) -> bool {
        let s = &self.states[name];
        self.handoff
            && s.last_failure == Some(FailureReason::Duplicate)
            && rejection.is_none_or(|r| class_of(r) == ReaskClass::NameTaken)
    }

    /// The tail's claim of `suggested` for `name` — applied directly, or
    /// (`handed`) handed to the barrier undecorated.
    fn land(&mut self, name: &str, suggested: &str, round: u64, handed: bool) {
        if handed {
            self.record_handoff(name, suggested);
        }
        self.claim(name, suggested);
        self.report
            .outcomes
            .set(name, IdentifierOutcome::renamed(suggested, round, None));
    }

    /// Record a hand-off: which shape it was, and the lane's EARLIER
    /// refused answers (all but the handed-off word — the model's own
    /// last word, else the suggestion — itself) for the barrier's
    /// disclosure.
    fn record_handoff(&mut self, name: &str, suggested: &str) {
        let s = &self.states[name];
        let word = s.last_raw.as_deref().unwrap_or(suggested);
        if s.cut_off {
            self.report.collision_handoffs += 1;
        } else {
            self.report.lane_end_handoffs += 1;
        }
        let earlier: Vec<String> = s.refused.iter().filter(|w| *w != word).cloned().collect();
        if !earlier.is_empty() {
            self.earlier.insert(name.to_string(), earlier);
        }
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
        let mut echoed: HashSet<String> = HashSet::new();
        for name in &remaining {
            let Some(raw) = prev.get(name) else {
                left.push(name.clone());
                continue;
            };
            // An INVALID last suggestion (a reserved word / global builtin,
            // log-proven 4/18) is still evidence: sanitize it —
            // `sanitizeIdentifier` exists for exactly this shape — instead
            // of settling the id as identity. The ladder decides the rest.
            let suggested = if is_valid_rename_target(raw) {
                raw.clone()
            } else {
                self.report.invalid_suggestion_finishes += 1;
                sanitize_identifier(raw)
            };
            if suggested == name.as_str() {
                if is_minified_echo(env.name_profile, name, &suggested) {
                    echoed.insert(name.clone());
                }
                left.push(name.clone());
                continue;
            }
            let rejection = (env.would_reject)(name, &suggested);
            let scope_rejected = rejection.is_some();
            let direct = !snap_used(&suggested) && !scope_rejected;
            let handed = !direct && self.hands_off(name, rejection);
            if direct || handed {
                self.land(name, &suggested, round, handed);
                continue;
            }
            // A rejection no name can escape (`exported-name`, a missing
            // binding) settles the id as identity: there is nothing to try.
            if rejection.is_some_and(|r| class_of(r) != ReaskClass::NameTaken) {
                left.push(name.clone());
                continue;
            }
            // The ladder steps past a decoration the scope check rejects
            // exactly as it steps past a taken one (finding #74,
            // `reask::ladder`): the first scope-safe decoration lands. It
            // used to give up on a scope-unsafe pick and record IDENTITY —
            // a valid answer silently dropped (2.1.215's axios adapter
            // `t`: `requestOptionsVal` was held by a nested callback that
            // reads `t`). The id's OWN name stops the ladder (it keeps it).
            let resolved = ladder(name, &suggested, snap_used, |n| (env.would_reject)(name, n));
            if (env.would_reject)(name, &resolved).is_some() {
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
            if echoed.contains(name) {
                // The model handed a multi-letter minified name back as
                // its answer: a REFUSED answer, not a keep — the barrier
                // re-asks it (round 2, 2026-10-03), module lanes too.
                self.effects.push(LaneEffect::Echo { name: name.clone() });
            } else if self.identity {
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
