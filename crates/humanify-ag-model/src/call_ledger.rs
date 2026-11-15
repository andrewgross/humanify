//! The LLM call ledger — the ONE owner of "what happened to each LLM
//! request" (docs/responsibility.md). Every call is filed under its namer
//! ([`CallSite`]) into a fixed, documented set of categories, so a KPI
//! change can be attributed to the server, to the model's answers, or to
//! real failures (exp206, Andrew 2026-11-08).
//!
//! Three views, kept apart because they behave differently across runs:
//!
//! - **outcomes** — what the pipeline got back. Every call is ANSWERED or
//!   FAILED (after every retry). Each answered call's raw answer gets one
//!   verdict per asked identifier ([`AnswerVerdicts`]). With the model held
//!   fixed (a warm cache) these counts are byte-identical run to run: a
//!   served answer is the same answer.
//! - **transport** — how the answers arrived: served from the run's memo
//!   or the disk cache without a request, or SENT. A sent call is answered
//!   on its first HTTP attempt, answered after retries (each retried
//!   attempt counted by reason), or failed (by the final error's kind).
//!   Tokens live here too. These move with the server and with the cache.
//! - **timing** — latency percentiles per namer: the whole sent call
//!   (retries and backoff included) and each HTTP attempt. Wall clock,
//!   never deterministic.
//!
//! RECORDING ONLY. Nothing in the pipeline reads the ledger back: the
//! verdicts are computed from the request and the raw answer and go
//! nowhere but the `--stats-json` `llmCalls` block (and the coverage
//! block's `retries` / `avgResponseTimeMs` / token fields, which this
//! ledger is the source of). Completion order cannot leak: every view is
//! a sum, and the latency lists are sorted before they are read.
//!
//! The answer verdicts judge the RAW answer, before any pipeline repair:
//! "proposed" means the model offered a usable new name, not that the
//! pipeline kept it — a scope rejection, a re-ask or a ladder decoration
//! after that is the namers' record (the stats `reask` block).

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::llm::{BatchRenameResponse, CallSite, LlmCall, LlmError, LlmErrorKind};
use crate::ordered::OrderedMap;

/// One HTTP attempt's result, as the client saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attempt {
    /// A 2xx with a readable completion.
    Ok,
    /// The per-attempt deadline fired.
    Timeout,
    /// The connection failed (or a 2xx body could not be read).
    Connection,
    /// A non-2xx response.
    Status(u16),
    /// A 2xx whose body is not a completion (unparsable / no `choices`).
    Malformed,
}

/// One HTTP attempt: its result and how long it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttemptRecord {
    pub result: Attempt,
    pub ms: u64,
}

/// Where an answer came from without a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Served {
    /// The run's answer memo (an identical request earlier in this run).
    Memo,
    /// The `--llm-cache` disk cache.
    Disk,
}

/// Why a sent call failed after every retry — the final error's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailKind {
    Timeout,
    Connection,
    /// A non-2xx status the retries did not get past.
    Status,
    /// A 2xx whose body was not a completion.
    Malformed,
    /// A replay-only cache had no entry.
    CacheMiss,
    Other,
}

/// A sent call's fate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentOutcome {
    /// Answered on its first HTTP attempt.
    FirstTry,
    /// Answered after at least one retried attempt.
    AfterRetry,
    /// No answer after every retry.
    Failed(FailKind),
}

/// Classify a sent call from its attempts and its final result.
pub fn classify_sent(
    attempts: &[AttemptRecord],
    result: &Result<BatchRenameResponse, LlmError>,
) -> SentOutcome {
    match result {
        Ok(_) if attempts.len() <= 1 => SentOutcome::FirstTry,
        Ok(_) => SentOutcome::AfterRetry,
        Err(e) => SentOutcome::Failed(match e.kind {
            LlmErrorKind::Timeout => FailKind::Timeout,
            LlmErrorKind::Connection => FailKind::Connection,
            LlmErrorKind::Status => FailKind::Status,
            LlmErrorKind::CacheMiss => FailKind::CacheMiss,
            LlmErrorKind::Other
                if attempts.last().map(|a| a.result) == Some(Attempt::Malformed) =>
            {
                FailKind::Malformed
            }
            LlmErrorKind::Other => FailKind::Other,
        }),
    }
}

