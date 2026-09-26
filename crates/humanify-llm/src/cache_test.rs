//! Port of src/llm/cached-provider.test.ts, fixture for fixture, plus the
//! byte-format pins. The TS file's header: identical runs agree to ~±115
//! noise lines within a serving session but drift by ±2.7k across
//! sessions — caching by request makes repeated prompts deterministic.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use humanify_model::llm::{
    BatchRenameRequest, BatchRenameResponse, CacheKeyParams, LlmCall, LlmError, LlmErrorKind,
    Renames, Usage,
};

use crate::cache::{CacheEntry, CachedProvider, DiskCache};
use crate::provider::AsyncProvider;

pub(crate) fn tmp_dir(tag: &str) -> std::path::PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "humanify-llm-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::remove_dir_all(&dir).ok();
    dir
}

pub(crate) fn renames(pairs: &[(&str, &str)]) -> Renames {
    Renames::from_entries(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Some(v.to_string()))),
    )
}

/// `class FakeProvider`: counts calls, answers a settable response or fails.
struct FakeProvider {
    calls: AtomicUsize,
    response: Mutex<BatchRenameResponse>,
    fail: Mutex<bool>,
}

impl FakeProvider {
    fn new() -> Self {
        FakeProvider {
            calls: AtomicUsize::new(0),
            response: Mutex::new(BatchRenameResponse {
                renames: renames(&[("a", "alpha")]),
                finish_reason: Some("stop".to_string()),
                usage: Some(Usage {
                    total_tokens: Some(15),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                }),
            }),
            fail: Mutex::new(false),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl AsyncProvider for &FakeProvider {
    async fn suggest_all_names(&self, _call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if *self.fail.lock().unwrap() {
            return Err(LlmError::new(LlmErrorKind::Other, "server down"));
        }
        Ok(self.response.lock().unwrap().clone())
    }
}

/// `function request(code)`: identifiers ["a"], usedNames {"taken"}.
fn request(code: &str) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            code: code.to_string(),
            identifiers: vec!["a".to_string()],
            used_names: vec!["taken".to_string()],
            ..BatchRenameRequest::default()
        },
        system_prompt: String::new(),
        user_prompt: String::new(),
    }
}

fn params(model: &str) -> CacheKeyParams {
    CacheKeyParams {
        model: model.to_string(),
        temperature: Some(0.0),
        max_tokens: None,
        reasoning_effort: None,
    }
}

fn cached<'a>(
    inner: &'a FakeProvider,
    dir: &std::path::Path,
    model: &str,
) -> CachedProvider<&'a FakeProvider> {
    CachedProvider::new(inner, DiskCache::open(dir).unwrap(), params(model), None)
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

/// "serves identical requests from disk after one inner call"
#[test]
fn serves_identical_requests_from_disk_after_one_inner_call() {
    let dir = tmp_dir("cache");
    let inner = FakeProvider::new();
    let provider = cached(&inner, &dir, "m1");
    let first = run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    let second = run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    assert_eq!(inner.calls(), 1);
    assert_eq!(first.renames, renames(&[("a", "alpha")]));
    assert_eq!(second.renames, renames(&[("a", "alpha")]));
}

/// "misses on different prompts and different model params"
#[test]
fn misses_on_different_prompts_and_different_model_params() {
    let dir = tmp_dir("cache");
    let inner = FakeProvider::new();
    let provider = cached(&inner, &dir, "m1");
    run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    run(provider.suggest_all_names(&request("function g() {}"))).unwrap();
    assert_eq!(inner.calls(), 2);
    let other = cached(&inner, &dir, "m2");
    run(other.suggest_all_names(&request("function f() {}"))).unwrap();
    assert_eq!(inner.calls(), 3);
}

/// "persists across provider instances (cross-session determinism)"
#[test]
fn persists_across_provider_instances() {
    let dir = tmp_dir("cache");
    let inner = FakeProvider::new();
    run(cached(&inner, &dir, "m1").suggest_all_names(&request("function f() {}"))).unwrap();
    let inner2 = FakeProvider::new();
    *inner2.response.lock().unwrap() = BatchRenameResponse {
        renames: renames(&[("a", "DIFFERENT")]),
        finish_reason: Some("stop".to_string()),
        usage: None,
    };
    let result =
        run(cached(&inner2, &dir, "m1").suggest_all_names(&request("function f() {}"))).unwrap();
    assert_eq!(inner2.calls(), 0, "must not reach the server");
    assert_eq!(result.renames, renames(&[("a", "alpha")]));
}

