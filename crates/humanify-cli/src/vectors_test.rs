//! The driver's text and JSON against the REAL TS functions
//! (test/parity/wpb4-vectors.json, generator test/parity/wpb4-probe.ts):
//! ERROR blocks, code frames, invariant messages, divergence text, stage
//! fingerprints, selection records, flag invariants, the metadata writers,
//! progress sessions and preserved failed output. Also carries the TS unit
//! tests' fixtures (unified.test.ts, stage-fingerprint.test.ts,
//! selection-record.test.ts, progress.test.ts, failed-output.test.ts).

use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::output_validation::{
    FreeNameMeasure, OutputParseFailure, OutputSemanticFailure, ParserError, build_excerpt,
    compare_semantics, describe_parse_error, format_divergence,
};
use crate::pipeline_config::{build_pipeline_config, pipeline_selection_record};
use crate::progress::{LineRenderer, ProgressRenderer, TtyRenderer};
use crate::report::{
    report_internal_errors, report_parse_failures, report_semantic_failures, report_vendor_naming,
};
use crate::unified::{CommandOptions, FlagExplicitness, check_flag_invariants};
use crate::writers::{PlacementStats, StageHashes, stage_fingerprint};
use humanify_model::jsshape::JsType;

fn vectors() -> Value {
    let path = format!(
        "{}/../../test/parity/wpb4-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn parse_failure(v: &Value) -> OutputParseFailure {
    OutputParseFailure {
        message: v["message"].as_str().unwrap().to_string(),
        line: v["line"].as_u64().map(|x| x as u32),
        column: v["column"].as_u64().map(|x| x as u32),
        excerpt: v["excerpt"].as_str().map(str::to_string),
    }
}

#[test]
fn error_blocks_match_the_ts_reporters() {
    let v = vectors();
    let eb = &v["errorBlocks"];
    let mut n = 0;
    for case in eb["parse"].as_array().unwrap() {
        let input: Vec<(String, OutputParseFailure)> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                (
                    x["filePath"].as_str().unwrap().to_string(),
                    parse_failure(&x["failure"]),
                )
            })
            .collect();
        let r = report_parse_failures(&input);
        assert_eq!(r.messages, strs(&case["messages"]));
        assert_eq!(r.fails_run, case["exitCode"] == 1);
        n += 1;
    }
    for case in eb["semantic"].as_array().unwrap() {
        let input: Vec<(String, OutputSemanticFailure)> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                (
                    x["filePath"].as_str().unwrap().to_string(),
                    OutputSemanticFailure {
                        message: x["failure"]["message"].as_str().unwrap().to_string(),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let r = report_semantic_failures(&input);
        assert_eq!(r.messages, strs(&case["messages"]));
        assert_eq!(r.fails_run, case["exitCode"] == 1);
        n += 1;
    }
    for case in eb["internal"].as_array().unwrap() {
        let r = report_internal_errors(case["input"].as_u64().unwrap() as usize);
        assert_eq!(r.messages, strs(&case["messages"]));
        assert_eq!(r.fails_run, case["exitCode"] == 1);
        n += 1;
    }
    for case in eb["vendorNaming"].as_array().unwrap() {
        let js = humanify_model::js::JsValue::parse(&case["input"].to_string()).unwrap();
        let stats = humanify_model::stats::VendorNamingStats::from_js(&js, "v").unwrap();
        assert_eq!(stats.attempted(), case["attempted"].as_bool().unwrap());
        let r = report_vendor_naming(&stats);
        assert_eq!(r.messages, strs(&case["messages"]));
        assert!(!r.fails_run);
        n += 1;
    }
    eprintln!("errorBlocks: {n} report cases identical");
}

#[test]
fn code_frames_and_parse_error_locations_match() {
    let v = vectors();
    for case in v["excerpts"].as_array().unwrap() {
        let code = case["code"].as_str().unwrap();
        let line = case["line"].as_u64().unwrap() as u32;
        assert_eq!(
            build_excerpt(code, line),
            case["excerpt"].as_str().unwrap(),
            "line {line}"
        );
    }
    for case in v["parseErrors"].as_array().unwrap() {
        let err = &case["err"];
        let pe = ParserError {
            message: err
                .get("message")
                .and_then(Value::as_str)
                .or(err.as_str())
                .unwrap()
                .to_string(),
            loc_line: err
                .pointer("/loc/line")
                .and_then(Value::as_u64)
                .map(|x| x as u32),
            loc_column: err
                .pointer("/loc/column")
                .and_then(Value::as_u64)
                .map(|x| x as u32),
        };
        let got = describe_parse_error(&pe, case["code"].as_str().unwrap());
        assert_eq!(got, parse_failure(&case["failure"]));
    }
    // Real Babel failures: the Rust validity verdict agrees on every one,
    // and the code frame at Babel's line is byte-identical. (The message
    // text is the parser's own and is not compared — declared.)
    for case in v["realParseFailures"].as_array().unwrap() {
        let code = case["code"].as_str().unwrap();
        let ts = parse_failure(&case["failure"]);
        assert!(
            crate::output_validation::parse_failure_of(code).is_some(),
            "oxc must also reject {code:?}"
        );
        if let (Some(line), Some(excerpt)) = (ts.line, &ts.excerpt) {
            assert_eq!(&build_excerpt(code, line), excerpt);
        }
    }
    assert!(crate::output_validation::parse_failure_of("const a = 1;\n").is_none());
}

#[test]
fn semantic_invariant_messages_match() {
    let v = vectors();
    for case in v["semantics"].as_array().unwrap() {
        let m = |x: &Value| FreeNameMeasure {
            free_names: strs(&x["freeNames"]),
            total_binding_count: x["count"].as_u64().unwrap(),
        };
        let got = compare_semantics(&m(&case["before"]), &m(&case["after"]));
        let ts = &case["failure"];
        match got {
            None => assert!(ts.is_null()),
            Some(f) => {
                assert_eq!(f.message, ts["message"].as_str().unwrap());
                assert_eq!(f.added_free_names, ts.get("addedFreeNames").map(strs));
                assert_eq!(f.removed_free_names, ts.get("removedFreeNames").map(strs));
                assert_eq!(f.binding_count_before, ts["bindingCountBefore"].as_u64());
                assert_eq!(f.binding_count_after, ts["bindingCountAfter"].as_u64());
            }
        }
    }
}

#[test]
fn divergence_text_matches_over_the_ts_token_streams() {
    let v = vectors();
    for case in v["divergence"].as_array().unwrap() {
        let got = format_divergence(&strs(&case["before"]), &strs(&case["after"]));
        assert_eq!(got.as_deref(), case["text"].as_str());
        if let Some(text) = &got {
            // Every detail line is indented (the harness keeps it).
            assert!(text.lines().all(|l| l.starts_with(' ')));
        }
    }
}

#[test]
fn stage_fingerprints_match() {
    let v = vectors();
    for case in v["fingerprints"].as_array().unwrap() {
        assert_eq!(
            stage_fingerprint(case["input"].as_str().unwrap()),
            case["hash"].as_str().unwrap()
        );
    }
    // stage-fingerprint.test.ts
    assert_eq!(
        stage_fingerprint("var a = 1;"),
        stage_fingerprint("var a = 1;")
    );
    assert_ne!(
        stage_fingerprint("var a = 1;"),
        stage_fingerprint("var a = 2;")
    );
    assert_ne!(
        stage_fingerprint("var a = 1;"),
        stage_fingerprint("var a  = 1;")
    );
    let fp = stage_fingerprint("x");
    assert!(
        fp.len() == 16
            && fp
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
}

#[test]
fn selection_matches_build_pipeline_config() {
    let v = vectors();
    let cases = v["selection"].as_array().unwrap();
    for case in cases {
        let detection: humanify_model::detection::BundlerDetectionResult =
            serde_json::from_value(case["detection"].clone()).unwrap();
        let o = &case["overrides"];
        let b = o
            .get("bundlerOverride")
            .map(|x| serde_json::from_value(x.clone()).unwrap());
        let m = o
            .get("minifierOverride")
            .map(|x| serde_json::from_value(x.clone()).unwrap());
        let config = build_pipeline_config(&detection, b, m);
        let record = pipeline_selection_record(&config);
        assert_eq!(
            humanify_model::js::stringify(&record.to_js()),
            case["recordJson"].as_str().unwrap(),
            "{case}"
        );
    }
    assert_eq!(cases.len(), 30);
}

fn opts_of(v: &Value) -> CommandOptions {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let b = |k: &str| v.get(k).and_then(Value::as_bool);
    CommandOptions {
        split: b("split").unwrap_or(false),
        split_pure: b("splitPure").unwrap_or(false),
        split_ledger: s("splitLedger"),
        naming_floor: b("namingFloor"),
        naming_floor_sweep: b("namingFloorSweep"),
        bundler: s("bundler"),
        minifier: s("minifier"),
        ..CommandOptions::default()
    }
}

#[test]
fn flag_invariants_match() {
    let v = vectors();
    for case in v["invariants"].as_array().unwrap() {
        let explicit = case["explicit"].as_object().map(|e| FlagExplicitness {
            naming_floor_sweep: e.get("namingFloorSweep").and_then(Value::as_bool),
        });
        assert_eq!(
            check_flag_invariants(&opts_of(&case["opts"]), explicit),
            strs(&case["violations"]),
            "{case}"
        );
    }
}

// ---- unified.test.ts, fixture for fixture ---------------------------------

fn plain() -> CommandOptions {
    CommandOptions::default()
}

#[test]
fn unified_test_plain_run_has_no_violations() {
    assert!(check_flag_invariants(&plain(), None).is_empty());
}

#[test]
fn unified_test_split_dependents_require_split() {
    let pure = CommandOptions {
        split_pure: true,
        ..plain()
    };
    assert_eq!(
        check_flag_invariants(&pure, None),
        ["--split-pure requires --split"]
    );
    let ledger = CommandOptions {
        split_ledger: Some("ledger.json".into()),
        ..plain()
    };
    assert_eq!(
        check_flag_invariants(&ledger, None),
        ["--split-ledger requires --split"]
    );
    for mut o in [pure, ledger] {
        o.split = true;
        assert!(check_flag_invariants(&o, None).is_empty());
    }
}

#[test]
fn unified_test_sweep_rules() {
    let sweep = CommandOptions {
        naming_floor_sweep: Some(true),
        ..plain()
    };
    assert!(
        check_flag_invariants(&sweep, None).is_empty(),
        "floor on by default"
    );
    let contradiction = CommandOptions {
        naming_floor: Some(false),
        ..sweep.clone()
    };
    assert_eq!(
        check_flag_invariants(&contradiction, None),
        ["--naming-floor-sweep requires --naming-floor"]
    );
    assert!(
        check_flag_invariants(
            &contradiction,
            Some(FlagExplicitness {
                naming_floor_sweep: Some(false)
            })
        )
        .is_empty(),
        "the DEFAULT sweep gates off silently"
    );
    let reconcile = CommandOptions {
        reconcile_prior_diff: Some(true),
        ..plain()
    };
    assert!(check_flag_invariants(&reconcile, None).is_empty());
}

#[test]
fn unified_test_every_violation_at_once_in_flag_order() {
    let o = CommandOptions {
        split_pure: true,
        split_ledger: Some("ledger.json".into()),
        naming_floor_sweep: Some(true),
        naming_floor: Some(false),
        ..plain()
    };
    assert_eq!(
        check_flag_invariants(&o, None),
        [
            "--split-pure requires --split",
            "--split-ledger requires --split",
            "--naming-floor-sweep requires --naming-floor"
        ]
    );
}

#[test]
fn unified_test_enum_flag_values() {
    for b in [
        "webpack",
        "browserify",
        "rollup",
        "esbuild",
        "parcel",
        "bun",
    ] {
        let o = CommandOptions {
            bundler: Some(b.into()),
            ..plain()
        };
        assert!(check_flag_invariants(&o, None).is_empty());
    }
    for m in ["terser", "esbuild", "swc", "bun", "none"] {
        let o = CommandOptions {
            minifier: Some(m.into()),
            ..plain()
        };
        assert!(check_flag_invariants(&o, None).is_empty());
    }
    let bad = CommandOptions {
        split_pure: true,
        bundler: Some("foobar".into()),
        minifier: Some("gzip".into()),
        ..plain()
    };
    assert_eq!(
        check_flag_invariants(&bad, None),
        [
            "--split-pure requires --split",
            "--bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got \"foobar\")",
            "--minifier must be one of: terser, esbuild, swc, bun, none (got \"gzip\")"
        ]
    );
    let sentinel = CommandOptions {
        bundler: Some("unknown".into()),
        ..plain()
    };
    assert_eq!(
        check_flag_invariants(&sentinel, None),
        [
            "--bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got \"unknown\")"
        ]
    );
}

// removeConsumedSourceFile / releaseSplitSourceState (unified.test.ts):
// both serve the split (stage 11) — the first ports with it, the second is
// designed out (Rust drops the ASTs by ownership, no GC to feed).

#[test]
fn metadata_writers_match_the_ts_bytes() {
    let v = vectors();
    let w = &v["writers"];
    let dir = std::env::temp_dir().join(format!("wpb4-writers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // Order-carrying inputs: navigate the file parsed with JS object rules
    // (serde_json's Value alphabetizes keys).
    let raw = humanify_model::js::JsValue::parse(
        &std::fs::read_to_string(format!(
            "{}/../../test/parity/wpb4-vectors.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    let field =
        |v: &humanify_model::js::JsValue, k: &str| v.as_object().unwrap().get(k).unwrap().clone();
    let writer_input = |name: &str| field(&field(&field(&raw, "writers"), name), "input");
    let js = |x: &Value| humanify_model::js::JsValue::parse(&x.to_string()).unwrap();
    let hashes = StageHashes::from_js(&js(&w["stageHashes"]["input"]), "h").unwrap();
    crate::writers::write_stage_hashes(&dir, &hashes).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(".humanify/stage-hashes.json")).unwrap(),
        w["stageHashes"]["text"].as_str().unwrap()
    );
    // writePlacementStats copies the named fields only (ignoredExtra drops).
    let mut input = writer_input("placementStats");
    if let humanify_model::js::JsValue::Object(o) = &mut input {
        let kept: humanify_model::js::JsObject = o
            .entries()
            .iter()
            .filter(|(k, _)| k != "ignoredExtra")
            .cloned()
            .collect();
        *o = kept;
    }
    let stats = PlacementStats::from_js(&input, "p").unwrap();
    crate::writers::write_placement_stats(&dir, &stats).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(".humanify/placement-stats.json")).unwrap(),
        w["placementStats"]["text"].as_str().unwrap()
    );
    crate::writers::write_split_ledger(&dir, &writer_input("splitLedger")).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(".humanify/split-ledger.json")).unwrap(),
        w["splitLedger"]["text"].as_str().unwrap()
    );
    for case in w["evalStats"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let stats = humanify_model::stats::EvalStats::parse(text).unwrap();
        let dest = dir.join("nested/dir/s.json");
        crate::writers::write_eval_stats(&dest, &stats).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), text);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn metrics_of(v: &Value) -> humanify_llm::metrics::ProcessingMetrics {
    use humanify_llm::metrics::*;
    let u = |x: &Value| x.as_f64().map(|f| f as u64);
    let stage = match v["stage"].as_str().unwrap() {
        "parsing" => PipelineStage::Parsing,
        "building-graph" => PipelineStage::BuildingGraph,
        "renaming" => PipelineStage::Renaming,
        "library-params" => PipelineStage::LibraryParams,
        "library-prefix" => PipelineStage::LibraryPrefix,
        "generating" => PipelineStage::Generating,
        _ => PipelineStage::Done,
    };
    let l = &v["llm"];
    ProcessingMetrics {
        llm: LlmMetrics {
            total_calls: u(&l["totalCalls"]).unwrap(),
            in_flight_calls: l["inFlightCalls"].as_i64().unwrap(),
            completed_calls: u(&l["completedCalls"]).unwrap(),
            failed_calls: u(&l["failedCalls"]).unwrap(),
            total_tokens: u(&l["totalTokens"]),
            input_tokens: u(&l["inputTokens"]),
            output_tokens: u(&l["outputTokens"]),
            retries: u(&l["retries"]).unwrap(),
            avg_response_time_ms: l["avgResponseTimeMs"].as_f64().unwrap(),
        },
        functions: FunctionMetrics {
            total: u(&v["functions"]["total"]).unwrap(),
            completed: u(&v["functions"]["completed"]).unwrap(),
            in_progress: u(&v["functions"]["inProgress"]).unwrap(),
            pending: u(&v["functions"]["pending"]).unwrap(),
            ready: u(&v["functions"]["ready"]).unwrap(),
        },
        module_bindings: ModuleBindingMetrics {
            total: u(&v["moduleBindings"]["total"]).unwrap(),
            completed: u(&v["moduleBindings"]["completed"]).unwrap(),
            in_progress: u(&v["moduleBindings"]["inProgress"]).unwrap(),
        },
        stage,
        start_time: u(&v["startTime"]).unwrap(),
        elapsed_ms: u(&v["elapsedMs"]).unwrap(),
        estimated_remaining_ms: v["estimatedRemainingMs"].as_f64(),
        tokens_per_second: v["tokensPerSecond"].as_f64().unwrap(),
    }
}

#[test]
fn progress_sessions_write_the_ts_bytes() {
    let v = vectors();
    let mut steps_checked = 0;
    for script in v["progress"].as_array().unwrap() {
        let writes: Arc<Mutex<Vec<String>>> = Arc::default();
        let now: Arc<Mutex<u64>> = Arc::default();
        let (w, n) = (writes.clone(), now.clone());
        let sink: crate::progress::Sink = Box::new(move |s| w.lock().unwrap().push(s.to_string()));
        let clock: crate::progress::Clock = Box::new(move || *n.lock().unwrap());
        let cols = script["columns"].as_u64().unwrap() as usize;
        let mut r: Box<dyn ProgressRenderer> = if script["tty"].as_bool().unwrap() {
            Box::new(TtyRenderer::new(sink, clock, cols))
        } else {
            Box::new(LineRenderer::new(sink, clock))
        };
        let steps = script["steps"].as_array().unwrap();
        for (step, event) in steps.iter().zip(script["events"].as_array().unwrap()) {
            *now.lock().unwrap() = step["now"].as_u64().unwrap();
            let before = writes.lock().unwrap().len();
            match step["op"].as_str().unwrap() {
                "update" => r.update(&metrics_of(&step["metrics"])),
                "message" => r.message(step["text"].as_str().unwrap()),
                _ => r.finish(),
            }
            let got: Vec<String> = writes.lock().unwrap()[before..].to_vec();
            assert_eq!(got, strs(&event["writes"]), "step {step}");
            steps_checked += 1;
        }
    }
    eprintln!("progress: {steps_checked} scripted steps byte-identical");
}

#[test]
fn preserved_failed_output_matches() {
    let v = vectors();
    let fo = &v["failedOutput"];
    let dir = std::env::temp_dir().join(format!("wpb4-failed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in fo["emitted"].as_object().unwrap() {
        std::fs::write(dir.join(name), text.as_str().unwrap()).unwrap();
    }
    let failures: Vec<crate::failed_output::FailedOutputFile> = fo["failures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| crate::failed_output::FailedOutputFile {
            file_path: dir
                .join(f["file"].as_str().unwrap())
                .to_string_lossy()
                .into_owned(),
            original_code: f["originalCode"].as_str().unwrap().to_string(),
            validated_code: f["validatedCode"].as_str().map(str::to_string),
        })
        .collect();
    crate::failed_output::preserve_failed_output(&dir, &failures);
    let mut files = std::collections::BTreeMap::new();
    fn walk(
        root: &std::path::Path,
        d: &std::path::Path,
        out: &mut std::collections::BTreeMap<String, String>,
    ) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),
                    std::fs::read_to_string(&p).unwrap(),
                );
            }
        }
    }
    walk(&dir, &dir, &mut files);
    let ts: std::collections::BTreeMap<String, String> = fo["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    assert_eq!(files, ts);
    // failed-output.test.ts: an empty list writes nothing.
    let empty = dir.join("empty");
    crate::failed_output::preserve_failed_output(&empty, &[]);
    assert!(!empty.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
