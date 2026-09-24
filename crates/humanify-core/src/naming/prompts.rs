//! The prompt builders — src/llm/prompts.ts — and the provider's render
//! selection (openai-compatible.ts `buildBatchUserPrompt`, mirrored by the
//! dump's `renderRequestUserPrompt`).
//!
//! Prompts are an external contract with the disk cache (02 §7): a Rust
//! leg replays a TS-populated cache only if every prompt is byte-identical,
//! and the cache key alone cannot prove it (it sorts `usedNames`; the
//! prompt shows the first 50 in SET ORDER — 07 §5). Gated byte-for-byte
//! against every prompt of the four oracle pairs (`humanify prompt-gate`)
//! and against probe vectors of every builder on adversarial inputs
//! (test/parity/wp42-vectors.json).
//!
//! JS semantics reproduced, each one a TS expression:
//! - `used_names` is a Set in insertion order; the prompt shows
//!   `[...usedNames].slice(0, 50)` (200, eligibility-filtered, for
//!   module-level prompts);
//! - truthiness: an EMPTY `priorVersionCode`, `userPrompt`,
//!   `systemPrompt`, hint or suggested name is absent;
//! - `Object.entries(alreadyRenamed)` enumerates index-like keys first —
//!   [`StrMap`] is already in JS enumeration order (built through
//!   `JsObject`);
//! - `record[key]` falls through to `Object.prototype` ([`js_record`]).

use humanify_model::js::JsValue;
use humanify_model::llm::{BatchRenameRequest, RenameFailures, StrMap};

use super::js_record;

/// System prompt for batch renaming all identifiers in a function at once.
pub const BATCH_RENAME_SYSTEM_PROMPT: &str = "You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to analyze a minified function and suggest meaningful, descriptive names for ALL identifiers at once.

CRITICAL RULES:
- You MUST provide a mapping for EVERY identifier listed. Do not skip any.
- All suggested names MUST be unique — no two identifiers can map to the same name.
- Respond with ONLY a JSON object. No explanation, no markdown, just the JSON.

Naming Guidelines:
- First understand what the function DOES semantically
- Name the function based on its PURPOSE (e.g., \"splitStringIntoChunks\" not \"processData\")
- Name variables based on what they REPRESENT (e.g., \"chunkSize\" not \"tVal\")
- Use camelCase for variables and functions
- Use PascalCase for classes/constructors (look for 'this' usage, 'new' calls)
- Start function names with verbs (get, set, fetch, create, handle, process, etc.)
- Name loop counters meaningfully when possible (index, i, j are OK for simple loops)
- Never shadow global built-in names (Date, Math, JSON, Array, Object, Map, Set, Promise, Error, Buffer, console, process, etc.) — this breaks the program at runtime";

/// System prompt for module-level identifier renaming.
pub const MODULE_LEVEL_RENAME_SYSTEM_PROMPT: &str = "You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to analyze top-level module declarations and suggest meaningful, descriptive names for minified identifiers.

Guidelines:
- For imports: use context from the module path (e.g., import { webcrypto as a } → webcrypto is already a good hint)
- For constants: use UPPER_SNAKE_CASE if the value is a true constant (literal number/string), camelCase otherwise
- For variables (let): use camelCase based on how they're used
- Be specific but concise
- Every identifier in the list MUST have a mapping
- All suggested names MUST be unique (no duplicates)
- Never use reserved words (if, for, class, etc.) or global built-in names (Date, Math, JSON, Array, Object, Map, Set, Promise, Error, Buffer, console, process, etc.) — shadowing these breaks the program

Respond with ONLY a JSON object mapping each original name to a descriptive name.";

/// Cap on the used-names list a function prompt shows.
const USED_NAMES_CAP: usize = 50;
/// Cap on per-identifier prior-name hints rendered in a rename prompt.
const MAX_PRIOR_NAME_HINTS: usize = 40;
/// Cap on the used-names list carried by module-level rename prompts.
const MODULE_USED_NAMES_CAP: usize = 200;

/// A `Record<string, string[]>` (module-level assignment / usage context),
/// in JS enumeration order. Read by key only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArrayRecord(pub Vec<(String, Vec<String>)>);

impl ArrayRecord {
    /// `record[key]` for an own key. An inherited `Object.prototype`
    /// member reads as absent here: the TS reads its `.length` (0 for
    /// `toString`/`valueOf`/`toLocaleString`, so skipped — the same as
    /// absent) or CRASHES iterating a function of length ≥ 1
    /// (`hasOwnProperty`, `constructor`, …) — a crash the port does not
    /// reproduce (unreachable on the oracle pairs; no identifier is a
    /// prototype name).
    fn get(&self, key: &str) -> Option<&[String]> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }
}

impl<'de> serde::Deserialize<'de> for ArrayRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let JsValue::Object(obj) = value else {
            return Err(serde::de::Error::custom(
                "expected an object of string arrays",
            ));
        };
        let strings = |v: &JsValue| -> Option<Vec<String>> {
            match v {
                JsValue::Array(items) => items
                    .iter()
                    .map(|s| s.as_str().map(str::to_string))
                    .collect(),
                _ => None,
            }
        };
        obj.entries()
            .iter()
            .map(|(k, v)| {
                strings(v)
                    .map(|list| (k.clone(), list))
                    .ok_or_else(|| serde::de::Error::custom(format!("{k}: expected strings")))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(ArrayRecord)
    }
}

/// The module-level builders' inputs (`buildModuleLevelRenameBody`'s
/// arguments bar the eligibility predicate).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModuleLevelInput {
    pub declarations: Vec<String>,
    pub assignment_context: ArrayRecord,
    pub usage_examples: ArrayRecord,
    pub identifiers: Vec<String>,
    /// A Set in insertion order.
    pub used_names: Vec<String>,
    pub suggested_names: Option<StrMap>,
}

