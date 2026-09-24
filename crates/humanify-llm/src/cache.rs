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
//! - writes are atomic (`<target>.<pid>.tmp` + rename), so racing lanes at
//!   worst write the same bytes twice;
//! - only responses with at least one rename are written; errors are never
//!   cached; a hit reports zero token usage (metrics reflect THIS run).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// The TS `CachedLLMProvider`: serve a request from disk when its key has
/// an entry, else ask the inner provider and record a non-empty answer.
pub struct CachedProvider<P> {
    inner: P,
    cache: DiskCache,
    params: CacheKeyParams,
    log: Option<LogSink>,
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
}

impl<P: AsyncProvider> AsyncProvider for CachedProvider<P> {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let key = self.key_of(call);
        if let Some(entry) = self.cache.read(&key) {
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
        if !response.renames.is_empty() {
            let entry = CacheEntry {
                renames: response.renames.clone(),
                finish_reason: response.finish_reason.clone(),
                original_usage: response.usage.clone(),
            };
            match self.cache.write(&key, &entry) {
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
        Ok(response)
    }

    fn cache_stats(&self) -> Option<CacheStats> {
        Some(self.stats())
    }
}
