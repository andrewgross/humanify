//! The LLM namer for NEW split files/folders — TS `src/split/split-namer.ts`
//! (`createSplitNamer`, `createTreeReviser`) plus the request types
//! `SplitNameRequest` / `FolderSummary` from stable-split.ts.
//!
//! Naming-only by construction: the namer sees per-entry summaries and
//! returns basenames; it never sees or moves code. A whole sibling scope is
//! ONE provider call. Best-effort: a decline, an echo of the stem, or a
//! provider failure (a replay-only cache MISS included) all resolve to
//! `None` — keep the mechanical stem. Validation of what comes back is the
//! caller's job ([`crate::place::stems::accept_proposed_name`]).
//!
//! The prompt bytes and the request (hence the cache key) are the TS's
//! exactly: gated against the oracle dump's `prompts.jsonl` rows
//! (`functionId: "split-namer"`) by the placement verb.

use std::collections::HashSet;

use humanify_model::llm::{BatchRenameRequest, LlmCall, NameProvider};

use crate::modules::vendor_names::unique_case_insensitive_name;

/// `SplitNameRequest.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    File,
    Folder,
}

impl NameKind {
    fn as_str(self) -> &'static str {
        match self {
            NameKind::File => "file",
            NameKind::Folder => "folder",
        }
    }
}

/// `SplitNameRequest.level`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameLevel {
    Top,
    Sub,
}

/// `SplitNameRequest`.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitNameRequest {
    pub kind: NameKind,
    pub mechanical_stem: String,
    pub siblings: Vec<String>,
    pub bindings: Vec<String>,
    pub members: Option<Vec<String>>,
    pub level: Option<NameLevel>,
    pub evidence: Option<String>,
}

/// `FolderSummary` — a top-level folder and its member files.
#[derive(Clone, Debug, PartialEq)]
pub struct FolderSummary {
    pub name: String,
    pub members: Vec<String>,
}

/// `SplitNamer`: one proposal (or `None`) per request, in request order.
pub trait SplitNamer {
    fn name(&mut self, requests: &[SplitNameRequest]) -> Vec<Option<String>>;
}

/// `TreeReviser`: a partial old-name → new-name map, in the TS's
/// enumeration order.
pub trait TreeReviser {
    fn revise(&mut self, folders: &[FolderSummary]) -> Vec<(String, String)>;
}

pub const SPLIT_NAMER_SYSTEM_PROMPT: &str = "You name source files and folders in a decompiled JavaScript CLI tool, \
the way an experienced engineer would organize a real repository.\n\
Name the CONCEPT — what the code is about — from the evidence (the \
strings it uses, the APIs it calls, its declarations). Do NOT just echo \
the loudest function name.\n\
Rules:\n\
- Use a NOUN or noun phrase, 1-3 words. Good: retry-scheduler, \
hostname-resolver, token-bucket, message-queue, diff-view, auth-flow.\n\
- A FOLDER is a domain bucket: a plain noun (auth, transcript, tools, \
permissions). Never a verb phrase (get-display-name), never a \
conjunction (foo-and-bar — that means it should be two folders), never \
a decoration suffix (Manager, Suite, Engine, Group, Handler).\n\
- Never start a name with a conjunction, article, or preposition \
(and, or, the, a, with, for). Never a bare verb.\n\
- Avoid generic names (utils, helpers, core, common, misc, index) and \
numeric suffixes (initializer17).\n\
- Siblings must be DISTINCT; a folder name must not repeat one member.\n\
Return a single kebab-case basename per entry, no extension, no path.";

const REVISER_SYSTEM_PROMPT: &str = "You are reviewing the top-level folders of a decompiled JavaScript CLI \
repository, now that every folder's files are named. Make the set read \
like a human's src/: each folder a short domain noun (1-2 words), all \
DISTINCT, no near-synonyms, no outliers. Only propose a change when it \
is a real improvement. Same rules as before: kebab-case nouns, never a \
verb phrase, conjunction, or Manager/Suite/Engine decoration.";

