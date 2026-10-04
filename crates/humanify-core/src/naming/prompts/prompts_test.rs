//! Ported fixture-for-fixture from src/llm/prompts.test.ts, plus byte-exact
//! snapshots per prompt type (the TS probe vectors that also ran here were
//! retired 2026-09-28).

use humanify_model::llm::{
    BatchRenameRequest, CalleeSignature, PriorReject, PriorRejects, RenameFailures, StrMap,
};

use super::*;

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
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        code,
        identifiers: &ids,
        used_names: &used,
        previous_attempt: prev,
        failures: f,
        prior_rejects: None,
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
        declarations: map(&[("ab", "var ab = 1;")]),
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

/// Round 2 (2026-10-03): a MULTI-LETTER minified name returned as itself
/// is told it IS the minified name — in the function and module retries
/// alike; a single letter keeps the generic wording.
#[test]
fn an_unchanged_minified_name_is_told_it_is_the_minified_name() {
    let line = "- \"yl\" is the minified name; suggest a descriptive name\n";
    let p = retry(
        "function f(yl) { return yl; }",
        &["yl"],
        &[],
        &map(&[("yl", "yl")]),
        &failures(&[], &[], &[], &["yl"]),
        None,
        None,
    );
    assert!(p.contains(line), "{p}");
    let m = build_module_level_retry_prefix(
        &map(&[("yl", "yl")]),
        &failures(&[], &[], &[], &["yl"]),
        None,
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(m.contains(line), "{m}");
    let z = build_module_level_retry_prefix(
        &map(&[("z", "z")]),
        &failures(&[], &[], &[], &["z"]),
        None,
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(z.contains(r#""z" was returned as itself"#), "{z}");
}

// ---- buildModuleLevelRetryPrefix ----

#[test]
fn module_prefix_renders_duplicate() {
    let p = build_module_level_retry_prefix(
        &map(&[("x", "config")]),
        &failures(&["x"], &[], &[], &[]),
        None,
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(p.contains(r#""x" was suggested as "config""#));
    assert!(p.contains("conflicts"));
}

#[test]
fn module_prefix_renders_unchanged() {
    let p = build_module_level_retry_prefix(
        &map(&[("z", "z")]),
        &failures(&[], &[], &[], &["z"]),
        None,
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(p.contains(r#""z" was returned as itself"#));
}

#[test]
fn module_prefix_renders_invalid() {
    let p = build_module_level_retry_prefix(
        &map(&[("y", "delete")]),
        &failures(&[], &["y"], &[], &[]),
        None,
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(p.contains(r#""y" was suggested as "delete""#));
    assert!(p.contains("not allowed"));
}

#[test]
fn module_prefix_includes_do_not_suggest() {
    let p = build_module_level_retry_prefix(
        &map(&[("a", "badName")]),
        &failures(&["a"], &[], &[], &[]),
        None,
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(p.contains("DO NOT suggest these names"));
    assert!(p.contains("badName"));
}

// ---- the accumulated re-ask disclosure (2026-09-29) ----

fn rejects(id: &str, failed: &[&str], invalid: &[bool]) -> PriorRejects {
    PriorRejects(vec![(
        id.to_string(),
        failed
            .iter()
            .zip(invalid)
            .map(|(n, inv)| PriorReject {
                name: n.to_string(),
                invalid: *inv,
                borrowed: None,
                held_by: None,
            })
            .collect(),
    )])
}

/// Every prior suggestion of a re-asked id gets its OWN disclosure line
/// and its own slot in the do-not-suggest block, oldest first — the
/// second re-ask of a colliding identifier tells the model everything
/// that already failed.
#[test]
fn an_accumulated_retry_discloses_every_prior_suggestion_in_order() {
    let prior = rejects("q1", &["eventHooks", "q2Named"], &[false, false]);
    let p = build_batch_rename_retry_prompt(&RetryInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        code: "var q1 = 1;",
        identifiers: &["q1".to_string()],
        used_names: &["eventHooks".to_string()],
        previous_attempt: &map(&[("q1", "q2Named")]),
        failures: &failures(&["q1"], &[], &[], &[]),
        prior_rejects: Some(&prior),
        prior_version_code: None,
        already_renamed: None,
    });
    let expected = "Your previous rename suggestions had issues:\n\
                    - \"q1\" was suggested as \"eventHooks\" but that conflicts with an existing name\n\
                    - \"q1\" was suggested as \"q2Named\" but that conflicts with an existing name\n";
    assert!(
        p.contains(expected),
        "one line per failure, oldest first: {p}"
    );
    assert!(
        p.contains("DO NOT suggest these names: eventHooks, q2Named\n"),
        "the accumulated blocklist: {p}"
    );
}

/// An accumulated entry that was rejected as an invalid target discloses
/// ITS reason too (the same wording the lane round-2 uses).
#[test]
fn an_accumulated_invalid_suggestion_discloses_its_own_reason() {
    let prior = rejects("a", &["taken", "delete"], &[false, true]);
    let p = build_module_level_retry_prefix(
        &map(&[("a", "delete")]),
        &failures(&["a"], &[], &[], &[]),
        Some(&prior),
        crate::rename::name_profile::NameProfile::Bun,
    );
    assert!(
        p.contains("- \"a\" was suggested as \"taken\" but that conflicts with an existing name\n"),
        "{p}"
    );
    assert!(
        p.contains(
            "- \"a\" was suggested as \"delete\" which is not allowed (reserved word, global built-in, or invalid syntax)\n"
        ),
        "{p}"
    );
    assert!(
        p.contains("DO NOT suggest these names: taken, delete\n"),
        "{p}"
    );
}

/// A single accumulated entry renders byte-identically to the legacy
/// `previous_attempt` shape — the first re-ask's prompt is unchanged.
#[test]
fn a_single_prior_reject_renders_the_legacy_bytes() {
    let prior = rejects("q1", &["eventHooks"], &[false]);
    let with_prior = build_batch_rename_retry_prompt(&RetryInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        code: "var q1 = 1;",
        identifiers: &["q1".to_string()],
        used_names: &["eventHooks".to_string()],
        previous_attempt: &map(&[("q1", "eventHooks")]),
        failures: &failures(&["q1"], &[], &[], &[]),
        prior_rejects: Some(&prior),
        prior_version_code: None,
        already_renamed: None,
    });
    let legacy = retry(
        "var q1 = 1;",
        &["q1"],
        &["eventHooks"],
        &map(&[("q1", "eventHooks")]),
        &failures(&["q1"], &[], &[], &[]),
        None,
        None,
    );
    assert_eq!(with_prior, legacy);
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
    assert_eq!(
        render_user_prompt(&r, crate::rename::name_profile::NameProfile::Bun),
        "verbatim"
    );
    assert_eq!(render_system_prompt(&r), "sys");
    // `if (request.userPrompt)` / `request.systemPrompt ||` — "" is falsy.
    r.user_prompt = Some(String::new());
    r.system_prompt = Some(String::new());
    assert_eq!(
        render_user_prompt(&r, crate::rename::name_profile::NameProfile::Bun),
        build_batch_rename_prompt(&r)
    );
    assert_eq!(render_system_prompt(&r), BATCH_RENAME_SYSTEM_PROMPT);
}

#[test]
fn render_takes_the_retry_path_only_with_failures() {
    let mut r = request("x", &["a"], &["u"]);
    r.is_retry = Some(true);
    assert_eq!(
        render_user_prompt(&r, crate::rename::name_profile::NameProfile::Bun),
        build_batch_rename_prompt(&r)
    );
    r.failures = Some(failures(&[], &[], &["a"], &[]));
    assert!(
        render_user_prompt(&r, crate::rename::name_profile::NameProfile::Bun)
            .starts_with("Your previous rename suggestions")
    );
    // previousAttempt `|| {}`
    assert!(
        render_user_prompt(&r, crate::rename::name_profile::NameProfile::Bun)
            .contains("MISSING from your response: a")
    );
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
        declarations: map(&[("ab", "var ab = 1;")]),
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