/// The function retry builders' inputs (`buildBatchRenameRetryBody`).
#[derive(Clone, Copy, Debug)]
pub struct RetryInput<'a> {
    pub code: &'a str,
    pub identifiers: &'a [String],
    /// A Set in insertion order.
    pub used_names: &'a [String],
    pub previous_attempt: &'a StrMap,
    pub failures: &'a RenameFailures,
    pub prior_version_code: Option<&'a str>,
    pub already_renamed: Option<&'a StrMap>,
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

fn first_n(names: &[String], n: usize) -> String {
    names
        .iter()
        .take(n)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `{ "a": "descriptiveName", "b": "descriptiveName" }`
fn mapping_template(identifiers: &[String]) -> String {
    let items: Vec<String> = identifiers
        .iter()
        .map(|id| format!("\"{id}\": \"descriptiveName\""))
        .collect();
    format!("{{ {} }}", items.join(", "))
}

/// Builds the user prompt for batch renaming (`buildBatchRenamePrompt`).
pub fn build_batch_rename_prompt(r: &BatchRenameRequest) -> String {
    let ids = &r.identifiers;
    let mut p = String::from(
        "Analyze this function and suggest descriptive names for ALL listed identifiers:\n\n",
    );
    p += &format!("```javascript\n{}\n```\n\n", r.code);
    p += &format!("Identifiers to rename: {}\n\n", ids.join(", "));
    if let Some(vars) = r.context_vars.as_ref().filter(|v| !v.is_empty()) {
        p += "Surrounding scope variables (for context only, do NOT rename these):\n";
        for v in vars {
            p += &format!("  {v}\n");
        }
        p += "\n";
    }
    if !r.callee_signatures.is_empty() {
        p += "This function calls:\n";
        for c in &r.callee_signatures {
            p += &format!("- {}({})\n", c.name, c.params.join(", "));
        }
        p += "\n";
    }
    if !r.callsites.is_empty() {
        p += "This function is called as:\n";
        for site in r.callsites.iter().take(3) {
            p += &format!("- {site}\n");
        }
        p += "\n";
    }
    p += &render_prior_version_block(
        r.prior_version_code.as_deref(),
        r.prior_version_names.as_deref(),
        r.prior_name_hints.as_ref(),
    );
    p += &render_prior_name_hints(ids, r.prior_name_hints.as_ref());
    p += &render_already_renamed(r.already_renamed.as_ref());
    if !r.used_names.is_empty() {
        p += &format!(
            "Names already in use (MUST avoid these): {}\n\n",
            first_n(&r.used_names, USED_NAMES_CAP)
        );
    }
    p += &format!(
        "You MUST respond with a JSON object containing exactly {} mappings — one for each identifier listed above:\n",
        ids.len()
    );
    p + &mapping_template(ids)
}

/// The prior-version reuse block, or "" without prior code. The flat name
/// bag omits every name some per-id hint already carries
/// (`Object.values(priorNameHints)` — own values only).
fn render_prior_version_block(
    code: Option<&str>,
    names: Option<&[String]>,
    hints: Option<&StrMap>,
) -> String {
    let Some(code) = non_empty(code) else {
        return String::new();
    };
    let mut s = String::from("IMPORTANT — A prior version of this function was already named:\n\n");
    s += &format!("```javascript\n{code}\n```\n\n");
    s += "You MUST reuse the names from the prior version unless the function's behavior has changed enough that a name is no longer accurate. ";
    s += "Small structural changes (reordered conditions, added error handling, extra parameters) do NOT justify renaming — keep the prior names. ";
    s += "Only choose a different name when the identifier's purpose has fundamentally changed. ";
    s += "If a prior name conflicts with an already-used name listed below, choose a close variant (e.g., \"handleError\" → \"handleComponentError\") rather than an unrelated name.\n\n";
    let hinted = |n: &str| hints.is_some_and(|h| h.0.iter().any(|(_, v)| v == n));
    let flat: Vec<&str> = names
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .filter(|n| !hinted(n))
        .collect();
    if !flat.is_empty() {
        s += &format!(
            "Reuse these names from the prior version for unchanged logic: {}\n\n",
            flat.join(", ")
        );
    }
    s
}

/// The per-identifier prior-name hint block, or "" when empty: each
/// requested identifier whose hint is truthy and differs from it, first 40.
fn render_prior_name_hints(identifiers: &[String], hints: Option<&StrMap>) -> String {
    let Some(hints) = hints else {
        return String::new();
    };
    let mut pairs = Vec::new();
    for id in identifiers {
        if let Some(prior) = js_record::get_truthy(hints, id).filter(|p| p != id) {
            pairs.push((id, prior));
        }
        if pairs.len() >= MAX_PRIOR_NAME_HINTS {
            break;
        }
    }
    if pairs.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "These identifiers were named as follows in the prior version. Reuse the exact name unless the identifier's role changed:\n",
    );
    for (id, prior) in pairs {
        s += &format!("  {id} → {prior}\n");
    }
    s + "\n"
}