/// The verdicts on one raw answer, one per ASKED identifier (exclusive, in
/// this priority: missing, unchanged, invalid, duplicate-in-answer,
/// taken-name, else proposed), plus two counts beside them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerVerdicts {
    /// Identifiers the call asked about.
    pub asked: u64,
    /// A usable new name: legal, not the identifier itself, not given to
    /// another identifier of the same answer, not a name the prompt said
    /// was taken.
    pub proposed: u64,
    /// No answer for the identifier (absent, null or empty) — a vendor
    /// decline and a reviser's "already good" land here too.
    pub missing: u64,
    /// The identifier handed back as its own name.
    pub unchanged: u64,
    /// Not a legal rename target (a reserved word, a builtin, bad chars).
    /// Identifier answers only (the `naming` site): the other namers
    /// answer file, folder and package names, which their own namers
    /// check, so they never land here.
    pub invalid: u64,
    /// The same name given to two or more identifiers of this answer.
    pub duplicate_in_answer: u64,
    /// A name the prompt listed as already in use.
    pub taken_name: u64,
    /// Asked identifiers that carried a prior-version name hint.
    pub hinted: u64,
    /// Of those, the answers that ARE the hint (the model repeating the
    /// prior's name) — beside the exclusive verdicts, not one of them.
    pub equals_prior_hint: u64,
    /// Answer keys that name no asked identifier.
    pub extra_keys: u64,
}

impl AnswerVerdicts {
    fn add(&mut self, o: &AnswerVerdicts) {
        self.asked += o.asked;
        self.proposed += o.proposed;
        self.missing += o.missing;
        self.unchanged += o.unchanged;
        self.invalid += o.invalid;
        self.duplicate_in_answer += o.duplicate_in_answer;
        self.taken_name += o.taken_name;
        self.hinted += o.hinted;
        self.equals_prior_hint += o.equals_prior_hint;
        self.extra_keys += o.extra_keys;
    }
}

/// Is `name` a legal rename target — the name rules' owner's predicate
/// (core `rename::validated::target::is_valid_rename_target`), handed in
/// by the binary because this crate sits below core.
pub type NameJudge = fn(&str) -> bool;

/// Judge one raw answer against the request it answers.
pub fn judge_answer(
    call: &LlmCall,
    response: &BatchRenameResponse,
    is_legal: NameJudge,
) -> AnswerVerdicts {
    let request = &call.request;
    let answer = |id: &str| response.renames.get(id).filter(|n| !n.is_empty());
    let mut v = AnswerVerdicts::default();
    for id in &request.identifiers {
        v.asked += 1;
        let given = answer(id);
        let hint = request
            .prior_name_hints
            .as_ref()
            .and_then(|h| h.0.iter().find(|(k, _)| k == id).map(|(_, n)| n.as_str()));
        if let Some(hint) = hint {
            v.hinted += 1;
            if given == Some(hint) {
                v.equals_prior_hint += 1;
            }
        }
        let Some(name) = given else {
            v.missing += 1;
            continue;
        };
        let shared = request
            .identifiers
            .iter()
            .filter(|other| answer(other) == Some(name))
            .count()
            > 1;
        if name == id {
            v.unchanged += 1;
        } else if call.site == CallSite::Naming && !is_legal(name) {
            v.invalid += 1;
        } else if shared {
            v.duplicate_in_answer += 1;
        } else if request.used_names.iter().any(|u| u == name) {
            v.taken_name += 1;
        } else {
            v.proposed += 1;
        }
    }
    v.extra_keys = response
        .renames
        .entries()
        .iter()
        .filter(|(k, _)| !request.identifiers.iter().any(|id| id == k))
        .count() as u64;
    v
}

