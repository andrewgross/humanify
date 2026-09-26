//! WP4.2's byte-identity gate, kept as a unit-test golden since the cutover:
//! the live prompt builders replayed against the frozen TS-captured fixture
//! (test/parity/wp42-gate-fixture/). Test-only (`#[cfg(test)]`).
//!
//! Four sections per pair:
//!
//! - **prompts** — every row of the oracle dump's `prompts.jsonl`,
//!   rebuilt from its typed request (the dump's `cache-keys.jsonl`, joined
//!   by `seq`) through [`render_system_prompt`] / [`render_user_prompt`];
//!   both strings must equal the TS's bytes. A function retry's
//!   `promptBody` (cache-key material no prompt shows) is rebuilt too.
//!   The request carries every field a prompt reads (the dump drops only
//!   the callee `snippet`, which no prompt renders — 16-findings #7/#8).
//! - **module** — a module-level request's `userPrompt` is built from
//!   inputs no dump carries (declarations, assignment/usage context, prior
//!   names, the eligibility predicate). The capture hook
//!   (test/parity/wp42-capture-hook.mjs) records them at build time and
//!   joins them to the dispatch; every module-level dispatch is rebuilt
//!   from them, and every recorded builder CALL is rebuilt as well.
//! - **code-window** — every `selectFunctionCode` / `capContextCode` call
//!   the TS made, from its captured inputs.
//! - **context** — every `buildContext` call, from the scope view the hook
//!   captured at call time: callee signatures (incl. the snippet), call
//!   sites, context vars byte-exact; used identifiers by count + sha256
//!   of the ordered list (the list is ~25k names per call).
//!
//! The capture's own `requests.jsonl` must reproduce the dump's key
//! sequence (same dispatches, same order), which is what ties the
//! captured inputs to the oracle's prompts.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use humanify_model::llm::{BatchRenameRequest, RenameFailures, StrMap};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::code_window::{FunctionCodeSelection, cap_context_code, select_function_code};
use super::context::{CalleeView, ContextView, DeclView, LlmContext, ParentBinding, build_context};
use super::prompts::{
    ArrayRecord, MODULE_LEVEL_RENAME_SYSTEM_PROMPT, ModuleLevelInput, RetryInput,
    build_batch_rename_retry_body, build_module_level_rename_body,
    build_module_level_rename_prompt, build_module_level_retry_prefix, render_system_prompt,
    render_user_prompt,
};

/// One section's tally: rows compared, rows identical, the first
/// divergences (row id + what differed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    pub compared: usize,
    pub identical: usize,
    pub divergences: Vec<String>,
}

impl Section {
    fn check(&mut self, id: impl FnOnce() -> String, ok: bool) {
        self.compared += 1;
        if ok {
            self.identical += 1;
        } else if self.divergences.len() < 20 {
            self.divergences.push(id());
        }
    }

    pub fn is_identical(&self) -> bool {
        self.compared == self.identical
    }

    fn line(&self, name: &str) -> String {
        format!("{name} {}/{}", self.identical, self.compared)
    }
}