/// The already-renamed section, or "" when absent or empty.
fn render_already_renamed(already: Option<&StrMap>) -> String {
    let Some(map) = already.filter(|m| !m.0.is_empty()) else {
        return String::new();
    };
    let mut s = String::from(
        "These identifiers in the same scope were already renamed (do NOT rename them again, keep your suggestions consistent with them):\n",
    );
    for (old, new) in &map.0 {
        s += &format!("  {old} → {new}\n");
    }
    s + "\n"
}

/// Shared response-format tail for retry prompts
/// (`buildRenameResponseInstruction`).
pub fn build_rename_response_instruction(identifiers: &[String]) -> String {
    format!(
        "Respond with JSON mapping each identifier to a UNIQUE name:\n{}",
        mapping_template(identifiers)
    )
}

/// The rejected-name block both retry renderers share: every truthy
/// suggestion for a duplicate/unchanged/invalid name, deduplicated in
/// first-seen order (a Set).
fn render_rejected_names(prev: &StrMap, f: &RenameFailures) -> String {
    let mut rejected: Vec<String> = Vec::new();
    for name in f.duplicates.iter().chain(&f.unchanged).chain(&f.invalid) {
        if let Some(s) = js_record::get_truthy(prev, name)
            && !rejected.iter().any(|r| *r == *s)
        {
            rejected.push(s.into_owned());
        }
    }
    if rejected.is_empty() {
        return String::new();
    }
    format!("\nDO NOT suggest these names: {}\n", rejected.join(", "))
}

