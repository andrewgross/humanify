//! WP4.1's replay gate (10-work-breakdown WP4.1: "the Rust client replays
//! a TS-written cache entry byte-for-byte"), kept as a unit-test golden
//! since the cutover: the committed fixture (test/parity/wp41-replay/)
//! replayed through the live key derivation and cache. Test-only.
//!
//! Inputs, per pair:
//! - `requests`: the FULL typed request of every TS dispatch
//!   (`{seq, params, request, cacheKey}` — the capture hook's rows; the
//!   oracle dump's own cache-keys.jsonl has the same shape but drops the
//!   callee `snippet`, so it is lossy for every request with callees);
//! - `dump_keys` (optional): the oracle's cache-keys.jsonl — the capture
//!   must reproduce its key SEQUENCE exactly (same dispatches, same order),
//!   which is what makes "every key in cache-keys.jsonl" covered;
//! - `ts_replay`: what the TS `CachedLLMProvider` returned for each row
//!   over the same cache (test/parity/wp41-replay-probe.ts);
//! - `cache`: the cache directory (a scratch copy; opened read-only).
//!
//! Checks: (1) the Rust key == the TS key for every row; (2) the key
//! sequence == the dump's; (3) hit/miss agrees with the TS and every hit's
//! response serializes to the TS's bytes; (4) every hit entry re-serializes
//! to its file's exact bytes (the Rust WRITE path matches the TS's);
//! plus the dump-only census: which dump rows re-derive without the capture.

use std::path::Path;

use humanify_model::llm::{BatchRenameRequest, CacheKeyParams, LlmCall, cache_key_of};

use crate::cache::{AnswerMemo, CacheEntry, DiskCache};
use crate::provider::{AsyncProvider, ReplayMiss};

/// One dispatch row (capture or dump shape).
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRow {
    pub seq: u64,
    pub params: CacheKeyParams,
    pub request: BatchRenameRequest,
    pub cache_key: String,
}

/// One TS replay row.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct TsReplayRow {
    pub seq: u64,
    pub key: String,
    pub hit: bool,
    pub response: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayGateReport {
    pub rows: usize,
    pub keys_identical: usize,
    pub key_mismatches: Vec<u64>,
    /// None when no dump was given.
    pub dump_rows: Option<usize>,
    pub dump_sequence_mismatches: Vec<u64>,
    /// Dump rows whose OWN material re-derives their key / those that do
    /// not, and how many of the latter carry callees (the dropped snippet).
    pub dump_self_keys_identical: usize,
    pub dump_self_key_mismatches: usize,
    pub dump_self_mismatches_with_callees: usize,
    pub hits: usize,
    pub misses: usize,
    pub responses_identical: usize,
    pub response_mismatches: Vec<u64>,
    pub entries_roundtrip_identical: usize,
    pub entry_roundtrip_mismatches: Vec<String>,
    pub cache_writes: usize,
}

impl ReplayGateReport {
    /// Every check passed.
    pub fn identical(&self) -> bool {
        self.key_mismatches.is_empty()
            && self.dump_sequence_mismatches.is_empty()
            && self.response_mismatches.is_empty()
            && self.entry_roundtrip_mismatches.is_empty()
            && self.cache_writes == 0
            && self.keys_identical == self.rows
            && self.responses_identical == self.rows
    }

    pub fn summary(&self) -> String {
        let dump = match self.dump_rows {
            Some(n) => format!(
                "dump rows {n}, sequence mismatches {}; dump-only re-derivation {} identical / {} not ({} of them carry callees)",
                self.dump_sequence_mismatches.len(),
                self.dump_self_keys_identical,
                self.dump_self_key_mismatches,
                self.dump_self_mismatches_with_callees
            ),
            None => "no dump given".to_string(),
        };
        format!(
            "rows {}: keys identical {} (mismatches {}); {dump}; replay hits {} misses {}; responses identical {} (mismatches {}); entries byte-identical on re-serialize {} (mismatches {}); cache writes {}; verdict {}",
            self.rows,
            self.keys_identical,
            self.key_mismatches.len(),
            self.hits,
            self.misses,
            self.responses_identical,
            self.response_mismatches.len(),
            self.entries_roundtrip_identical,
            self.entry_roundtrip_mismatches.len(),
            self.cache_writes,
            if self.identical() {
                "IDENTICAL"
            } else {
                "DIVERGED"
            }
        )
    }
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .filter(|l| !l.is_empty())
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))
        })
        .collect()
}