/// The gate's per-pair report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptGateReport {
    /// Dump rows by kind: first-round function prompts, function retries,
    /// and rows whose userPrompt is sent verbatim (module-level, sweep,
    /// folders, vendor — their builders are gated by `module` or live
    /// outside WP4.2).
    pub first_round: usize,
    pub retries: usize,
    pub verbatim: usize,
    pub verbatim_by_site: Vec<(String, usize)>,
    pub system_prompts: Section,
    pub user_prompts: Section,
    pub retry_bodies: Section,
    /// None when no capture was given.
    pub capture: Option<CaptureReport>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureReport {
    /// The capture's key sequence equals the dump's.
    pub key_sequence_matches: bool,
    pub module_dispatches: Section,
    pub module_builder_calls: Section,
    pub code_window: Section,
    pub context: Section,
    /// How often each non-trivial path ran (a green section over paths
    /// that never ran would prove nothing about them).
    pub exercised: Vec<(&'static str, usize)>,
}

impl CaptureReport {
    fn bump(&mut self, path: &'static str) {
        match self.exercised.iter_mut().find(|(p, _)| *p == path) {
            Some((_, n)) => *n += 1,
            None => self.exercised.push((path, 1)),
        }
    }
}

impl PromptGateReport {
    pub fn identical(&self) -> bool {
        let dump = self.system_prompts.is_identical()
            && self.user_prompts.is_identical()
            && self.retry_bodies.is_identical();
        let capture = self.capture.as_ref().is_none_or(|c| {
            c.key_sequence_matches
                && c.module_dispatches.is_identical()
                && c.module_builder_calls.is_identical()
                && c.code_window.is_identical()
                && c.context.is_identical()
        });
        dump && capture
    }

    pub fn summary(&self) -> String {
        let sites: Vec<String> = self
            .verbatim_by_site
            .iter()
            .map(|(s, n)| format!("{s}={n}"))
            .collect();
        let mut out = format!(
            "rows {} (first-round {}, retry {}, verbatim userPrompt {} [{}]) | {} | {} | {}",
            self.first_round + self.retries + self.verbatim,
            self.first_round,
            self.retries,
            self.verbatim,
            sites.join(" "),
            self.system_prompts.line("systemPrompt"),
            self.user_prompts.line("userPrompt"),
            self.retry_bodies.line("retryPromptBody"),
        );
        if let Some(c) = &self.capture {
            out += &format!(
                " || capture key-sequence {} | {} | {} | {} | {}",
                if c.key_sequence_matches {
                    "IDENTICAL"
                } else {
                    "DIFFERS"
                },
                c.module_dispatches.line("module-dispatch"),
                c.module_builder_calls.line("module-builder-call"),
                c.code_window.line("code-window"),
                c.context.line("context"),
            );
            let paths: Vec<String> = c
                .exercised
                .iter()
                .map(|(p, n)| format!("{p}={n}"))
                .collect();
            out += &format!(" | exercised [{}]", paths.join(" "));
        }
        out + if self.identical() {
            " => IDENTICAL"
        } else {
            " => DIVERGES"
        }
    }
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rows = Vec::new();
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
        if line.is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str(&line)
                .map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?,
        );
    }
    Ok(rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptRow {
    seq: u64,
    site: String,
    is_retry: bool,
    system_prompt: String,
    user_prompt: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyRow {
    seq: u64,
    request: BatchRenameRequest,
    cache_key: String,
}

fn is_function_retry(r: &BatchRenameRequest) -> bool {
    r.is_retry == Some(true)
        && r.failures.is_some()
        && r.user_prompt.as_deref().is_none_or(str::is_empty)
}

/// The rebuilt tail-less retry body of a function retry request.
fn retry_body_of(r: &BatchRenameRequest) -> Option<String> {
    let failures = r.failures.as_ref()?;
    let empty = StrMap::default();
    Some(build_batch_rename_retry_body(&RetryInput {
        code: &r.code,
        identifiers: &r.identifiers,
        used_names: &r.used_names,
        previous_attempt: r.previous_attempt.as_ref().unwrap_or(&empty),
        failures,
        prior_version_code: r.prior_version_code.as_deref(),
        already_renamed: r.already_renamed.as_ref(),
    }))
}

fn classify(report: &mut PromptGateReport, row: &PromptRow, req: &BatchRenameRequest) {
    if req.user_prompt.as_deref().is_some_and(|u| !u.is_empty()) {
        report.verbatim += 1;
        match report
            .verbatim_by_site
            .iter_mut()
            .find(|(s, _)| *s == row.site)
        {
            Some((_, n)) => *n += 1,
            None => report.verbatim_by_site.push((row.site.clone(), 1)),
        }
    } else if is_function_retry(req) {
        report.retries += 1;
    } else {
        report.first_round += 1;
    }
}

/// Section "prompts": the oracle dump alone.
fn gate_dump(dump: &Path, report: &mut PromptGateReport) -> Result<Vec<String>, String> {
    let prompts: Vec<PromptRow> = read_jsonl(&dump.join("prompts.jsonl"))?;
    let keys: Vec<KeyRow> = read_jsonl(&dump.join("cache-keys.jsonl"))?;
    if prompts.len() != keys.len() {
        return Err(format!(
            "prompts.jsonl has {} rows, cache-keys.jsonl {}",
            prompts.len(),
            keys.len()
        ));
    }
    for (row, key) in prompts.iter().zip(&keys) {
        if row.seq != key.seq {
            return Err(format!("seq misaligned: {} vs {}", row.seq, key.seq));
        }
        let req = &key.request;
        classify(report, row, req);
        if row.is_retry != (req.is_retry == Some(true)) {
            return Err(format!("seq {}: isRetry disagrees", row.seq));
        }
        let seq = row.seq;
        report.system_prompts.check(
            || format!("seq {seq}"),
            render_system_prompt(req) == row.system_prompt,
        );
        report.user_prompts.check(
            || format!("seq {seq} (site {})", row.site),
            render_user_prompt(req) == row.user_prompt,
        );
        // A WAVE retry drops its body (processor.ts executeWaveRetry: the
        // body predates the winners merged into alreadyRenamed, so it is
        // unset before dispatch); only the per-node retry rounds carry one.
        if is_function_retry(req) && req.prompt_body.is_some() {
            report.retry_bodies.check(
                || format!("seq {seq}"),
                retry_body_of(req) == req.prompt_body,
            );
        }
    }
    Ok(keys.into_iter().map(|k| k.cache_key).collect())
}

// ---- the capture ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedModuleInputs {
    declarations: Vec<String>,
    assignment_context: ArrayRecord,
    usage_examples: ArrayRecord,
    identifiers: Vec<String>,
    used_names: Vec<String>,
    used_eligible: Vec<bool>,
    #[serde(default)]
    suggested_names: Option<StrMap>,
}

impl CapturedModuleInputs {
    fn input(&self) -> ModuleLevelInput {
        ModuleLevelInput {
            declarations: self.declarations.clone(),
            assignment_context: self.assignment_context.clone(),
            usage_examples: self.usage_examples.clone(),
            identifiers: self.identifiers.clone(),
            used_names: self.used_names.clone(),
            suggested_names: self.suggested_names.clone(),
        }
    }

    /// The TS's `isEligible`, replayed from its captured answers (the
    /// builder only asks about `usedNames` members).
    fn eligibility(&self) -> impl Fn(&str) -> bool + '_ {
        move |name: &str| {
            let i = self
                .used_names
                .iter()
                .position(|n| n == name)
                .unwrap_or_else(|| panic!("eligibility asked about unrecorded name {name:?}"));
            self.used_eligible[i]
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedRetry {
    previous_attempt: StrMap,
    failures: RenameFailures,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedModuleJoin {
    prompt: CapturedModuleInputs,
    #[serde(default)]
    retry_prefix: Option<CapturedRetry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestRow {
    seq: u64,
    request: BatchRenameRequest,
    cache_key: String,
    #[serde(default)]
    module_inputs: Option<CapturedModuleJoin>,
}

fn rebuild_module_dispatch(join: &CapturedModuleJoin) -> (String, Option<String>) {
    let input = join.prompt.input();
    let eligible = join.prompt.eligibility();
    let prompt = build_module_level_rename_prompt(&input, &eligible);
    match &join.retry_prefix {
        None => (prompt, None),
        Some(r) => {
            let prefix = build_module_level_retry_prefix(&r.previous_attempt, &r.failures);
            let body = build_module_level_rename_body(&input, &eligible);
            (
                format!("{prefix}\n{prompt}"),
                Some(format!("{prefix}\n{body}")),
            )
        }
    }
}

fn gate_module_dispatches(rows: &[RequestRow], report: &mut CaptureReport) {
    for row in rows {
        let req = &row.request;
        if req.system_prompt.as_deref() != Some(MODULE_LEVEL_RENAME_SYSTEM_PROMPT) {
            continue;
        }
        if row
            .module_inputs
            .as_ref()
            .is_some_and(|j| j.retry_prefix.is_some())
        {
            report.bump("module-retry");
        }
        if req.prompt_body.is_some() {
            report.bump("module-retry-body");
        }
        let seq = row.seq;
        let ok = row.module_inputs.as_ref().is_some_and(|join| {
            let (user, body) = rebuild_module_dispatch(join);
            // A wave retry unsets its body before dispatch (as for
            // function retries); a present body must match.
            req.user_prompt.as_deref() == Some(user.as_str())
                && (req.prompt_body.is_none() || body == req.prompt_body)
        });
        report.module_dispatches.check(
            || {
                format!(
                    "seq {seq} (inputs captured: {})",
                    row.module_inputs.is_some()
                )
            },
            ok,
        );
    }
}

#[derive(Deserialize)]
struct BuilderCall {
    seq: u64,
    #[serde(rename = "fn")]
    function: String,
    inputs: serde_json::Value,
    out: String,
}

fn gate_module_builder_calls(path: &Path, section: &mut Section) -> Result<(), String> {
    for call in read_jsonl::<BuilderCall>(path)? {
        let rebuilt = match call.function.as_str() {
            "buildModuleLevelRetryPrefix" => {
                let r: CapturedRetry =
                    serde_json::from_value(call.inputs).map_err(|e| e.to_string())?;
                build_module_level_retry_prefix(&r.previous_attempt, &r.failures)
            }
            f => {
                let inputs: CapturedModuleInputs =
                    serde_json::from_value(call.inputs).map_err(|e| e.to_string())?;
                let (input, eligible) = (inputs.input(), inputs.eligibility());
                if f == "buildModuleLevelRenamePrompt" {
                    build_module_level_rename_prompt(&input, eligible)
                } else {
                    build_module_level_rename_body(&input, eligible)
                }
            }
        };
        let (seq, f) = (call.seq, call.function.clone());
        section.check(|| format!("call {seq} ({f})"), rebuilt == call.out);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedSelection {
    code: String,
    session_id: String,
    fn_start_line: Option<i64>,
    fn_end_line: Option<i64>,
    anchor_start_lines: Option<Vec<Option<i64>>>,
    identifier_names: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeWindowRow {
    seq: u64,
    #[serde(rename = "fn")]
    function: String,
    #[serde(default)]
    sel: Option<CapturedSelection>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    out: String,
}

fn gate_code_window(path: &Path, report: &mut CaptureReport) -> Result<(), String> {
    for row in read_jsonl::<CodeWindowRow>(path)? {
        let input_len = row.sel.as_ref().map_or(0, |s| s.code.len());
        if row.out.len() != input_len && row.sel.is_some() {
            report.bump(if row.out.contains("[truncated]") {
                "code-window-flat-cut"
            } else {
                "code-window-windowed"
            });
        }
        if row.sel.is_none() && row.code.as_deref() != Some(row.out.as_str()) {
            report.bump("cap-context-capped");
        }
        let rebuilt = match (&row.sel, &row.code) {
            (Some(s), _) => select_function_code(&FunctionCodeSelection {
                code: &s.code,
                session_id: &s.session_id,
                fn_start_line: s.fn_start_line,
                fn_end_line: s.fn_end_line,
                anchor_start_lines: s.anchor_start_lines.as_deref(),
                identifier_names: s.identifier_names.as_deref(),
            }),
            (None, Some(code)) => cap_context_code(code, row.session_id.as_deref().unwrap_or("")),
            _ => return Err(format!("code-window row {} has no input", row.seq)),
        };
        let (seq, f) = (row.seq, row.function.clone());
        report
            .code_window
            .check(|| format!("call {seq} ({f})"), rebuilt == row.out);
    }
    Ok(())
}

#[derive(Deserialize)]
struct ScopeSnapshot {
    id: u64,
    names: Vec<String>,
    globals: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ScopeEntry {
    Names(Vec<String>),
    Ref { r#ref: u64 },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedBinding {
    name: String,
    eligible: bool,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedView {
    callees: Vec<CalleeView>,
    scope_chain: Vec<ScopeEntry>,
    program_scope: u64,
    parent_pending: bool,
    parent_bindings: Option<Vec<CapturedBinding>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedCallee {
    name: String,
    params: Vec<String>,
    snippet: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapturedContextOut {
    callee_signatures: Vec<CapturedCallee>,
    callsites: Vec<String>,
    used_identifiers_count: usize,
    used_identifiers_sha: String,
    context_vars: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextRow {
    seq: u64,
    session_id: String,
    view: CapturedView,
    out: CapturedContextOut,
}

fn decl_view(b: &CapturedBinding) -> DeclView {
    let code = b.code.clone().unwrap_or_default();
    match b.kind.as_deref() {
        Some("fnOrClass") => DeclView::FunctionOrClass,
        Some("declarator") => DeclView::Declarator { code },
        _ => DeclView::Other { code },
    }
}

fn context_view(
    v: &CapturedView,
    scopes: &HashMap<u64, ScopeSnapshot>,
) -> Result<ContextView, String> {
    let snapshot = |id: u64| {
        scopes
            .get(&id)
            .ok_or_else(|| format!("scope snapshot {id} missing"))
    };
    let mut chain = Vec::with_capacity(v.scope_chain.len());
    for entry in &v.scope_chain {
        chain.push(match entry {
            ScopeEntry::Names(names) => names.clone(),
            ScopeEntry::Ref { r#ref } => snapshot(*r#ref)?.names.clone(),
        });
    }
    let program = snapshot(v.program_scope)?;
    let parent_bindings = v
        .parent_bindings
        .as_ref()
        .filter(|_| v.parent_pending)
        .map(|bs| {
            bs.iter()
                .map(|b| ParentBinding {
                    name: b.name.clone(),
                    decl: decl_view(b),
                })
                .collect()
        });
    Ok(ContextView {
        callees: v.callees.clone(),
        scope_chain: chain,
        program_bindings: program.names.clone(),
        program_globals: program.globals.clone(),
        parent_bindings,
    })
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn context_matches(ctx: &LlmContext, out: &CapturedContextOut) -> bool {
    let callees_match = ctx.callee_signatures.len() == out.callee_signatures.len()
        && ctx
            .callee_signatures
            .iter()
            .zip(&out.callee_signatures)
            .all(|(a, b)| {
                a.name == b.name && a.params == b.params && a.snippet.as_deref() == Some(&b.snippet)
            });
    callees_match
        && ctx.callsites == out.callsites
        && ctx.used_identifiers.len() == out.used_identifiers_count
        && sha256_hex(&ctx.used_identifiers.join("\n")) == out.used_identifiers_sha
        && ctx.context_vars == out.context_vars
}

fn gate_context(rows_dir: &Path, report: &mut CaptureReport) -> Result<(), String> {
    let scopes: HashMap<u64, ScopeSnapshot> =
        read_jsonl::<ScopeSnapshot>(&rows_dir.join("scopes.jsonl"))?
            .into_iter()
            .map(|s| (s.id, s))
            .collect();
    for row in read_jsonl::<ContextRow>(&rows_dir.join("context.jsonl"))? {
        let view = context_view(&row.view, &scopes)?;
        let eligible: Vec<(&str, bool)> = row
            .view
            .parent_bindings
            .iter()
            .flatten()
            .map(|b| (b.name.as_str(), b.eligible))
            .collect();
        let is_eligible = |name: &str| {
            eligible
                .iter()
                .find(|(n, _)| *n == name)
                .is_some_and(|(_, e)| *e)
        };
        let ctx = build_context(&view, &row.out.callsites, is_eligible);
        if !ctx.callee_signatures.is_empty() {
            report.bump("context-with-callees");
        }
        if view.parent_bindings.is_some() {
            report.bump("context-parent-pending");
        }
        if ctx.context_vars.is_some() {
            report.bump("context-with-context-vars");
        }
        let (seq, sid) = (row.seq, row.session_id.clone());
        report.context.check(
            || format!("call {seq} ({sid})"),
            context_matches(&ctx, &row.out),
        );
    }
    Ok(())
}

fn gate_capture(rows_dir: &Path, dump_keys: &[String]) -> Result<CaptureReport, String> {
    let requests: Vec<RequestRow> = read_jsonl(&rows_dir.join("requests.jsonl"))?;
    let mut report = CaptureReport {
        key_sequence_matches: requests.len() == dump_keys.len()
            && requests
                .iter()
                .zip(dump_keys)
                .all(|(r, k)| r.cache_key == *k),
        ..Default::default()
    };
    gate_module_dispatches(&requests, &mut report);
    gate_module_builder_calls(
        &rows_dir.join("module-builders.jsonl"),
        &mut report.module_builder_calls,
    )?;
    gate_code_window(&rows_dir.join("code-window.jsonl"), &mut report)?;
    gate_context(rows_dir, &mut report)?;
    Ok(report)
}

/// Run the gate over one pair: `dump` is the oracle's dump dir; `capture`
/// the capture hook's rows dir (optional — without it only the dump
/// section runs).
pub fn run(dump: &Path, capture: Option<&Path>) -> Result<PromptGateReport, String> {
    let mut report = PromptGateReport::default();
    let dump_keys = gate_dump(dump, &mut report)?;
    if let Some(rows) = capture {
        report.capture = Some(gate_capture(rows, &dump_keys)?);
    }
    Ok(report)
}

#[cfg(test)]
mod prompt_gate_test;