fn missing_line(f: &RenameFailures) -> String {
    if f.missing.is_empty() {
        return String::new();
    }
    format!(
        "- These identifiers were MISSING from your response: {}\n",
        f.missing.join(", ")
    )
}

/// Failure diagnostics + rejected-name blocklist for function retries.
fn render_retry_diagnostics(prev: &StrMap, f: &RenameFailures) -> String {
    let mut s = String::from("Your previous rename suggestions had issues:\n");
    for name in &f.duplicates {
        s += &match js_record::get_truthy(prev, name) {
            Some(sug) => format!(
                "- \"{name}\" was suggested as \"{sug}\" but that conflicts with an existing name\n"
            ),
            None => format!("- \"{name}\" had a duplicate/conflicting name\n"),
        };
    }
    for name in &f.unchanged {
        s += &format!("- \"{name}\" was returned as itself — you MUST suggest a DIFFERENT name\n");
    }
    for name in &f.invalid {
        s += &match js_record::get_truthy(prev, name) {
            Some(sug) => format!(
                "- \"{name}\" was suggested as \"{sug}\" which is not allowed (reserved word, global built-in, or invalid syntax)\n"
            ),
            None => format!("- \"{name}\" had an invalid suggested name\n"),
        };
    }
    s += &missing_line(f);
    s + &render_rejected_names(prev, f)
}

/// The retry prompt without its response-format tail
/// (`buildBatchRenameRetryBody`) — the request's `promptBody`, which is
/// cache-key material.
pub fn build_batch_rename_retry_body(i: &RetryInput<'_>) -> String {
    let mut p = render_retry_diagnostics(i.previous_attempt, i.failures);
    p += &format!("\n{}", render_already_renamed(i.already_renamed));
    p += "\nPlease suggest DIFFERENT names for these remaining identifiers:\n\n";
    p += &format!("```javascript\n{}\n```\n\n", i.code);
    p += &format!(
        "Identifiers still needing names: {}\n\n",
        i.identifiers.join(", ")
    );
    if let Some(prior) = non_empty(i.prior_version_code) {
        p += "IMPORTANT — The prior version of this function used these names:\n\n";
        p += &format!("```javascript\n{prior}\n```\n\n");
        p += "You MUST reuse names from the prior version where the identifier's purpose is the same. Only pick a different name if it conflicts with the used-names list below or the identifier's purpose has fundamentally changed.\n\n";
    }
    p + &format!(
        "Names already in use (MUST avoid ALL of these): {}\n",
        first_n(i.used_names, USED_NAMES_CAP)
    )
}

/// The retry prompt (`buildBatchRenameRetryPrompt`).
pub fn build_batch_rename_retry_prompt(i: &RetryInput<'_>) -> String {
    format!(
        "{}\n{}",
        build_batch_rename_retry_body(i),
        build_rename_response_instruction(i.identifiers)
    )
}

/// `declByIdentifier`: for each identifier, the declarations that contain
/// it as a SUBSTRING (`decl.includes(id)`), in declaration order; only the
/// first is ever shown.
fn first_declaration<'a>(declarations: &'a [String], id: &str) -> Option<&'a str> {
    declarations
        .iter()
        .find(|d| d.contains(id))
        .map(String::as_str)
}

fn indented_block(s: &mut String, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    *s += title;
    for item in items {
        let lines: Vec<String> = item.split('\n').map(|l| format!("    {l}")).collect();
        *s += &format!("{}\n", lines.join("\n"));
    }
}

fn identifier_profile(input: &ModuleLevelInput, id: &str) -> String {
    let mut s = format!("Identifier: {id}\n");
    let suggested = input
        .suggested_names
        .as_ref()
        .and_then(|m| js_record::get_truthy(m, id));
    if let Some(name) = suggested {
        s += &format!("  Prior version name: {name}\n");
    }
    if let Some(decl) = first_declaration(&input.declarations, id) {
        s += &format!("  Declaration: {decl}\n");
    }
    indented_block(
        &mut s,
        "  Assignments:\n",
        input.assignment_context.get(id).unwrap_or_default(),
    );
    indented_block(
        &mut s,
        "  Usage:\n",
        input.usage_examples.get(id).unwrap_or_default(),
    );
    s
}