/// `renderEntry`: one entry's brief within the batch prompt.
fn render_entry(key: &str, request: &SplitNameRequest) -> Vec<String> {
    let kind = request.kind.as_str();
    let mut lines = vec![format!("### {key} ({kind})")];
    if let Some(evidence) = request.evidence.as_deref().filter(|e| !e.is_empty()) {
        lines.push(format!("What it does (from its code): {evidence}"));
    }
    lines.push("Most-referenced declarations:".to_string());
    lines.extend(request.bindings.iter().map(|b| format!("  - {b}")));
    if let Some(members) = request.members.as_ref().filter(|m| !m.is_empty()) {
        lines.push(format!("Files it contains: {}", members.join(", ")));
        lines.push("Name the whole group, not one member.".to_string());
    }
    if request.level == Some(NameLevel::Top) {
        lines.push(
            "This is a TOP-LEVEL source folder: prefer a short plain domain \
noun (like auth, permissions, transcript, tools) — no decorated \
suffixes such as Suite, Engine, Hub, or Manager."
                .to_string(),
        );
    }
    if !request.siblings.is_empty() {
        lines.push(format!(
            "Sibling {kind}s (pick a DISTINCT name): {}",
            request.siblings.join(", ")
        ));
    }
    lines
}

/// `buildPrompt`.
fn build_prompt(requests: &[SplitNameRequest], keys: &[String]) -> String {
    let mut lines = vec![
        format!(
            "Name {} entries in a decompiled CLI tool repository.",
            requests.len()
        ),
        String::new(),
    ];
    for (request, key) in requests.iter().zip(keys) {
        lines.extend(render_entry(key, request));
        lines.push(String::new());
    }
    let reply: Vec<String> = keys
        .iter()
        .map(|k| format!("\"{k}\": \"<name>\""))
        .collect();
    lines.push(format!(
        "Reply with JSON {{{}}} — one specific name per entry.",
        reply.join(", ")
    ));
    lines.join("\n")
}

/// A JS `new Set(items)` as an insertion-ordered Vec.
fn dedup_in_order(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

/// The provider call a batch makes, plus the per-entry prompt keys
/// (duplicate stems uniquified case-insensitively, so every brief maps to
/// exactly one answer).
pub fn split_namer_call(requests: &[SplitNameRequest]) -> (LlmCall, Vec<String>) {
    let mut used = HashSet::new();
    let keys: Vec<String> = requests
        .iter()
        .map(|r| unique_case_insensitive_name(&r.mechanical_stem, &mut used, ""))
        .collect();
    let prompt = build_prompt(requests, &keys);
    let request = BatchRenameRequest {
        code: prompt.clone(),
        identifiers: keys.clone(),
        used_names: dedup_in_order(requests.iter().flat_map(|r| r.siblings.clone())),
        system_prompt: Some(SPLIT_NAMER_SYSTEM_PROMPT.to_string()),
        user_prompt: Some(prompt.clone()),
        ..BatchRenameRequest::default()
    };
    let call = LlmCall {
        request,
        system_prompt: SPLIT_NAMER_SYSTEM_PROMPT.to_string(),
        user_prompt: prompt,
    };
    (call, keys)
}

/// `buildReviserPrompt`.
fn build_reviser_prompt(folders: &[FolderSummary]) -> String {
    let mut lines = vec![
        format!(
            "Review these {} top-level folders of a decompiled CLI repo.",
            folders.len()
        ),
        String::new(),
    ];
    for f in folders {
        lines.push(format!("- {}/  (files: {})", f.name, f.members.join(", ")));
    }
    lines.push(String::new());
    lines.push(
        "Reply with JSON mapping ONLY the folders you would rename to their \
better name, e.g. {\"oldName\": \"betterName\"}. Omit folders that are \
already good."
            .to_string(),
    );
    lines.join("\n")
}

/// The provider call a revision makes.
pub fn tree_reviser_call(folders: &[FolderSummary]) -> LlmCall {
    let prompt = build_reviser_prompt(folders);
    LlmCall {
        request: BatchRenameRequest {
            code: prompt.clone(),
            identifiers: folders.iter().map(|f| f.name.clone()).collect(),
            system_prompt: Some(REVISER_SYSTEM_PROMPT.to_string()),
            user_prompt: Some(prompt.clone()),
            ..BatchRenameRequest::default()
        },
        system_prompt: REVISER_SYSTEM_PROMPT.to_string(),
        user_prompt: prompt,
    }
}

/// The model context assumed when none is configured (`--context-tokens`):
/// gpt-oss-20b's, the measurement default.
pub const DEFAULT_CONTEXT_TOKENS: u64 = 32_768;

/// Conservative prompt-bytes per token. Finding #39's refused prompts ran
/// ~3.9 bytes/token (759K chars → 195,758 tokens); 3 over-counts tokens.
const BYTES_PER_TOKEN: usize = 3;
/// Share of the computed prompt room actually used (chat-template overhead,
/// tokenizer variance).
const HEADROOM_PERCENT: usize = 75;
/// Completion tokens reserved per entry (the `"key": "name"` answer plus
/// its share of the model's reasoning).
const COMPLETION_TOKENS_PER_ENTRY: u64 = 60;

/// How big one split-namer prompt may get (finding #39): the namer used to
/// send every request in ONE prompt, and a fossil hop's thousands of fresh
/// mints made that prompt 759K-1.2M chars — refused by the model every run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitNamerBudget {
    /// Upper bound on a user prompt's length in bytes.
    pub max_prompt_chars: usize,
    /// Upper bound on entries per call (the completion must answer them).
    pub max_entries: usize,
}

