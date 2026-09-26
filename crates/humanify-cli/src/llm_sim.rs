//! `--simulate-llm-latency <out.json>`: price a WARM run's LLM schedule as
//! if it were cold (docs/rust-port/20-fast-mode.md).
//!
//! A warm replay answers every call from the cache in microseconds, so it
//! measures the CPU and hides the thing that dominates a cold run: how the
//! naming stage's barriers serialize model latency. This wrapper keeps the
//! real (cached) answers and runs a VIRTUAL clock beside them: every call
//! gets a latency drawn deterministically from the measured cold
//! distribution (keyed by a hash of its prompts), and calls occupy one of
//! `slots` concurrent slots, exactly as the rate limiter would admit them.
//!
//! - `run_wave` is a batch: its calls start in order as slots free up, and
//!   the wave ends when its last call does;
//! - `run_pipelined` is event-driven: a call's follow-ups are queued the
//!   moment it completes in virtual time, and completions are handed to the
//!   driver in VIRTUAL completion order — which also exercises the
//!   pipelined driver's order-independence on real data.
//!
//! What it cannot see: the CPU work between calls (measured for real by
//! the phase profile), server-side queueing effects on latency (the draw is
//! load-independent), and cache misses' real latency (a miss is priced like
//! any call). Observation only — nothing it computes reaches a decision.

use std::collections::{BTreeMap, BinaryHeap, VecDeque};
use std::sync::Mutex;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, OnCallDone};
use sha2::{Digest, Sha256};

/// Cold per-call latency quantiles in ms, p0..p100 — 10,283 unique calls
/// of the four pairs in the last cold Rust eval (`rust-5b-c3b272f-a`
/// -vv logs, gpt-oss-20b at concurrency 32, 2026-09-26).
const LATENCY_MS: [u32; 101] = [
    105, 173, 218, 254, 292, 340, 384, 418, 443, 465, 485, 505, 523, 541, 558, 578, 597, 617, 636,
    655, 672, 691, 709, 727, 743, 764, 778, 793, 808, 824, 841, 855, 869, 885, 899, 914, 929, 945,
    958, 975, 991, 1008, 1022, 1039, 1057, 1073, 1089, 1107, 1124, 1142, 1155, 1174, 1193, 1213,
    1233, 1251, 1270, 1290, 1307, 1327, 1348, 1370, 1386, 1406, 1427, 1447, 1475, 1502, 1524, 1548,
    1571, 1598, 1621, 1650, 1680, 1711, 1747, 1783, 1818, 1849, 1888, 1923, 1972, 2019, 2070, 2116,
    2183, 2251, 2325, 2414, 2524, 2640, 2765, 2912, 3070, 3277, 3479, 3804, 4370, 5293, 20339,
];

/// A call's simulated latency (ms): a deterministic draw from the
/// quantile table, linear between quantiles.
pub fn latency_ms(call: &LlmCall) -> f64 {
    let mut h = Sha256::new();
    h.update(call.system_prompt.as_bytes());
    h.update([0]);
    h.update(call.user_prompt.as_bytes());
    let d = h.finalize();
    let u = u64::from_be_bytes(d[..8].try_into().expect("8 bytes")) as f64 / u64::MAX as f64;
    let x = u * 100.0;
    let i = (x.floor() as usize).min(99);
    let f = x - i as f64;
    f64::from(LATENCY_MS[i]) * (1.0 - f) + f64::from(LATENCY_MS[i + 1]) * f
}

/// What the virtual clock recorded.
#[derive(Default, Debug)]
pub struct SimReport {
    /// Virtual ms elapsed across every dispatch (the LLM-bound wall).
    pub wall_ms: f64,
    pub calls: u64,
    /// Sum of the drawn latencies (call-ms).
    pub busy_ms: f64,
    /// Per prompt kind (the system prompt's first words): (calls, wall ms).
    pub by_kind: BTreeMap<String, (u64, f64)>,
    /// Every dispatch in order: (kind, calls, wall ms).
    pub dispatches: Vec<(String, u64, f64)>,
}