/// The module-level prompt without its response-format tail
/// (`buildModuleLevelRenameBody`). The used-names line lists only
/// NON-eligible names (eligible ones are about to be renamed), first 200.
pub fn build_module_level_rename_body(
    input: &ModuleLevelInput,
    is_eligible: impl Fn(&str) -> bool,
) -> String {
    let mut p = String::from(
        "Analyze these top-level module identifiers and suggest descriptive names.\n\n",
    );
    if input
        .suggested_names
        .as_ref()
        .is_some_and(|m| !m.0.is_empty())
    {
        p += "Where a \"Prior version name\" is shown, strongly prefer that name unless the binding's purpose has fundamentally changed in this version.\n\n";
    }
    for id in &input.identifiers {
        p += &identifier_profile(input, id);
        p += "\n";
    }
    p += &format!(
        "Identifiers to rename: {}\n\n",
        input.identifiers.join(", ")
    );
    let used: Vec<&str> = input
        .used_names
        .iter()
        .map(String::as_str)
        .filter(|n| !is_eligible(n))
        .take(MODULE_USED_NAMES_CAP)
        .collect();
    if !used.is_empty() {
        p += &format!(
            "Names already in use (MUST avoid these): {}\n\n",
            used.join(", ")
        );
    }
    p
}

/// The module-level prompt (`buildModuleLevelRenamePrompt`).
pub fn build_module_level_rename_prompt(
    input: &ModuleLevelInput,
    is_eligible: impl Fn(&str) -> bool,
) -> String {
    format!(
        "{}Respond with JSON mapping EVERY identifier to a new name:\n{}",
        build_module_level_rename_body(input, is_eligible),
        mapping_template(&input.identifiers)
    )
}

/// The retry prefix prepended to a module-level prompt
/// (`buildModuleLevelRetryPrefix`). Unlike the function retry, a
/// duplicate/invalid name without a truthy suggestion gets NO line.
pub fn build_module_level_retry_prefix(prev: &StrMap, f: &RenameFailures) -> String {
    let mut s = String::from("Your previous rename suggestions had issues:\n");
    for name in &f.duplicates {
        if let Some(sug) = js_record::get_truthy(prev, name) {
            s += &format!(
                "- \"{name}\" was suggested as \"{sug}\" but that conflicts with an existing name\n"
            );
        }
    }
    for name in &f.unchanged {
        s += &format!("- \"{name}\" was returned as itself — you MUST suggest a DIFFERENT name\n");
    }
    for name in &f.invalid {
        if let Some(sug) = js_record::get_truthy(prev, name) {
            s += &format!(
                "- \"{name}\" was suggested as \"{sug}\" which is not allowed (reserved word, global built-in, or invalid syntax)\n"
            );
        }
    }
    s += &missing_line(f);
    s += &render_rejected_names(prev, f);
    s + "\nPlease suggest DIFFERENT names for the remaining identifiers below:\n"
}

/// The user prompt the provider sends for `r`: a truthy `userPrompt`
/// verbatim, else the retry prompt when `isRetry && failures`
/// (`previousAttempt || {}`), else the first-round prompt.
pub fn render_user_prompt(r: &BatchRenameRequest) -> String {
    if let Some(user) = non_empty(r.user_prompt.as_deref()) {
        return user.to_string();
    }
    if let (Some(true), Some(failures)) = (r.is_retry, r.failures.as_ref()) {
        let empty = StrMap::default();
        return build_batch_rename_retry_prompt(&RetryInput {
            code: &r.code,
            identifiers: &r.identifiers,
            used_names: &r.used_names,
            previous_attempt: r.previous_attempt.as_ref().unwrap_or(&empty),
            failures,
            prior_version_code: r.prior_version_code.as_deref(),
            already_renamed: r.already_renamed.as_ref(),
        });
    }
    build_batch_rename_prompt(r)
}

/// The system prompt the provider sends: a truthy override, else the
/// batch default.
pub fn render_system_prompt(r: &BatchRenameRequest) -> String {
    non_empty(r.system_prompt.as_deref())
        .unwrap_or(BATCH_RENAME_SYSTEM_PROMPT)
        .to_string()
}

#[cfg(test)]
mod prompts_test;