/// One namer's running tally.
#[derive(Clone, Debug, Default)]
struct Tally {
    // outcomes
    calls: u64,
    answered: u64,
    failed: u64,
    truncated: u64,
    answers: AnswerVerdicts,
    // transport
    served_memo: u64,
    served_disk: u64,
    sent: u64,
    ok_first_try: u64,
    ok_after_retry: u64,
    failed_by: BTreeMap<&'static str, u64>,
    http_attempts: u64,
    retried_by: BTreeMap<&'static str, u64>,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    // timing
    call_ms: Vec<u64>,
    attempt_ms: Vec<u64>,
}

const FAIL_KEYS: [&str; 6] = [
    "timeout",
    "connection",
    "status",
    "malformed",
    "cacheMiss",
    "other",
];
const RETRY_KEYS: [&str; 6] = [
    "timeout",
    "connection",
    "status429",
    "status5xx",
    "statusOther",
    "malformed",
];

fn fail_key(kind: FailKind) -> &'static str {
    match kind {
        FailKind::Timeout => "timeout",
        FailKind::Connection => "connection",
        FailKind::Status => "status",
        FailKind::Malformed => "malformed",
        FailKind::CacheMiss => "cacheMiss",
        FailKind::Other => "other",
    }
}

/// A retried (non-final) attempt's reason. An `Ok` never precedes another
/// attempt; if one ever did it would be counted nowhere.
fn retry_key(result: Attempt) -> Option<&'static str> {
    match result {
        Attempt::Ok => None,
        Attempt::Timeout => Some("timeout"),
        Attempt::Connection => Some("connection"),
        Attempt::Status(429) => Some("status429"),
        Attempt::Status(s) if (500..600).contains(&s) => Some("status5xx"),
        Attempt::Status(_) => Some("statusOther"),
        Attempt::Malformed => Some("malformed"),
    }
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.calls += o.calls;
        self.answered += o.answered;
        self.failed += o.failed;
        self.truncated += o.truncated;
        self.answers.add(&o.answers);
        self.served_memo += o.served_memo;
        self.served_disk += o.served_disk;
        self.sent += o.sent;
        self.ok_first_try += o.ok_first_try;
        self.ok_after_retry += o.ok_after_retry;
        for (k, n) in &o.failed_by {
            *self.failed_by.entry(k).or_default() += n;
        }
        self.http_attempts += o.http_attempts;
        for (k, n) in &o.retried_by {
            *self.retried_by.entry(k).or_default() += n;
        }
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.total_tokens += o.total_tokens;
        self.call_ms.extend(&o.call_ms);
        self.attempt_ms.extend(&o.attempt_ms);
    }

    fn outcomes(&self) -> Outcomes {
        Outcomes {
            calls: self.calls,
            answered: self.answered,
            failed: self.failed,
            truncated: self.truncated,
            answers: self.answers,
        }
    }

    fn transport(&self) -> Transport {
        let keyed = |keys: &[&'static str], map: &BTreeMap<&'static str, u64>| {
            OrderedMap(
                keys.iter()
                    .map(|k| (k.to_string(), map.get(k).copied().unwrap_or(0)))
                    .collect(),
            )
        };
        Transport {
            served_memo: self.served_memo,
            served_disk: self.served_disk,
            sent: self.sent,
            ok_first_try: self.ok_first_try,
            ok_after_retry: self.ok_after_retry,
            failed: keyed(&FAIL_KEYS, &self.failed_by),
            http_attempts: self.http_attempts,
            retried: keyed(&RETRY_KEYS, &self.retried_by),
            tokens: Tokens {
                input: self.input_tokens,
                output: self.output_tokens,
                total: self.total_tokens,
            },
        }
    }

    fn timing(&self) -> Timing {
        Timing {
            call_ms: percentiles(&self.call_ms),
            attempt_ms: percentiles(&self.attempt_ms),
        }
    }
}

