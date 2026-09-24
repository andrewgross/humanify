//! Ported fixture-for-fixture from src/llm/prompts.test.ts, plus the probe
//! vectors (every builder on adversarial inputs, recorded from the real TS)
//! and byte-exact snapshots per prompt type.

use humanify_model::js::JsValue;
use humanify_model::llm::{BatchRenameRequest, CalleeSignature, RenameFailures, StrMap};

use super::*;
use crate::naming::{test_vectors, test_vectors_js};

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn map(pairs: &[(&str, &str)]) -> StrMap {
    StrMap(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

fn failures(dup: &[&str], inv: &[&str], miss: &[&str], unch: &[&str]) -> RenameFailures {
    RenameFailures {
        duplicates: strs(dup),
        invalid: strs(inv),
        missing: strs(miss),
        unchanged: strs(unch),
    }
}

fn request(code: &str, ids: &[&str], used: &[&str]) -> BatchRenameRequest {
    BatchRenameRequest {
        code: code.into(),
        identifiers: strs(ids),
        used_names: strs(used),
        ..Default::default()
    }
}

fn retry(
    code: &str,
    ids: &[&str],
    used: &[&str],
    prev: &StrMap,
    f: &RenameFailures,
    prior: Option<&str>,
    renamed: Option<&StrMap>,
) -> String {
    let ids = strs(ids);
    let used = strs(used);
    build_batch_rename_retry_prompt(&RetryInput {
        code,
        identifiers: &ids,
        used_names: &used,
        previous_attempt: prev,
        failures: f,
        prior_version_code: prior,
        already_renamed: renamed,
    })
}

// ---- all system prompts warn about global built-ins ----

#[test]
fn system_prompts_warn_about_globals() {
    assert!(BATCH_RENAME_SYSTEM_PROMPT.contains("global"));
    assert!(MODULE_LEVEL_RENAME_SYSTEM_PROMPT.contains("global"));
}

#[test]
fn system_prompts_equal_the_ts_bytes() {
    let v = test_vectors();
    assert_eq!(BATCH_RENAME_SYSTEM_PROMPT, v["systemPrompts"]["batch"]);
    assert_eq!(
        MODULE_LEVEL_RENAME_SYSTEM_PROMPT,
        v["systemPrompts"]["moduleLevel"]
    );
}

// ---- buildBatchRenameRetryPrompt ----

#[test]
fn retry_renders_duplicate_reasons() {
    let p = retry(
        "function f(x, y) { return x + y; }",
        &["x", "y"],
        &["config"],
        &map(&[("x", "config"), ("y", "total")]),
        &failures(&["x"], &[], &[], &[]),
        None,
        None,
    );
    assert!(p.contains(r#""x" was suggested as "config""#));
    assert!(p.contains("conflicts"));
}

#[test]
fn retry_renders_unchanged_with_emphasis() {
    let p = retry(
        "function f(z) { return z; }",
        &["z"],
        &[],
        &map(&[("z", "z")]),
        &failures(&[], &[], &[], &["z"]),
        None,
        None,
    );
    assert!(p.contains(r#""z" was returned as itself"#));
    assert!(p.contains("MUST suggest a DIFFERENT name"));
}

#[test]
fn retry_renders_invalid_with_suggestion() {
    let p = retry(
        "function f(y) {}",
        &["y"],
        &[],
        &map(&[("y", "123bad")]),
        &failures(&[], &["y"], &[], &[]),
        None,
        None,
    );
    assert!(p.contains(r#""y" was suggested as "123bad""#));
    assert!(p.contains("not allowed"));
}

#[test]
fn retry_renders_global_builtin_rejection() {
    let p = retry(
        "function f(x) { return createTypeChecker('Date'); }",
        &["x"],
        &[],
        &map(&[("x", "Date")]),
        &failures(&[], &["x"], &[], &[]),
        None,
        None,
    );
    assert!(p.contains(r#""x" was suggested as "Date""#));
    assert!(p.contains("DO NOT suggest these names"));
    assert!(p.contains("Date"));
}

#[test]
fn retry_includes_do_not_suggest_list() {
    let p = retry(
        "function f(a, b) {}",
        &["a", "b"],
        &["config"],
        &map(&[("a", "config"), ("b", "b")]),
        &failures(&["a"], &[], &[], &["b"]),
        None,
        None,
    );
    assert!(p.contains("DO NOT suggest these names"));
    assert!(p.contains("config"));
}

#[test]
fn retry_lists_missing_identifiers() {
    let p = retry(
        "function f(a) {}",
        &["a"],
        &[],
        &StrMap::default(),
        &failures(&[], &[], &["a"], &[]),
        None,
        None,
    );
    assert!(p.contains("MISSING"));
}

// ---- buildModuleLevelRenameBody used-names cap ----

#[test]
fn module_body_caps_used_names() {
    let used: Vec<String> = (0..8000).map(|i| format!("descriptiveName{i}")).collect();
    let input = ModuleLevelInput {
        declarations: strs(&["var ab = 1;"]),
        assignment_context: ArrayRecord(vec![("ab".into(), vec![])]),
        usage_examples: ArrayRecord(vec![("ab".into(), vec![])]),
        identifiers: strs(&["ab"]),
        used_names: used,
        suggested_names: None,
    };
    let body = build_module_level_rename_body(&input, |_| false);
    let line = body
        .split('\n')
        .find(|l| l.starts_with("Names already in use"))
        .expect("used-names line");
    let listed = line.split(':').nth(1).unwrap().split(',').count();
    assert!(listed <= 200, "got {listed}");
}

// ---- buildModuleLevelRetryPrefix ----

#[test]
fn module_prefix_renders_duplicate() {
    let p =
        build_module_level_retry_prefix(&map(&[("x", "config")]), &failures(&["x"], &[], &[], &[]));
    assert!(p.contains(r#""x" was suggested as "config""#));
    assert!(p.contains("conflicts"));
}

#[test]
fn module_prefix_renders_unchanged() {
    let p = build_module_level_retry_prefix(&map(&[("z", "z")]), &failures(&[], &[], &[], &["z"]));
    assert!(p.contains(r#""z" was returned as itself"#));
}

#[test]
fn module_prefix_renders_invalid() {
    let p =
        build_module_level_retry_prefix(&map(&[("y", "delete")]), &failures(&[], &["y"], &[], &[]));
    assert!(p.contains(r#""y" was suggested as "delete""#));
    assert!(p.contains("not allowed"));
}

#[test]
fn module_prefix_includes_do_not_suggest() {
    let p = build_module_level_retry_prefix(
        &map(&[("a", "badName")]),
        &failures(&["a"], &[], &[], &[]),
    );
    assert!(p.contains("DO NOT suggest these names"));
    assert!(p.contains("badName"));
}

// ---- buildBatchRenameRetryPrompt alreadyRenamed context ----

#[test]
fn retry_includes_already_renamed() {
    let p = retry(
        "function f(a, b, c, d) { return a + b + c + d; }",
        &["c", "d"],
        &["parentDom", "newChildren"],
        &map(&[("c", "c"), ("d", "d")]),
        &failures(&[], &[], &[], &["c", "d"]),
        None,
        Some(&map(&[("a", "parentDom"), ("b", "newChildren")])),
    );
    assert!(p.contains("already renamed"));
    assert!(p.contains("a → parentDom"));
    assert!(p.contains("b → newChildren"));
}

#[test]
fn retry_omits_already_renamed_when_absent() {
    let p = retry(
        "function f(a, b) { return a + b; }",
        &["a", "b"],
        &[],
        &map(&[("a", "a"), ("b", "b")]),
        &failures(&[], &[], &[], &["a", "b"]),
        None,
        None,
    );
    assert!(!p.contains("already renamed"));
}

// ---- buildBatchRenamePrompt prior-version context ----

#[test]
fn batch_includes_prior_version_section() {
    let mut r = request(
        "function a(b, c) { return b; }",
        &["a", "b", "c"],
        &["existing"],
    );
    r.prior_version_code =
        Some("function handleError(error, currentFiber) { return error; }".into());
    let p = build_batch_rename_prompt(&r);
    for needle in [
        "prior version",
        "handleError",
        "currentFiber",
        "MUST reuse",
        "conflict",
    ] {
        assert!(p.contains(needle), "{needle}");
    }
}

#[test]
fn batch_omits_prior_version_without_context() {
    let p = build_batch_rename_prompt(&request("function a(b) { return b; }", &["a", "b"], &[]));
    assert!(!p.contains("prior version"));
}

#[test]
fn batch_lists_prior_names_for_mechanical_reuse() {
    let mut r = request("function a(b, c) { return b; }", &["a", "b", "c"], &[]);
    r.prior_version_code = Some("function handleError(error) { return error; }".into());
    r.prior_version_names = Some(strs(&["handleError", "error", "currentFiber"]));
    let p = build_batch_rename_prompt(&r);
    assert!(p.contains("handleError, error, currentFiber"));
    assert!(p.to_lowercase().contains("reuse these names"));
}

#[test]
fn batch_renders_already_transferred_names() {
    let mut r = request(
        "function a(b) { const userId = b.id; return userId; }",
        &["a", "b"],
        &[],
    );
    r.prior_version_code =
        Some("function getUser(request) { const userId = request.id; return userId; }".into());
    r.already_renamed = Some(map(&[("t", "userId")]));
    let p = build_batch_rename_prompt(&r);
    assert!(p.contains("t → userId"));
    assert!(p.to_lowercase().contains("already renamed"));
}

// ---- buildBatchRenamePrompt per-identifier prior-name hints ----

#[test]
fn batch_renders_per_identifier_hints() {
    let mut r = request(
        "function a(b) { const c = b.foo(); return c; }",
        &["a", "b", "c"],
        &[],
    );
    r.prior_version_code = Some(
        "function loadConfig(source) { const caughtError = source.foo(); return caughtError; }"
            .into(),
    );
    r.prior_name_hints = Some(map(&[("c", "caughtError"), ("b", "source")]));
    let p = build_batch_rename_prompt(&r);
    assert!(p.contains("c → caughtError"));
    assert!(p.contains("b → source"));
    assert!(p.contains("unless the identifier's role changed"));
}

#[test]
fn batch_hints_only_identifiers_being_renamed() {
    let mut r = request("function a(b) { return b; }", &["a", "b"], &[]);
    r.prior_version_code = Some("function f(x) { return x; }".into());
    r.prior_name_hints = Some(map(&[("b", "source"), ("zzz", "unrelatedPrior")]));
    assert!(!build_batch_rename_prompt(&r).contains("unrelatedPrior"));
}

#[test]
fn batch_without_hints_has_no_hint_block() {
    let mut r = request("function a(b) { return b; }", &["a", "b"], &[]);
    r.prior_version_code = Some("function f(x) { return x; }".into());
    assert!(!build_batch_rename_prompt(&r).contains("named as follows in the prior version"));
}

// ---- render selection (openai-compatible.ts buildBatchUserPrompt) ----

#[test]
fn render_prefers_a_non_empty_user_prompt_and_system_override() {
    let mut r = request("x", &["a"], &[]);
    r.user_prompt = Some("verbatim".into());
    r.system_prompt = Some("sys".into());
    assert_eq!(render_user_prompt(&r), "verbatim");
    assert_eq!(render_system_prompt(&r), "sys");
    // `if (request.userPrompt)` / `request.systemPrompt ||` — "" is falsy.
    r.user_prompt = Some(String::new());
    r.system_prompt = Some(String::new());
    assert_eq!(render_user_prompt(&r), build_batch_rename_prompt(&r));
    assert_eq!(render_system_prompt(&r), BATCH_RENAME_SYSTEM_PROMPT);
}

#[test]
fn render_takes_the_retry_path_only_with_failures() {
    let mut r = request("x", &["a"], &["u"]);
    r.is_retry = Some(true);
    assert_eq!(render_user_prompt(&r), build_batch_rename_prompt(&r));
    r.failures = Some(failures(&[], &[], &["a"], &[]));
    assert!(render_user_prompt(&r).starts_with("Your previous rename suggestions"));
    // previousAttempt `|| {}`
    assert!(render_user_prompt(&r).contains("MISSING from your response: a"));
}

// ---- probe vectors: every builder, byte-exact ----
//
// Read through the JS-semantics parser (`JsValue`), not serde_json::Value:
// the latter sorts object keys, and `alreadyRenamed`'s enumeration order
// is part of the prompt bytes.

fn field<'a>(v: &'a JsValue, key: &str) -> &'a JsValue {
    v.as_object()
        .and_then(|o| o.get(key))
        .unwrap_or(&JsValue::Null)
}

fn items(v: &JsValue) -> &[JsValue] {
    match v {
        JsValue::Array(a) => a,
        _ => panic!("expected an array, got {v:?}"),
    }
}

fn text(v: &JsValue) -> &str {
    v.as_str().unwrap()
}

fn opt_str(v: &JsValue) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn str_vec(v: &JsValue) -> Vec<String> {
    items(v).iter().map(|s| text(s).to_string()).collect()
}

fn opt_vec(v: &JsValue) -> Option<Vec<String>> {
    (*v != JsValue::Null).then(|| str_vec(v))
}

fn str_map(v: &JsValue) -> StrMap {
    StrMap(
        v.as_object()
            .unwrap()
            .entries()
            .iter()
            .map(|(k, v)| (k.clone(), text(v).to_string()))
            .collect(),
    )
}

fn opt_map(v: &JsValue) -> Option<StrMap> {
    (*v != JsValue::Null).then(|| str_map(v))
}

fn array_record(v: &JsValue) -> ArrayRecord {
    ArrayRecord(
        v.as_object()
            .unwrap()
            .entries()
            .iter()
            .map(|(k, v)| (k.clone(), str_vec(v)))
            .collect(),
    )
}

fn fails(v: &JsValue) -> RenameFailures {
    RenameFailures {
        duplicates: str_vec(field(v, "duplicates")),
        invalid: str_vec(field(v, "invalid")),
        missing: str_vec(field(v, "missing")),
        unchanged: str_vec(field(v, "unchanged")),
    }
}

fn callees(v: &JsValue) -> Vec<CalleeSignature> {
    items(v)
        .iter()
        .map(|c| CalleeSignature {
            name: text(field(c, "name")).to_string(),
            params: str_vec(field(c, "params")),
            snippet: None,
        })
        .collect()
}

fn arg(args: &[JsValue], i: usize) -> &JsValue {
    args.get(i).unwrap_or(&JsValue::Null)
}

fn eligibility(name: &str) -> fn(&str) -> bool {
    match name {
        "short" => |n| n.chars().map(char::len_utf16).sum::<usize>() <= 3,
        "all" => |_| true,
        "none" => |_| false,
        other => panic!("unknown predicate {other}"),
    }
}

fn module_input(a: &[JsValue]) -> ModuleLevelInput {
    ModuleLevelInput {
        declarations: str_vec(&a[0]),
        assignment_context: array_record(&a[1]),
        usage_examples: array_record(&a[2]),
        identifiers: str_vec(&a[3]),
        used_names: str_vec(&a[4]),
        suggested_names: opt_map(arg(a, 6)),
    }
}

fn run_retry_vector(fname: &str, a: &[JsValue]) -> String {
    let (ids, used) = (str_vec(&a[1]), str_vec(&a[2]));
    let (prev, f, renamed) = (str_map(&a[3]), fails(&a[4]), opt_map(arg(a, 6)));
    let input = RetryInput {
        code: text(&a[0]),
        identifiers: &ids,
        used_names: &used,
        previous_attempt: &prev,
        failures: &f,
        prior_version_code: arg(a, 5).as_str(),
        already_renamed: renamed.as_ref(),
    };
    if fname == "buildBatchRenameRetryBody" {
        build_batch_rename_retry_body(&input)
    } else {
        build_batch_rename_retry_prompt(&input)
    }
}

fn run_vector(fname: &str, a: &[JsValue]) -> String {
    match fname {
        "buildBatchRenamePrompt" => build_batch_rename_prompt(&BatchRenameRequest {
            code: text(&a[0]).into(),
            identifiers: str_vec(&a[1]),
            used_names: str_vec(&a[2]),
            callee_signatures: callees(&a[3]),
            callsites: str_vec(&a[4]),
            context_vars: opt_vec(arg(a, 5)),
            prior_version_code: opt_str(arg(a, 6)),
            prior_version_names: opt_vec(arg(a, 7)),
            already_renamed: opt_map(arg(a, 8)),
            prior_name_hints: opt_map(arg(a, 9)),
            ..Default::default()
        }),
        "buildBatchRenameRetryPrompt" | "buildBatchRenameRetryBody" => run_retry_vector(fname, a),
        "buildRenameResponseInstruction" => build_rename_response_instruction(&str_vec(&a[0])),
        "buildModuleLevelRenamePrompt" => {
            build_module_level_rename_prompt(&module_input(a), eligibility(text(&a[5])))
        }
        "buildModuleLevelRenameBody" => {
            build_module_level_rename_body(&module_input(a), eligibility(text(&a[5])))
        }
        "buildModuleLevelRetryPrefix" => {
            build_module_level_retry_prefix(&str_map(&a[0]), &fails(&a[1]))
        }
        other => panic!("unknown builder {other}"),
    }
}

#[test]
fn probe_vectors_match_the_ts_byte_for_byte() {
    let v = test_vectors_js();
    let cases = items(field(&v, "prompts"));
    assert!(cases.len() >= 16);
    for c in cases {
        let got = run_vector(text(field(c, "fn")), items(field(c, "args")));
        assert_eq!(
            got,
            text(field(c, "out")),
            "case {}",
            text(field(c, "name"))
        );
    }
}

// ---- snapshots, one per prompt type (inline; any byte change is loud) ----

#[test]
fn snapshot_first_round_function_prompt() {
    let mut r = request("function a(b) {\n  return b;\n}", &["a", "b"], &["x", "y"]);
    r.callee_signatures = vec![CalleeSignature {
        name: "g".into(),
        params: strs(&["p", "...q"]),
        snippet: Some("return p;".into()),
    }];
    r.callsites = strs(&["a(1);"]);
    r.context_vars = Some(strs(&["var k = 1;"]));
    assert_eq!(
        build_batch_rename_prompt(&r),
        "Analyze this function and suggest descriptive names for ALL listed identifiers:\n\n\
```javascript\nfunction a(b) {\n  return b;\n}\n```\n\n\
Identifiers to rename: a, b\n\n\
Surrounding scope variables (for context only, do NOT rename these):\n  var k = 1;\n\n\
This function calls:\n- g(p, ...q)\n\n\
This function is called as:\n- a(1);\n\n\
Names already in use (MUST avoid these): x, y\n\n\
You MUST respond with a JSON object containing exactly 2 mappings — one for each identifier listed above:\n\
{ \"a\": \"descriptiveName\", \"b\": \"descriptiveName\" }"
    );
}

#[test]
fn snapshot_retry_function_prompt() {
    let got = retry(
        "a(b);",
        &["b"],
        &["taken"],
        &map(&[("b", "taken")]),
        &failures(&["b"], &[], &[], &[]),
        None,
        None,
    );
    assert_eq!(
        got,
        "Your previous rename suggestions had issues:\n\
- \"b\" was suggested as \"taken\" but that conflicts with an existing name\n\
\nDO NOT suggest these names: taken\n\
\n\
\nPlease suggest DIFFERENT names for these remaining identifiers:\n\n\
```javascript\na(b);\n```\n\n\
Identifiers still needing names: b\n\n\
Names already in use (MUST avoid ALL of these): taken\n\
\nRespond with JSON mapping each identifier to a UNIQUE name:\n\
{ \"b\": \"descriptiveName\" }"
    );
}

#[test]
fn snapshot_module_level_prompt() {
    let input = ModuleLevelInput {
        declarations: strs(&["var ab = 1;"]),
        assignment_context: ArrayRecord(vec![("ab".into(), strs(&["ab = 2;"]))]),
        usage_examples: ArrayRecord(vec![("ab".into(), strs(&["f(ab)"]))]),
        identifiers: strs(&["ab"]),
        used_names: strs(&["ab", "fetchThing"]),
        suggested_names: Some(map(&[("ab", "count")])),
    };
    assert_eq!(
        build_module_level_rename_prompt(&input, |n| n.len() <= 2),
        "Analyze these top-level module identifiers and suggest descriptive names.\n\n\
Where a \"Prior version name\" is shown, strongly prefer that name unless the binding's purpose has fundamentally changed in this version.\n\n\
Identifier: ab\n  Prior version name: count\n  Declaration: var ab = 1;\n  Assignments:\n    ab = 2;\n  Usage:\n    f(ab)\n\n\
Identifiers to rename: ab\n\n\
Names already in use (MUST avoid these): fetchThing\n\n\
Respond with JSON mapping EVERY identifier to a new name:\n{ \"ab\": \"descriptiveName\" }"
    );
}
