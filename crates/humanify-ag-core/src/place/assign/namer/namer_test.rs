//! The TS namer's exact requests + answer mapping
//! (test/parity/wp51-namer.json, from test/parity/wp51-namer-probe.ts), and
//! the split-namer.test.ts behaviors.

use super::{
    FolderSummary, NameKind, NameLevel, ProviderSplitNamer, ProviderTreeReviser, SplitNameRequest,
    SplitNamer, SplitNamerBudget, TreeReviser, split_namer_batches,
};
use humanify_ag_model::llm::{CacheKeyParams, LlmCall, LlmError, LlmErrorKind, cache_key_of};
use humanify_ag_testkit::fake::{ShuffledFake, names, reply};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../../../../test/parity/wp51-namer.json");

/// A fake answering every call from a fixed map (`None`: every call
/// fails); the fake records each call.
fn map_provider(answer: Option<Vec<(String, String)>>) -> ShuffledFake {
    ShuffledFake::answering(1, move |_| match &answer {
        Some(entries) => reply(entries.iter().map(|(k, v)| (k.clone(), Some(v.clone())))),
        None => Err(LlmError::new(LlmErrorKind::CacheMiss, "down")),
    })
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn request_of(v: &Value) -> SplitNameRequest {
    SplitNameRequest {
        kind: if v["kind"] == "file" {
            NameKind::File
        } else {
            NameKind::Folder
        },
        mechanical_stem: v["mechanicalStem"].as_str().unwrap().to_string(),
        siblings: strs(&v["siblings"]),
        bindings: strs(&v["bindings"]),
        members: v.get("members").map(strs),
        level: match v.get("level").and_then(Value::as_str) {
            Some("top") => Some(NameLevel::Top),
            Some("sub") => Some(NameLevel::Sub),
            _ => None,
        },
        evidence: v
            .get("evidence")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn params(v: &Value) -> CacheKeyParams {
    CacheKeyParams {
        model: v["model"].as_str().unwrap().to_string(),
        temperature: v["temperature"].as_f64(),
        max_tokens: None,
        reasoning_effort: v["reasoningEffort"].as_str().map(str::to_string),
    }
}

fn check_request(call: &LlmCall, want: &Value, key: &str, params: &CacheKeyParams) {
    let req = &want["request"];
    assert_eq!(call.request.code, req["code"].as_str().unwrap());
    assert_eq!(call.user_prompt, req["userPrompt"].as_str().unwrap());
    assert_eq!(call.system_prompt, req["systemPrompt"].as_str().unwrap());
    assert_eq!(call.request.identifiers, strs(&req["identifiers"]));
    assert_eq!(call.request.used_names, strs(&req["usedNames"]));
    assert_eq!(cache_key_of(&call.request, params), key);
}

/// The retaining test log (finding #65): the namer tests read the calls
/// back out of it.
fn split_log() -> crate::artifact_dump::DispatchLog {
    crate::artifact_dump::DispatchLog::retain_for_tests(
        humanify_ag_model::llm::CacheKeyParams::default(),
    )
}

#[test]
fn every_ts_namer_request_and_answer_mapping_agrees() {
    let doc: Value = serde_json::from_str(VECTORS).expect("vectors");
    let params = params(&doc["params"]);
    let mut namers = 0;
    for row in doc["rows"].as_array().unwrap() {
        if row["kind"] != "namer" {
            continue;
        }
        let requests: Vec<SplitNameRequest> = row["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(request_of)
            .collect();
        let answers: Vec<(String, String)> = row["answers"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        let provider = map_provider(Some(answers));
        let mut log = split_log();
        let mut namer = ProviderSplitNamer::new(&provider, &mut log);
        let result = namer.name(&requests);
        let calls = provider.calls();
        assert_eq!(calls.len(), 1, "one batch = one provider call");
        check_request(&calls[0], row, row["cacheKey"].as_str().unwrap(), &params);
        let want: Vec<Option<String>> = row["result"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect();
        assert_eq!(result, want);
        namers += 1;
    }
    assert_eq!(namers, 5);
}

#[test]
fn the_ts_reviser_request_and_map_agree() {
    let doc: Value = serde_json::from_str(VECTORS).expect("vectors");
    let params = params(&doc["params"]);
    let row = doc["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "reviser")
        .unwrap();
    let folders: Vec<FolderSummary> = row["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| FolderSummary {
            name: f["name"].as_str().unwrap().to_string(),
            members: strs(&f["members"]),
        })
        .collect();
    let provider = map_provider(Some(vec![
        ("auth".into(), "authFlow".into()),
        ("conn".into(), "conn".into()),
        ("same".into(), String::new()),
    ]));
    let mut reviser = ProviderTreeReviser::retaining(&provider);
    let out = reviser.revise(&folders);
    check_request(
        &provider.calls()[0],
        row,
        row["cacheKey"].as_str().unwrap(),
        &params,
    );
    assert_eq!(out, vec![("auth".to_string(), "authFlow".to_string())]);
}

#[test]
fn a_provider_failure_is_all_none_and_an_empty_batch_calls_nothing() {
    let provider = map_provider(None);
    let mut log = split_log();
    let mut namer = ProviderSplitNamer::new(&provider, &mut log);
    let req = SplitNameRequest {
        kind: NameKind::File,
        mechanical_stem: "a".into(),
        siblings: vec![],
        bindings: vec![],
        members: None,
        level: None,
        evidence: None,
    };
    assert_eq!(namer.name(&[req.clone(), req]), vec![None, None]);
    assert_eq!(namer.failed_batches, 1);
    assert!(namer.name(&[]).is_empty());
    assert_eq!(provider.calls().len(), 1);
    let mut reviser = ProviderTreeReviser::retaining(&provider);
    assert!(reviser.revise(&[]).is_empty());
    assert!(
        reviser
            .revise(&[FolderSummary {
                name: "x".into(),
                members: vec!["y".into()]
            }])
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Finding #39: the namer batches by a size budget derived from the model's
// context, so a fossil hop's thousands of fresh mints never become ONE
// prompt the model refuses (400 context length).
// ---------------------------------------------------------------------------

/// A fossil-mint-shaped request: 12 declarations, 20 siblings.
fn mint(i: usize) -> SplitNameRequest {
    SplitNameRequest {
        kind: NameKind::File,
        mechanical_stem: format!("module{}", i % 37),
        siblings: (0..20).map(|s| format!("sibling-file-{s}")).collect(),
        bindings: (0..12).map(|b| format!("declaredBinding{i}x{b}")).collect(),
        members: None,
        level: None,
        evidence: None,
    }
}

/// Answers every key of every prompt with `named-<key>`.
fn echo_provider() -> ShuffledFake {
    ShuffledFake::answering(1, |c| names(c, |k| format!("named-{k}")))
}

#[test]
fn the_budget_is_derived_from_the_model_context() {
    let small = SplitNamerBudget::for_model(32_768, 6_000);
    let large = SplitNamerBudget::for_model(131_072, 6_000);
    // Room for the completion and the system prompt, with headroom: the
    // prompt stays under the context even at 2.5 bytes/token (#39's
    // refused prompts measured ~3.9).
    assert!(small.max_prompt_chars > 20_000, "{small:?}");
    assert!(small.max_prompt_chars * 2 / 5 < 32_768 - 6_000, "{small:?}");
    assert!(
        large.max_prompt_chars > 3 * small.max_prompt_chars,
        "{large:?}"
    );
    // A context smaller than the completion reserve still makes progress:
    // one entry per call, never zero.
    let tiny = SplitNamerBudget::for_model(1_000, 6_000);
    assert!(tiny.max_entries >= 1);
    assert_eq!(
        split_namer_batches(&[mint(0), mint(1)], &tiny),
        vec![0..1, 1..2]
    );
}

#[test]
fn a_large_mint_set_is_many_bounded_requests_with_deterministic_boundaries() {
    let requests: Vec<SplitNameRequest> = (0..4_000).map(mint).collect();
    let budget = SplitNamerBudget::for_model(32_768, 6_000);
    let run = || {
        let provider = echo_provider();
        let mut log = split_log();
        let mut namer = ProviderSplitNamer::with_budget(&provider, &mut log, budget);
        let names = namer.name(&requests);
        let prompts: Vec<String> = provider
            .calls()
            .iter()
            .map(|c| c.user_prompt.clone())
            .collect();
        (names, prompts, namer.dispatched.len())
    };
    let (names, prompts, dispatched) = run();
    assert!(prompts.len() > 10, "{} prompts", prompts.len());
    assert_eq!(dispatched, prompts.len());
    for p in &prompts {
        assert!(
            p.len() <= budget.max_prompt_chars,
            "{} > {}",
            p.len(),
            budget.max_prompt_chars
        );
    }
    // Every request answered, in request order, from its own batch.
    assert_eq!(names.len(), requests.len());
    assert!(names.iter().all(Option::is_some));
    assert_eq!(names[0].as_deref(), Some("named-module0"));
    // Same input, same prompts: stable boundaries, so cache hits replay.
    let (names2, prompts2, _) = run();
    assert_eq!(prompts, prompts2);
    assert_eq!(names, names2);
    // The boundaries are the pure function's: contiguous, covering, capped.
    let ranges = split_namer_batches(&requests, &budget);
    assert_eq!(ranges.len(), prompts.len());
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, requests.len());
    assert!(ranges.windows(2).all(|w| w[0].end == w[1].start));
    assert!(ranges.iter().all(|r| r.len() <= budget.max_entries));
}

#[test]
fn a_scope_that_fits_is_still_exactly_one_call() {
    let requests: Vec<SplitNameRequest> = (0..5).map(mint).collect();
    assert_eq!(
        split_namer_batches(&requests, &SplitNamerBudget::default()),
        vec![0..5]
    );
}

/// Fails the SECOND batch of [`one_failed_batch_falls_back_alone`] only
/// (mints 10..20 — by its content, so the completion order cannot move
/// the failure to another batch).
fn second_fails() -> ShuffledFake {
    ShuffledFake::answering(1, |c| {
        if c.user_prompt.contains("declaredBinding10x") {
            return Err(LlmError::new(LlmErrorKind::CacheMiss, "down"));
        }
        names(c, |k| format!("n-{k}"))
    })
}

#[test]
fn one_failed_batch_falls_back_alone() {
    let requests: Vec<SplitNameRequest> = (0..30).map(mint).collect();
    let budget = SplitNamerBudget {
        max_prompt_chars: usize::MAX,
        max_entries: 10,
    };
    let mut log = split_log();
    let provider = second_fails();
    let mut namer = ProviderSplitNamer::with_budget(&provider, &mut log, budget);
    let names = namer.name(&requests);
    assert_eq!(namer.failed_batches, 1);
    assert!(names[..10].iter().all(Option::is_some));
    assert!(names[10..20].iter().all(Option::is_none));
    assert!(names[20..].iter().all(Option::is_some));
}

// ---------------------------------------------------------------------------
// The module kind (docs/design/module-naming.md): neutral keys, its own
// system prompt and site, at most MODULE_ENTRIES_PER_CALL per call, the
// taken names on a retry, and no stem-echo rule.
// ---------------------------------------------------------------------------

fn module_request(i: usize, taken: &[&str]) -> SplitNameRequest {
    SplitNameRequest {
        kind: NameKind::Module,
        mechanical_stem: format!("first-function-{i}"),
        siblings: taken.iter().map(|s| s.to_string()).collect(),
        bindings: Vec::new(),
        members: None,
        level: None,
        evidence: Some(format!(
            "Declares: function helper{i}(x); var state{i}\nCode:\n```js\nvar MODULE_INIT = __esm(() => {{}});\n```"
        )),
    }
}

#[test]
fn a_module_batch_has_neutral_keys_its_own_prompt_and_at_most_eight_entries() {
    let requests: Vec<SplitNameRequest> = (0..10).map(|i| module_request(i, &[])).collect();
    let provider = echo_provider();
    let mut log = split_log();
    let mut namer = ProviderSplitNamer::new(&provider, &mut log);
    let names = namer.name(&requests);
    let calls = provider.calls();
    assert_eq!(calls.len(), 2, "10 modules = 8 + 2");
    assert_eq!(calls[0].request.identifiers.len(), 8);
    assert_eq!(calls[1].request.identifiers, vec!["m1", "m2"]);
    assert_eq!(calls[0].system_prompt, super::MODULE_NAMER_SYSTEM_PROMPT);
    let user = &calls[0].user_prompt;
    assert!(user.starts_with("Name 8 source files of a decompiled JavaScript program."));
    assert!(user.contains("### m1\nDeclares: function helper0(x); var state0\n"));
    assert!(!user.contains("first-function"), "the stem never shows");
    assert!(user.ends_with("— one specific, distinct name per file."));
    assert_eq!(names[0].as_deref(), Some("named-m1"));
    assert_eq!(names[9].as_deref(), Some("named-m2"));
}

/// Answers `m1` with the first module's mechanical stem.
fn stem_answer() -> ShuffledFake {
    ShuffledFake::answering(1, |_| reply([("m1", Some("first-function-0".to_string()))]))
}

#[test]
fn a_module_retry_lists_the_taken_names_and_an_answer_equal_to_the_stem_counts() {
    let provider = stem_answer();
    let mut log = split_log();
    let mut namer = ProviderSplitNamer::new(&provider, &mut log);
    let names = namer.name(&[module_request(0, &["color-utils"])]);
    assert_eq!(names, vec![Some("first-function-0".to_string())]);
    let calls = provider.calls();
    assert!(
        calls[0]
            .user_prompt
            .contains("Already taken by other files (pick a DIFFERENT name): color-utils")
    );
    assert_eq!(calls[0].request.used_names, vec!["color-utils"]);
}