/// A row's material as a replayable call (the prompts are irrelevant to
/// the cache — it keys on the typed request).
fn call_of(row: &KeyRow) -> LlmCall {
    LlmCall {
        request: row.request.clone(),
        system_prompt: String::new(),
        user_prompt: String::new(),
    }
}

fn census_dump(report: &mut ReplayGateReport, rows: &[KeyRow], dump: &[KeyRow]) {
    report.dump_rows = Some(dump.len());
    for (i, d) in dump.iter().enumerate() {
        if rows
            .get(i)
            .is_none_or(|r| r.cache_key != d.cache_key || r.seq != d.seq)
        {
            report.dump_sequence_mismatches.push(d.seq);
        }
        if cache_key_of(&d.request, &d.params) == d.cache_key {
            report.dump_self_keys_identical += 1;
        } else {
            report.dump_self_key_mismatches += 1;
            if !d.request.callee_signatures.is_empty() {
                report.dump_self_mismatches_with_callees += 1;
            }
        }
    }
    if rows.len() > dump.len() {
        report
            .dump_sequence_mismatches
            .extend(rows[dump.len()..].iter().map(|r| r.seq));
    }
}

fn check_entry_bytes(report: &mut ReplayGateReport, cache: &DiskCache, key: &str) {
    let raw = std::fs::read_to_string(cache.path_of(key)).unwrap_or_default();
    match CacheEntry::from_json(&raw) {
        Some(entry) if entry.to_json() == raw => report.entries_roundtrip_identical += 1,
        _ => report.entry_roundtrip_mismatches.push(key.to_string()),
    }
}

/// Run the gate for one pair.
pub fn run(
    requests: &Path,
    dump_keys: Option<&Path>,
    ts_replay: &Path,
    cache_dir: &Path,
) -> Result<ReplayGateReport, String> {
    let rows: Vec<KeyRow> = read_jsonl(requests)?;
    let ts: Vec<TsReplayRow> = read_jsonl(ts_replay)?;
    if ts.len() != rows.len() {
        return Err(format!(
            "{} request rows vs {} TS replay rows",
            rows.len(),
            ts.len()
        ));
    }
    let mut report = ReplayGateReport {
        rows: rows.len(),
        ..ReplayGateReport::default()
    };
    if let Some(path) = dump_keys {
        census_dump(&mut report, &rows, &read_jsonl::<KeyRow>(path)?);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| e.to_string())?;
    for (row, expected) in rows.iter().zip(&ts) {
        let key = cache_key_of(&row.request, &row.params);
        if key == row.cache_key && key == expected.key && row.seq == expected.seq {
            report.keys_identical += 1;
        } else {
            report.key_mismatches.push(row.seq);
        }
        // One provider per row: the params are per row (the TS probe does
        // the same), the cache read-only, the inner provider a dead end.
        let provider = AnswerMemo::on_disk(
            ReplayMiss,
            DiskCache::open_read_only(cache_dir),
            row.params.clone(),
            None,
        );
        let got = runtime
            .block_on(provider.suggest_all_names(&call_of(row)))
            .ok();
        report.cache_writes += provider.stats().map_or(0, |s| s.writes);
        let got_json = got.as_ref().map(|r| r.to_json());
        if got.is_some() {
            report.hits += 1;
            check_entry_bytes(&mut report, &DiskCache::open_read_only(cache_dir), &key);
        } else {
            report.misses += 1;
        }
        if got_json == expected.response && got.is_some() == expected.hit {
            report.responses_identical += 1;
        } else {
            report.response_mismatches.push(row.seq);
        }
    }
    Ok(report)
}
