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
    compare_semantics, describe_parse_error,
};
use crate::pipeline_config::pipeline_selection_record;
use crate::progress::{LineRenderer, ProgressRenderer, TtyRenderer};
use crate::report::{
    report_internal_errors, report_parse_failures, report_semantic_failures, report_vendor_naming,
};
use crate::surface::CommandOptions;
use crate::writers::stage_fingerprint;
use humanify_ag_core::naming::driver::validate::format_divergence;

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
        let count = |k: &str| case["input"][k].as_u64().unwrap();
        let stats = humanify_ag_model::stats::VendorNamingStats {
            named: count("named"),
            declined: count("declined"),
            echoed: count("echoed"),
            batches_failed: count("batchesFailed"),
        };
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

/// The TS-captured record, with ONE declared departure (finding #81,
/// review R19, 2026-10-04): the toolchain acts on a DEFINITIVE bundler
/// verdict only, so a `likely` verdict with no `--bundler` flag selects
/// as `unknown` → passthrough (the TS acted on it: webcrack / esbuild).
/// The six affected vectors are synthetic — the TS detector never emitted
/// a `likely` bundler verdict; the first real one is `installedModules`
/// alone, which used to send any file containing that word to webcrack.
fn expected_selection(
    case: &serde_json::Value,
    detection: &humanify_ag_model::detection::BundlerDetectionResult,
    bundler_flag: Option<humanify_ag_model::detection::BundlerType>,
) -> String {
    use humanify_ag_model::detection::{BundlerType, DetectionTier};
    let captured = case["recordJson"].as_str().unwrap().to_string();
    let flagged = bundler_flag.is_some_and(|b| b != BundlerType::Unknown);
    if flagged || detection.bundler.tier == DetectionTier::Definitive {
        return captured;
    }
    let mut record: serde_json::Value = serde_json::from_str(&captured).unwrap();
    record["bundler"] = "unknown".into();
    record["bundlerTier"] = "unknown".into();
    record["unpackAdapter"] = "passthrough".into();
    serde_json::to_string(&record).unwrap()
}

#[test]
fn selection_matches_the_resolved_toolchain() {
    let v = vectors();
    let cases = v["selection"].as_array().unwrap();
    for case in cases {
        let detection: humanify_ag_model::detection::BundlerDetectionResult =
            serde_json::from_value(case["detection"].clone()).unwrap();
        let o = &case["overrides"];
        let b = o
            .get("bundlerOverride")
            .map(|x| serde_json::from_value(x.clone()).unwrap());
        let m = o
            .get("minifierOverride")
            .map(|x| serde_json::from_value(x.clone()).unwrap());
        let toolchain = humanify_ag_core::toolchain::resolve_toolchain(&detection, b, m);
        let record = pipeline_selection_record(&toolchain);
        assert_eq!(
            serde_json::to_string(&record).unwrap(),
            expected_selection(case, &detection, b),
            "{case}"
        );
    }
    assert_eq!(cases.len(), 30);
}

/// The vectors' option record as the config file it now is (config sweep
/// step 5: these were flags; they are `[split]`/`[unpack]` keys).
fn toml_of(v: &Value) -> String {
    let mut out = String::new();
    let mut section = |name: &str, keys: &[(&str, &str)]| {
        let set: Vec<String> = keys
            .iter()
            .filter_map(|(json, key)| v.get(*json).map(|x| format!("{key} = {x}\n")))
            .collect();
        if !set.is_empty() {
            out.push_str(&format!("[{name}]\n{}", set.concat()));
        }
    };
    section("split", &[("split", "enabled"), ("splitPure", "pure")]);
    section(
        "unpack",
        &[("bundler", "bundler"), ("minifier", "minifier")],
    );
    out
}

/// Every refusal of a configuration, the file's own prefix dropped.
fn violations(toml: &str) -> Vec<String> {
    let f = crate::config::ConfigFile::parse("cfg.toml", toml).expect("TOML");
    let o = CommandOptions::default();
    let mut v = match crate::config::Config::build("in.js", &o, Some(&f), |_| Some("k".into())) {
        Ok(_) => Vec::new(),
        Err(e) => e
            .into_iter()
            .map(|m| m.trim_start_matches("cfg.toml: ").to_string())
            .collect(),
    };
    v.sort();
    v
}