/// One namer's outcomes view.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Outcomes {
    pub calls: u64,
    pub answered: u64,
    pub failed: u64,
    pub truncated: u64,
    pub answers: AnswerVerdicts,
}

/// One namer's transport view: failures by kind and retries by reason,
/// every kind written (zeros included).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transport {
    pub served_memo: u64,
    pub served_disk: u64,
    pub sent: u64,
    pub ok_first_try: u64,
    pub ok_after_retry: u64,
    pub failed: OrderedMap<u64>,
    pub http_attempts: u64,
    pub retried: OrderedMap<u64>,
    pub tokens: Tokens,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

/// One namer's timing view (ms).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub call_ms: Percentiles,
    pub attempt_ms: Percentiles,
}

/// `{n, p50, p90, p99, max, mean}` of a latency list.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Percentiles {
    pub n: u64,
    pub p50: u64,
    pub p90: u64,
    pub p99: u64,
    pub max: u64,
    pub mean: u64,
}

/// The `--stats-json` `llmCalls` block: each view keyed by namer (every
/// namer, zeros included) and then `total`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CallStats {
    pub outcomes: OrderedMap<Outcomes>,
    pub transport: OrderedMap<Transport>,
    pub timing: OrderedMap<Timing>,
}

/// Nearest-rank percentile of a SORTED list.
fn rank(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[idx]
}

/// The percentiles of a latency list, sorted first so they are
/// independent of the order the calls completed in.
fn percentiles(values: &[u64]) -> Percentiles {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mean = if sorted.is_empty() {
        0
    } else {
        (sorted.iter().sum::<u64>() as f64 / sorted.len() as f64).round() as u64
    };
    Percentiles {
        n: sorted.len() as u64,
        p50: rank(&sorted, 0.50),
        p90: rank(&sorted, 0.90),
        p99: rank(&sorted, 0.99),
        max: sorted.last().copied().unwrap_or(0),
        mean,
    }
}

/// The coverage block's LLM fields this ledger is the source of.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CoverageLlm {
    /// Retried HTTP attempts, every layer and every namer.
    pub retries: u64,
    /// Mean wall time of a sent call (retries included), ms.
    pub avg_response_time_ms: f64,
    pub total_tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// The run's ledger. Shared by the provider stack's layers (each records
/// what it alone sees); read once, at the end of the run.
pub struct CallLedger {
    is_legal: NameJudge,
    // concurrency: short-lock — one tally update per event, never held across an await
    tallies: Mutex<BTreeMap<CallSite, Tally>>,
}

impl CallLedger {
    pub fn new(is_legal: NameJudge) -> Self {
        CallLedger {
            is_legal,
            tallies: Mutex::new(BTreeMap::new()),
        }
    }

    fn with(&self, site: CallSite, f: impl FnOnce(&mut Tally)) {
        let mut tallies = self.tallies.lock().expect("the call ledger lock");
        f(tallies.entry(site).or_default());
    }

    /// The outermost layer: every call, whatever answered it.
    pub fn outcome(&self, call: &LlmCall, result: &Result<BatchRenameResponse, LlmError>) {
        // Judged outside the lock; a failed call is tallied below.
        let verdicts = match result {
            Ok(r) => Some(judge_answer(call, r, self.is_legal)),
            Err(_) => None,
        };
        self.with(call.site, |t| {
            t.calls += 1;
            match result {
                Ok(r) => {
                    t.answered += 1;
                    if r.finish_reason.as_deref() == Some("length") {
                        t.truncated += 1;
                    }
                }
                Err(_) => t.failed += 1,
            }
            if let Some(v) = verdicts {
                t.answers.add(&v);
            }
        });
    }

