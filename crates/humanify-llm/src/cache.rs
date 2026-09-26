//! The disk response cache (llm/cached-provider.ts) — the ONE owner of the
//! cache's disk side: shard layout, entry format, read, atomic write. The
//! key is derived by `humanify_model::llm::cache_key_of` (its owner; see
//! that module for why it lives in the model crate).
//!
//! Format (unchanged from the TS, 02 §7 — a Rust leg replays a TS-written
//! cache and vice versa):
//! - path: `<dir>/<key[0..2]>/<key[2..]>.json`;
//! - entry: `JSON.stringify({v: 1, renames, finishReason, originalUsage})`
//!   — undefined fields absent, compact, no trailing newline;
//! - a read accepts `v === 1` and an object-typed `renames`; anything else
//!   (missing, unparsable, other version) is a miss;
//! - writes are atomic (`<target>.<pid>.tmp` + rename);
//! - ONE KEY, ONE ANSWER (finding #57): requests with the same key are
//!   single-flighted — the first goes to the model, every copy in flight
//!   behind it waits and is served the recorded answer. The model answers
//!   two copies of one prompt differently, and the cache can keep only one
//!   of them: when both went live, each copy's function took its own answer
//!   while the disk kept the last writer's, so a replay of the run handed
//!   every copy that one answer and asked questions the live run never did
//!   (64 misses on a 0-error 2.1.85→86 run);
//! - every successful answer is written, an EMPTY one too (a retry that
//!   came back `{}` is still the answer the run acted on — unrecorded, the
//!   replay missed it); errors are never cached; a hit reports zero token
//!   usage (metrics reflect THIS run).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use humanify_model::js::{JsObject, JsValue, stringify};
use humanify_model::llm::{
    BatchRenameResponse, CacheKeyParams, LlmCall, LlmError, Renames, Usage, cache_key_of,
};

use crate::debug::{LlmLogEvent, LogSink, emit};
use crate::provider::AsyncProvider;

/// One cache entry (`CacheEntry` in the TS).
#[derive(Clone, Debug, PartialEq)]
pub struct CacheEntry {
    pub renames: Renames,
    pub finish_reason: Option<String>,
    /// The live call's usage, kept for reference.
    pub original_usage: Option<Usage>,
}

impl CacheEntry {
    /// The entry file's bytes, exactly as the TS `JSON.stringify` writes them.
    pub fn to_json(&self) -> String {
        let mut obj = JsObject::new();
        obj.insert("v", JsValue::Number(1.0));
        obj.insert("renames", self.renames.to_js());
        obj.insert_opt(
            "finishReason",
            self.finish_reason.as_deref().map(JsValue::str),
        );
        obj.insert_opt(
            "originalUsage",
            self.original_usage.as_ref().map(Usage::to_js),
        );
        stringify(&JsValue::Object(obj))
    }

    /// Parse an entry file with the TS read's acceptance rule: `v === 1`
    /// and `typeof renames === "object"`. `renames: null` passes that check
    /// in JS (typeof null is "object") and spreads to `{}`; an array spreads
    /// to its index keys. A rename value that is neither a string nor null
    /// is treated as corrupt (a miss) — never observed (0 of 188,720 values
    /// in the standing cache, 2026-09-24).
    pub fn from_json(text: &str) -> Option<CacheEntry> {
        let value = JsValue::parse(text).ok()?;
        let obj = value.as_object()?;
        if obj.get("v") != Some(&JsValue::Number(1.0)) {
            return None;
        }
        let renames = match obj.get("renames")? {
            JsValue::Null => Renames::default(),
            JsValue::Object(map) => Renames::from_js(map)?,
            JsValue::Array(items) => Renames::from_js(
                &items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v.clone()))
                    .collect(),
            )?,
            _ => return None,
        };
        let finish_reason = match obj.get("finishReason") {
            Some(JsValue::String(s)) => Some(s.clone()),
            _ => None,
        };
        let original_usage = obj.get("originalUsage").and_then(Usage::from_js);
        Some(CacheEntry {
            renames,
            finish_reason,
            original_usage,
        })
    }
}

/// The cache directory. A read-only handle refuses writes (the standing
/// caches are never written by parity tooling).
pub struct DiskCache {
    dir: PathBuf,
    read_only: bool,
}