impl SimReport {
    fn charge(&mut self, kind: &str, calls: u64, wall_ms: f64, busy_ms: f64) {
        if calls == 0 {
            return;
        }
        self.dispatches.push((kind.to_string(), calls, wall_ms));
        self.wall_ms += wall_ms;
        self.calls += calls;
        self.busy_ms += busy_ms;
        let e = self.by_kind.entry(kind.to_string()).or_default();
        e.0 += calls;
        e.1 += wall_ms;
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "simulatedLlmWallMs": self.wall_ms.round(),
            "calls": self.calls,
            "callMs": self.busy_ms.round(),
            "effectiveConcurrency": if self.wall_ms > 0.0 { self.busy_ms / self.wall_ms } else { 0.0 },
            "byKind": self.by_kind.iter().map(|(k, (n, w))| {
                serde_json::json!({ "kind": k, "calls": n, "wallMs": w.round() })
            }).collect::<Vec<_>>(),
            "dispatches": self.dispatches.iter().map(|(k, n, w)| {
                serde_json::json!([k.split_whitespace().nth(3).unwrap_or(""), n, w.round()])
            }).collect::<Vec<_>>(),
        })
    }
}

fn kind_of(call: &LlmCall) -> String {
    call.system_prompt
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ")
}

/// An f64 ordered for a min-heap of (finish, seq).
#[derive(PartialEq)]
struct Finish(f64, u64);
impl Eq for Finish {}
impl PartialOrd for Finish {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Finish {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reversed: BinaryHeap is a max-heap.
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| other.1.cmp(&self.1))
    }
}

/// The wrapper (see the module doc).
pub struct LatencySim<'p> {
    inner: &'p dyn NameProvider,
    slots: usize,
    report: Mutex<SimReport>,
}

impl<'p> LatencySim<'p> {
    pub fn new(inner: &'p dyn NameProvider, slots: usize) -> Self {
        LatencySim {
            inner,
            slots: slots.max(1),
            report: Mutex::new(SimReport::default()),
        }
    }

    pub fn report(&self) -> SimReport {
        std::mem::take(&mut self.report.lock().expect("sim lock"))
    }

    /// List-schedule `lat` (in order) on the slots from t = 0: the makespan.
    fn makespan(&self, lat: &[f64]) -> f64 {
        let mut free: BinaryHeap<Finish> = (0..self.slots as u64).map(|i| Finish(0.0, i)).collect();
        let mut end = 0.0f64;
        for (i, l) in lat.iter().enumerate() {
            let Finish(t, _) = free.pop().expect("a slot");
            let done = t + l;
            end = end.max(done);
            free.push(Finish(done, self.slots as u64 + i as u64));
        }
        end
    }
}

impl NameProvider for LatencySim<'_> {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        let lat: Vec<f64> = calls.iter().map(latency_ms).collect();
        let kind = calls.first().map(kind_of).unwrap_or_default();
        let wall = self.makespan(&lat);
        self.report.lock().expect("sim lock").charge(
            &kind,
            calls.len() as u64,
            wall,
            lat.iter().sum(),
        );
        self.inner.run_wave(calls)
    }

    fn run_pipelined(&self, initial: Vec<(usize, LlmCall)>, on_done: &mut OnCallDone<'_>) {
        let mut waiting: VecDeque<(usize, LlmCall)> = initial.into();
        let mut running: BinaryHeap<Finish> = BinaryHeap::new();
        let mut in_flight: BTreeMap<u64, (usize, LlmCall)> = BTreeMap::new();
        let mut now = 0.0f64;
        let mut seq = 0u64;
        let mut busy = 0.0f64;
        let mut calls = 0u64;
        let mut kind = String::new();
        loop {
            while running.len() < self.slots
                && let Some((id, call)) = waiting.pop_front()
            {
                let l = latency_ms(&call);
                busy += l;
                calls += 1;
                if kind.is_empty() {
                    kind = kind_of(&call);
                }
                running.push(Finish(now + l, seq));
                in_flight.insert(seq, (id, call));
                seq += 1;
            }
            let Some(Finish(t, s)) = running.pop() else {
                break;
            };
            now = t;
            let (id, call) = in_flight.remove(&s).expect("in flight");
            let result = self
                .inner
                .run_wave(vec![call])
                .pop()
                .expect("one result per call");
            waiting.extend(on_done(id, result));
        }
        self.report
            .lock()
            .expect("sim lock")
            .charge(&kind, calls, now, busy);
    }
}

#[cfg(test)]
mod llm_sim_test;