impl SplitNamerBudget {
    /// The budget for a model with `context_tokens` of context, of which
    /// `completion_tokens` (`max_tokens`) are reserved for the answer.
    pub fn for_model(context_tokens: u64, completion_tokens: u64) -> Self {
        let system_tokens = SPLIT_NAMER_SYSTEM_PROMPT.len().div_ceil(BYTES_PER_TOKEN) as u64;
        let prompt_tokens = context_tokens
            .saturating_sub(completion_tokens)
            .saturating_sub(system_tokens);
        let max_prompt_chars = usize::try_from(prompt_tokens)
            .unwrap_or(usize::MAX / BYTES_PER_TOKEN)
            .saturating_mul(HEADROOM_PERCENT)
            / 100
            * BYTES_PER_TOKEN;
        let max_entries = usize::try_from(completion_tokens / COMPLETION_TOKENS_PER_ENTRY)
            .unwrap_or(usize::MAX)
            .max(1);
        SplitNamerBudget {
            max_prompt_chars,
            max_entries,
        }
    }
}

impl Default for SplitNamerBudget {
    fn default() -> Self {
        SplitNamerBudget::for_model(
            DEFAULT_CONTEXT_TOKENS,
            humanify_model::llm::DEFAULT_MAX_TOKENS,
        )
    }
}

/// The prompt's fixed frame (the `Name N entries…` header and the reply
/// line's text), over-counted.
const PROMPT_FRAME_BYTES: usize = 160;
/// A uniquified key's suffix (`-NN`), counted once in the brief header and
/// once in the reply template, over-counted.
const KEY_SUFFIX_SLACK: usize = 8;

/// One entry's bytes in a batch prompt: its brief (with its blank line)
/// plus its `"key": "<name>", ` reply fragment.
fn entry_bytes(request: &SplitNameRequest) -> usize {
    let key = &request.mechanical_stem;
    let brief: usize = render_entry(key, request).iter().map(|l| l.len() + 1).sum();
    brief + 1 + key.len() + "\"\": \"<name>\", ".len() + 2 * KEY_SUFFIX_SLACK
}

/// The batch boundaries for `requests` under `budget`: contiguous ranges in
/// request order, greedily filled, each under both caps. A pure function of
/// the requests, so the same input always yields the same prompts (cache
/// hits replay). An entry too large on its own is still sent, alone.
pub fn split_namer_batches(
    requests: &[SplitNameRequest],
    budget: &SplitNamerBudget,
) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut bytes = PROMPT_FRAME_BYTES;
    for (i, request) in requests.iter().enumerate() {
        let cost = entry_bytes(request);
        let filled = i - start;
        if filled > 0
            && (filled >= budget.max_entries
                || bytes.saturating_add(cost) > budget.max_prompt_chars)
        {
            out.push(start..i);
            start = i;
            bytes = PROMPT_FRAME_BYTES;
        }
        bytes = bytes.saturating_add(cost);
    }
    if start < requests.len() {
        out.push(start..requests.len());
    }
    out
}