impl DiskCache {
    /// Open for read + write, creating the directory (`mkdirSync(dir,
    /// {recursive: true})` in the TS constructor).
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(DiskCache {
            dir: dir.to_path_buf(),
            read_only: false,
        })
    }

    /// Open for reads only: no directory is created, every write is refused.
    pub fn open_read_only(dir: &Path) -> Self {
        DiskCache {
            dir: dir.to_path_buf(),
            read_only: true,
        }
    }

    /// `<dir>/<key[0..2]>/<key[2..]>.json`.
    pub fn path_of(&self, key: &str) -> PathBuf {
        self.dir.join(&key[..2]).join(format!("{}.json", &key[2..]))
    }

    /// The entry for `key`, or None (missing or corrupt — both a miss).
    pub fn read(&self, key: &str) -> Option<CacheEntry> {
        let text = std::fs::read_to_string(self.path_of(key)).ok()?;
        CacheEntry::from_json(&text)
    }

    /// Atomic write: temp file beside the target, then rename.
    pub fn write(&self, key: &str, entry: &CacheEntry) -> std::io::Result<()> {
        if self.read_only {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the cache was opened read-only",
            ));
        }
        let target = self.path_of(key);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = PathBuf::from(format!("{}.{}.tmp", target.display(), std::process::id()));
        std::fs::write(&tmp, entry.to_json())?;
        std::fs::rename(&tmp, &target)
    }
}

/// The cache's counters (`stats` in the TS, plus `writes` — the rule-10
/// proof obligation: a warm replay must write ZERO).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub writes: usize,
}

/// A key's single-flight gate: an async lock the copies of one request
/// queue on (FIFO, so the first dispatched is the one that asks).
type KeyGate = Arc<tokio::sync::Mutex<()>>;

/// Serve a request from disk when its key has an entry, else ask the inner
/// provider and record the answer — one request per key at a time, so
/// every copy of a request gets the answer the cache records.
pub struct CachedProvider<P> {
    inner: P,
    cache: DiskCache,
    params: CacheKeyParams,
    log: Option<LogSink>,
    in_flight: Mutex<HashMap<String, KeyGate>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
    writes: AtomicUsize,
}

impl<P> CachedProvider<P> {
    pub fn new(inner: P, cache: DiskCache, params: CacheKeyParams, log: Option<LogSink>) -> Self {
        CachedProvider {
            inner,
            cache,
            params,
            log,
            in_flight: Mutex::new(HashMap::new()),
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            writes: self.writes.load(Ordering::Relaxed),
        }
    }

    pub fn key_of(&self, call: &LlmCall) -> String {
        cache_key_of(&call.request, &self.params)
    }

    fn gate_of(&self, key: &str) -> KeyGate {
        let mut map = self.in_flight.lock().expect("the in-flight map lock");
        map.entry(key.to_string()).or_default().clone()
    }

    /// Drop the key's gate once no other copy holds or waits on it (the map
    /// and `gate` are the only two owners left).
    fn release(&self, key: &str, gate: KeyGate) {
        let mut map = self.in_flight.lock().expect("the in-flight map lock");
        if Arc::strong_count(&gate) <= 2 {
            map.remove(key);
        }
    }

    fn record(&self, key: &str, response: &BatchRenameResponse) {
        let entry = CacheEntry {
            renames: response.renames.clone(),
            finish_reason: response.finish_reason.clone(),
            original_usage: response.usage.clone(),
        };
        match self.cache.write(key, &entry) {
            Ok(()) => {
                self.writes.fetch_add(1, Ordering::Relaxed);
            }
            // A cache write failure must never fail the run.
            Err(err) => emit(
                &self.log,
                LlmLogEvent::Message {
                    category: "processor".to_string(),
                    message: format!("llm-cache: write failed for {key}: {err}"),
                },
            ),
        }
    }

    async fn serve<'a>(
        &'a self,
        key: &str,
        call: &'a LlmCall,
    ) -> Result<BatchRenameResponse, LlmError>
    where
        P: AsyncProvider,
    {
        if let Some(entry) = self.cache.read(key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(BatchRenameResponse {
                renames: entry.renames,
                finish_reason: entry.finish_reason,
                // Zero spend: metrics reflect what THIS run cost.
                usage: Some(Usage::zero()),
            });
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let response = self.inner.suggest_all_names(call).await?;
        self.record(key, &response);
        Ok(response)
    }
}

impl<P: AsyncProvider> AsyncProvider for CachedProvider<P> {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let key = self.key_of(call);
        let gate = self.gate_of(&key);
        let result = {
            let _turn = gate.lock().await;
            self.serve(&key, call).await
        };
        self.release(&key, gate);
        result
    }

    fn cache_stats(&self) -> Option<CacheStats> {
        Some(self.stats())
    }
}