/// "zeroes usage on hits and reports hit/miss counters"
#[test]
fn zeroes_usage_on_hits_and_reports_counters() {
    let dir = tmp_dir("cache");
    let inner = FakeProvider::new();
    let provider = cached(&inner, &dir, "m1");
    run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    let hit = run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    assert_eq!(hit.usage.and_then(|u| u.total_tokens).unwrap_or(0), 0);
    let stats = provider.stats();
    assert_eq!((stats.hits, stats.misses, stats.writes), (1, 1, 1));
}

/// Errors are never cached; an EMPTY answer is (finding #57: it is the
/// answer the run acted on, and a replay must take the same path).
#[test]
fn caches_empty_responses_but_never_errors() {
    let dir = tmp_dir("cache");
    let inner = FakeProvider::new();
    *inner.response.lock().unwrap() = BatchRenameResponse {
        renames: Renames::default(),
        finish_reason: Some("length".to_string()),
        usage: None,
    };
    let provider = cached(&inner, &dir, "m1");
    run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    run(provider.suggest_all_names(&request("function f() {}"))).unwrap();
    assert_eq!(inner.calls(), 1, "an empty response is recorded");
    *inner.fail.lock().unwrap() = true;
    assert!(run(provider.suggest_all_names(&request("function h() {}"))).is_err());
    *inner.fail.lock().unwrap() = false;
    *inner.response.lock().unwrap() = BatchRenameResponse {
        renames: renames(&[("a", "beta")]),
        finish_reason: Some("stop".to_string()),
        usage: None,
    };
    let after = run(provider.suggest_all_names(&request("function h() {}"))).unwrap();
    assert_eq!(after.renames, renames(&[("a", "beta")]));
}

/// The entry file is exactly what the TS writes: key order v, renames,
/// finishReason, originalUsage; the shard is key[0..2]/key[2..].json.
#[test]
fn writes_the_ts_entry_bytes_at_the_ts_shard_path() {
    let dir = tmp_dir("cache");
    let inner = FakeProvider::new();
    let provider = cached(&inner, &dir, "m1");
    let call = request("function f() {}");
    run(provider.suggest_all_names(&call)).unwrap();
    let key = provider.key_of(&call);
    let path = dir.join(&key[..2]).join(format!("{}.json", &key[2..]));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        r#"{"v":1,"renames":{"a":"alpha"},"finishReason":"stop","originalUsage":{"totalTokens":15,"inputTokens":10,"outputTokens":5}}"#
    );
}

/// The read rule: `v === 1` and `typeof renames === "object"` — so a null
/// renames is a HIT with no renames (typeof null is "object"), a wrong
/// version or a string renames is a miss.
#[test]
fn reads_with_the_ts_acceptance_rule() {
    assert!(CacheEntry::from_json(r#"{"v":2,"renames":{}}"#).is_none());
    assert!(CacheEntry::from_json(r#"{"v":1,"renames":"x"}"#).is_none());
    assert!(CacheEntry::from_json(r#"{"v":1}"#).is_none());
    assert!(CacheEntry::from_json("not json").is_none());
    let null = CacheEntry::from_json(r#"{"v":1,"renames":null}"#).unwrap();
    assert!(null.renames.is_empty());
    let decline = CacheEntry::from_json(r#"{"v":1,"renames":{"k":null,"j":"x"}}"#).unwrap();
    assert_eq!(decline.renames.get("k"), None);
    assert_eq!(decline.renames.get("j"), Some("x"));
}

/// A read-only handle never writes, and a replay-only stack reports the
/// miss as an error without touching the directory.
#[test]
fn read_only_cache_refuses_writes() {
    let dir = tmp_dir("cache");
    let cache = DiskCache::open_read_only(&dir);
    let entry = CacheEntry {
        renames: renames(&[("a", "b")]),
        finish_reason: None,
        original_usage: None,
    };
    assert!(cache.write("abcdef", &entry).is_err());
    assert!(!dir.exists(), "a read-only open creates nothing");
}