/// A TS-era message, in today's names: the flags became keys, and an
/// empty value is refused rather than silently "unset".
fn renamed(m: &str) -> String {
    m.replace("--split-pure", "split.pure")
        .replace("--split", "split.enabled")
        .replace("--bundler", "unpack.bundler")
        .replace("--minifier", "unpack.minifier")
}

#[test]
fn flag_invariants_match() {
    let v = vectors();
    for case in v["invariants"].as_array().unwrap() {
        let toml = toml_of(&case["opts"]);
        // The ledger flag is gone: its refusals have no successor.
        let mut want: Vec<String> = strs(&case["violations"])
            .iter()
            .filter(|m| !m.contains("--split-ledger"))
            .map(|m| renamed(m))
            .collect();
        // One deliberate change: an EMPTY value is refused (config sweep
        // step 4) — the TS treated an empty bundler as an unknown name.
        if case["opts"]["bundler"] == "" {
            want = vec!["unpack.bundler must not be empty".to_string()];
        }
        want.sort();
        assert_eq!(violations(&toml), want, "{case}");
    }
}

// ---- unified.test.ts, fixture for fixture ---------------------------------

#[test]
fn unified_test_plain_run_has_no_violations() {
    assert!(violations("").is_empty());
}

#[test]
fn unified_test_split_dependents_require_split() {
    // split.enabled is ON by default since exp179: off, explicitly.
    let pure = "[split]\nenabled = false\npure = true\n";
    assert_eq!(violations(pure), ["split.pure requires split.enabled"]);
    assert!(violations(&pure.replace("enabled = false", "enabled = true")).is_empty());
    assert!(
        violations(&pure.replace("enabled = false\n", "")).is_empty(),
        "default on"
    );
}

#[test]
fn unified_test_every_violation_at_once() {
    assert_eq!(
        violations("[split]\nenabled = false\npure = true\n"),
        ["split.pure requires split.enabled"]
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
        assert!(violations(&format!("[unpack]\nbundler = \"{b}\"\n")).is_empty());
    }
    for m in ["terser", "esbuild", "swc", "bun", "none"] {
        assert!(violations(&format!("[unpack]\nminifier = \"{m}\"\n")).is_empty());
    }
    assert_eq!(
        violations(
            "[split]\nenabled = false\npure = true\n[unpack]\nbundler = \"foobar\"\nminifier = \"gzip\"\n"
        ),
        [
            "split.pure requires split.enabled",
            "unpack.bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got \"foobar\")",
            "unpack.minifier must be one of: terser, esbuild, swc, bun, none (got \"gzip\")"
        ]
    );
    assert_eq!(
        violations("[unpack]\nbundler = \"unknown\"\n"),
        [
            "unpack.bundler must be one of: webpack, browserify, rollup, esbuild, parcel, bun (got \"unknown\")"
        ]
    );
}

// removeConsumedSourceFile / releaseSplitSourceState (unified.test.ts):
// both serve the split (stage 11) — the first ports with it, the second is
// designed out (Rust drops the ASTs by ownership, no GC to feed).

#[test]
fn the_split_ledger_writer_is_compact_in_key_order() {
    // The writer is compact JSON, no trailing newline, in the ledger's key order.
    let ledger = humanify_ag_core::place::ledger::SplitLedger {
        version: 1,
        files: vec!["src/a.js".into()],
        name_to_files: [("a".to_string(), vec!["src/a.js".to_string()])]
            .into_iter()
            .collect(),
        hash_version: 3,
        ..Default::default()
    };
    assert_eq!(
        crate::writers::SPLIT_LEDGER_PATH,
        ".humanify/split-ledger.json"
    );
    assert_eq!(
        ledger.to_json(),
        "{\"version\":1,\"files\":[\"src/a.js\"],\"nameToFiles\":{\"a\":[\"src/a.js\"]},\"order\":[],\"hashes\":[],\"emitHashes\":[],\"emitNames\":[],\"hashVersion\":3}"
    );
}

fn metrics_of(v: &Value) -> humanify_ag_llm::metrics::ProcessingMetrics {
    use humanify_ag_llm::metrics::*;
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
    crate::failed_output::preserve_failed_output(&crate::output_tree::tree_for(&dir), &failures);
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
    crate::failed_output::preserve_failed_output(&crate::output_tree::tree_for(&empty), &[]);
    assert!(!empty.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