/// `createSplitNamer` over a [`NameProvider`]. Every dispatched call is
/// kept (`dispatched`) so a gate can compare its bytes with the oracle's.
pub struct ProviderSplitNamer<'p> {
    provider: &'p dyn NameProvider,
    budget: SplitNamerBudget,
    pub dispatched: Vec<LlmCall>,
    /// Batches whose provider call failed (all their entries fell back).
    pub failed_batches: usize,
    /// Entries the provider answered with a usable proposal.
    pub proposals: usize,
}

impl<'p> ProviderSplitNamer<'p> {
    pub fn new(provider: &'p dyn NameProvider) -> Self {
        ProviderSplitNamer::with_budget(provider, SplitNamerBudget::default())
    }

    pub fn with_budget(provider: &'p dyn NameProvider, budget: SplitNamerBudget) -> Self {
        ProviderSplitNamer {
            provider,
            budget,
            dispatched: Vec::new(),
            failed_batches: 0,
            proposals: 0,
        }
    }
}

/// One batch's answers mapped onto its requests.
fn batch_proposals(
    requests: &[SplitNameRequest],
    keys: &[String],
    response: &humanify_model::llm::BatchRenameResponse,
) -> Vec<Option<String>> {
    requests
        .iter()
        .zip(keys)
        .map(|(request, key)| {
            // `!proposed || proposed === mechanicalStem || === key`
            response
                .renames
                .get(key)
                .filter(|p| !p.is_empty() && *p != request.mechanical_stem && p != key)
                .map(str::to_string)
        })
        .collect()
}

impl SplitNamer for ProviderSplitNamer<'_> {
    /// Batches the requests by [`split_namer_batches`] and dispatches every
    /// batch in ONE wave (the provider's rate limiter bounds concurrency).
    /// A failed batch falls back to the stems alone.
    fn name(&mut self, requests: &[SplitNameRequest]) -> Vec<Option<String>> {
        let batches = split_namer_batches(requests, &self.budget);
        let (calls, keys): (Vec<LlmCall>, Vec<Vec<String>>) = batches
            .iter()
            .map(|r| split_namer_call(&requests[r.clone()]))
            .unzip();
        if calls.is_empty() {
            return Vec::new();
        }
        self.dispatched.extend(calls.iter().cloned());
        let mut results = self.provider.run_wave(calls).into_iter();
        let mut out = Vec::with_capacity(requests.len());
        for (range, keys) in batches.into_iter().zip(&keys) {
            let batch = &requests[range];
            match results.next() {
                Some(Ok(response)) => out.extend(batch_proposals(batch, keys, &response)),
                _ => {
                    self.failed_batches += 1;
                    out.extend(batch.iter().map(|_| None));
                }
            }
        }
        self.proposals += out.iter().flatten().count();
        out
    }
}

/// `createTreeReviser` over a [`NameProvider`].
pub struct ProviderTreeReviser<'p> {
    provider: &'p dyn NameProvider,
    pub dispatched: Vec<LlmCall>,
}

impl<'p> ProviderTreeReviser<'p> {
    pub fn new(provider: &'p dyn NameProvider) -> Self {
        ProviderTreeReviser {
            provider,
            dispatched: Vec::new(),
        }
    }
}

impl TreeReviser for ProviderTreeReviser<'_> {
    fn revise(&mut self, folders: &[FolderSummary]) -> Vec<(String, String)> {
        if folders.is_empty() {
            return Vec::new();
        }
        let call = tree_reviser_call(folders);
        self.dispatched.push(call.clone());
        match self.provider.run_wave(vec![call]).pop() {
            Some(Ok(response)) => folders
                .iter()
                .filter_map(|f| {
                    response
                        .renames
                        .get(&f.name)
                        .filter(|p| !p.is_empty() && *p != f.name)
                        .map(|p| (f.name.clone(), p.to_string()))
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod namer_stub_test;
#[cfg(test)]
mod namer_test;
