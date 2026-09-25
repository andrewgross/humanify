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

/// `createSplitNamer` over a [`NameProvider`]. Every dispatched call is
/// kept (`dispatched`) so a gate can compare its bytes with the oracle's.
pub struct ProviderSplitNamer<'p> {
    provider: &'p dyn NameProvider,
    pub dispatched: Vec<LlmCall>,
    /// Batches whose provider call failed (all entries fell back).
    pub failed_batches: usize,
}

impl<'p> ProviderSplitNamer<'p> {
    pub fn new(provider: &'p dyn NameProvider) -> Self {
        ProviderSplitNamer {
            provider,
            dispatched: Vec::new(),
            failed_batches: 0,
        }
    }
}

impl SplitNamer for ProviderSplitNamer<'_> {
    fn name(&mut self, requests: &[SplitNameRequest]) -> Vec<Option<String>> {
        if requests.is_empty() {
            return Vec::new();
        }
        let (call, keys) = split_namer_call(requests);
        self.dispatched.push(call.clone());
        match self.provider.run_wave(vec![call]).pop() {
            Some(Ok(response)) => requests
                .iter()
                .zip(&keys)
                .map(|(request, key)| {
                    // `!proposed || proposed === mechanicalStem || === key`
                    response
                        .renames
                        .get(key)
                        .filter(|p| !p.is_empty() && *p != request.mechanical_stem && p != key)
                        .map(str::to_string)
                })
                .collect(),
            _ => {
                self.failed_batches += 1;
                requests.iter().map(|_| None).collect()
            }
        }
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
mod namer_test;
