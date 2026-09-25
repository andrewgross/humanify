//! The TS namer's exact requests + answer mapping
//! (test/parity/wp51-namer.json, from test/parity/wp51-namer-probe.ts), and
//! the split-namer.test.ts behaviors.

use super::{
    FolderSummary, NameKind, NameLevel, ProviderSplitNamer, ProviderTreeReviser, SplitNameRequest,
    SplitNamer, TreeReviser,
};
use humanify_model::llm::{
    BatchRenameResponse, CacheKeyParams, LlmCall, LlmError, LlmErrorKind, NameProvider, Renames,
    cache_key_of,
};
use serde_json::Value;
use std::cell::RefCell;

const VECTORS: &str = include_str!("../../../../../../test/parity/wp51-namer.json");

/// A provider answering from a fixed map, recording each call.
struct MapProvider {
    answer: Option<Vec<(String, String)>>,
    calls: RefCell<Vec<LlmCall>>,
}

impl NameProvider for MapProvider {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        self.calls.borrow_mut().extend(calls.iter().cloned());
        calls
            .iter()
            .map(|_| match &self.answer {
                Some(entries) => Ok(BatchRenameResponse {
                    renames: Renames::from_entries(
                        entries.iter().map(|(k, v)| (k.clone(), Some(v.clone()))),
                    ),
                    ..BatchRenameResponse::default()
                }),
                None => Err(LlmError::new(LlmErrorKind::CacheMiss, "down")),
            })
            .collect()
    }
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
        let provider = MapProvider {
            answer: Some(answers),
            calls: RefCell::new(Vec::new()),
        };
        let mut namer = ProviderSplitNamer::new(&provider);
        let result = namer.name(&requests);
        let calls = provider.calls.borrow();
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
    let provider = MapProvider {
        answer: Some(vec![
            ("auth".into(), "authFlow".into()),
            ("conn".into(), "conn".into()),
            ("same".into(), String::new()),
        ]),
        calls: RefCell::new(Vec::new()),
    };
    let mut reviser = ProviderTreeReviser::new(&provider);
    let out = reviser.revise(&folders);
    check_request(
        &provider.calls.borrow()[0],
        row,
        row["cacheKey"].as_str().unwrap(),
        &params,
    );
    assert_eq!(out, vec![("auth".to_string(), "authFlow".to_string())]);
}

#[test]
fn a_provider_failure_is_all_none_and_an_empty_batch_calls_nothing() {
    let provider = MapProvider {
        answer: None,
        calls: RefCell::new(Vec::new()),
    };
    let mut namer = ProviderSplitNamer::new(&provider);
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
    assert_eq!(provider.calls.borrow().len(), 1);
    let mut reviser = ProviderTreeReviser::new(&provider);
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