    /// An answer served without a request.
    pub fn served(&self, site: CallSite, from: Served) {
        self.with(site, |t| match from {
            Served::Memo => t.served_memo += 1,
            Served::Disk => t.served_disk += 1,
        });
    }

    /// A call that went to the server: its attempts (every layer's, in
    /// order), its final result and its wall time.
    pub fn sent(
        &self,
        site: CallSite,
        attempts: &[AttemptRecord],
        result: &Result<BatchRenameResponse, LlmError>,
        wall_ms: u64,
    ) {
        let outcome = classify_sent(attempts, result);
        self.with(site, |t| {
            t.sent += 1;
            match outcome {
                SentOutcome::FirstTry => t.ok_first_try += 1,
                SentOutcome::AfterRetry => t.ok_after_retry += 1,
                SentOutcome::Failed(kind) => *t.failed_by.entry(fail_key(kind)).or_default() += 1,
            }
            t.http_attempts += attempts.len() as u64;
            let retried = attempts.len().saturating_sub(1);
            for a in &attempts[..retried] {
                if let Some(k) = retry_key(a.result) {
                    *t.retried_by.entry(k).or_default() += 1;
                }
            }
            if let Ok(r) = result
                && let Some(u) = &r.usage
            {
                t.input_tokens += u.input_tokens.unwrap_or(0);
                t.output_tokens += u.output_tokens.unwrap_or(0);
                t.total_tokens += u.total_tokens.unwrap_or(0);
            }
            t.call_ms.push(wall_ms);
            t.attempt_ms.extend(attempts.iter().map(|a| a.ms));
        });
    }

    /// Calls of `site` that got no answer after every retry (the outcomes
    /// view — what the namer's `providerErrors` count must equal).
    pub fn failed(&self, site: CallSite) -> u64 {
        let tallies = self.tallies.lock().expect("the call ledger lock");
        tallies.get(&site).map_or(0, |t| t.failed)
    }

    fn snapshot(&self) -> (Vec<(CallSite, Tally)>, Tally) {
        let tallies = self.tallies.lock().expect("the call ledger lock");
        let mut total = Tally::default();
        let per: Vec<(CallSite, Tally)> = CallSite::ALL
            .iter()
            .map(|s| {
                let t = tallies.get(s).cloned().unwrap_or_default();
                total.add(&t);
                (*s, t)
            })
            .collect();
        (per, total)
    }

    /// The coverage block's LLM fields.
    pub fn coverage_llm(&self) -> CoverageLlm {
        let (_, total) = self.snapshot();
        let positive = |n: u64| (n > 0).then_some(n);
        let retries: u64 = total.retried_by.values().sum();
        let avg = if total.call_ms.is_empty() {
            0.0
        } else {
            (total.call_ms.iter().sum::<u64>() as f64 / total.call_ms.len() as f64).round()
        };
        CoverageLlm {
            retries,
            avg_response_time_ms: avg,
            total_tokens: positive(total.total_tokens),
            input_tokens: positive(total.input_tokens),
            output_tokens: positive(total.output_tokens),
        }
    }

    /// The `--stats-json` `llmCalls` block: `outcomes`, `transport` and
    /// `timing`, each keyed by namer (every namer, zeros included) plus
    /// `total`.
    pub fn stats(&self) -> CallStats {
        let (per, total) = self.snapshot();
        fn view<T>(per: &[(CallSite, Tally)], total: &Tally, f: fn(&Tally) -> T) -> OrderedMap<T> {
            let mut rows: Vec<(String, T)> = per
                .iter()
                .map(|(s, t)| (s.as_str().to_string(), f(t)))
                .collect();
            rows.push(("total".to_string(), f(total)));
            OrderedMap(rows)
        }
        CallStats {
            outcomes: view(&per, &total, Tally::outcomes),
            transport: view(&per, &total, Tally::transport),
            timing: view(&per, &total, Tally::timing),
        }
    }
}
