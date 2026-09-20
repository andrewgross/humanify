//! The comparison engine (docs/rust-port/07-differential-validation.md §4).
//!
//! `humanify-parity compare <a> <b>` diffs two artifact dumps section by
//! section; every gate is exact set or byte equality — the engine has no
//! epsilon parameter by design (07 §9: an inexact gate is how decision
//! drift gets in). Exit codes: 0 identical, 1 divergences, 2 not comparable
//! (schema/anchor/commit mismatch — spans into different texts are not keys).
//!
//! Sections:
//! - keyed rows (`functions`, `matches`, `transfers`, `votes`, `names`,
//!   `placement`, `emit`): join by primary key, compare whole values;
//! - `partitions`: hash BYTES differ between implementations by design
//!   (02 §4a), so equality is over the equivalence classes: member ->
//!   representative (the smallest member sharing its hash), compared exactly;
//! - `prompts`: rows joined by `(functionId, round)`, `systemPrompt` and
//!   `userPrompt` compared byte-exact, first differing byte reported with
//!   context. (The join key is functionId rather than the primary span:
//!   the sessionIds are deterministic for a fixed input, and the folder/
//!   sweep/vendor sites have no span at all — recorded as a WP0.3 decision.)
//! - whole-value sections (`tree-manifest`, `regions`): exact equality.

use std::collections::BTreeMap;
use std::path::Path;

use humanify_model::dump::*;
use serde_json::Value;

/// One reported divergence.
#[derive(Clone, Debug)]
pub struct Divergence {
    pub section: String,
    pub kind: &'static str, // missing-left | missing-right | mismatch
    pub key: String,
    pub left: Option<String>,
    pub right: Option<String>,
}

/// The result of one compare run.
#[derive(Clone, Debug, Default)]
pub struct CompareOutcome {
    /// False = not comparable (anchor/commit/schema mismatch) — exit 2.
    pub comparable: bool,
    pub not_comparable_reason: Option<String>,
    pub divergences: Vec<Divergence>,
}

impl CompareOutcome {
    /// The process exit code the CLI maps this outcome to.
    pub fn exit_code(&self) -> i32 {
        if !self.comparable {
            2
        } else if self.divergences.is_empty() {
            0
        } else {
            1
        }
    }
}

fn read_json<T: serde::de::DeserializeOwned>(dir: &Path, file: &str) -> Option<T> {
    let text = std::fs::read_to_string(dir.join(file)).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_json_strict<T: serde::de::DeserializeOwned>(
    dir: &Path,
    file: &str,
    why: &mut Option<String>,
) -> Option<T> {
    match std::fs::read_to_string(dir.join(file)) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                *why = Some(format!("{file} failed to parse: {e}"));
                None
            }
        },
        Err(e) => {
            *why = Some(format!("{file} missing: {e}"));
            None
        }
    }
}

/// Sections keyed by rows, in canonical order; the row-keyed ones first
/// (the 07 §2 keyed-row list), then the partition/prompt/whole-value ones.
pub const KEYED_SECTIONS: [&str; 7] = [
    "functions",
    "matches",
    "transfers",
    "votes",
    "names",
    "placement",
    "emit",
];
pub const OTHER_SECTIONS: [&str; 4] = ["partitions", "prompts", "tree-manifest", "regions"];
pub const ALL_SECTIONS: [&str; 15] = [
    "functions",
    "partitions",
    "twins",
    "modules",
    "matches",
    "matches.close",
    "transfers",
    "votes",
    "names",
    "placement",
    "emit",
    "prompts",
    "cache-keys",
    "tree-manifest",
    "regions",
];

/// Parse a `--sections a,b` value into canonical order; empty = all.
pub fn parse_sections(spec: &str) -> Vec<String> {
    if spec.is_empty() {
        return ALL_SECTIONS.iter().map(|s| s.to_string()).collect();
    }
    let wanted: Vec<String> = spec.split(',').map(|s| s.trim().to_string()).collect();
    ALL_SECTIONS
        .iter()
        .filter(|s| wanted.iter().any(|w| w == *s))
        .map(|s| s.to_string())
        .collect()
}

/// The anchor/commit/schema pre-check (07 §4): a mismatch is exit 2, not a
/// divergence — spans into different texts are not keys.
fn anchors_comparable(left: &MetaFile, right: &MetaFile) -> Option<String> {
    if left.schema_version != right.schema_version {
        return Some(format!(
            "schemaVersion mismatch: {} vs {}",
            left.schema_version, right.schema_version
        ));
    }
    if left.commit != right.commit {
        return Some(format!(
            "commit mismatch: {} vs {} (dumps from different commits never compare — 07 §10)",
            left.commit, right.commit
        ));
    }
    for (name, l, r) in [
        ("fresh", &left.texts.fresh, &right.texts.fresh),
        ("prior", &left.texts.prior, &right.texts.prior),
        ("minified", &left.texts.minified, &right.texts.minified),
    ] {
        if l != r {
            return Some(format!("text anchor '{name}' differs between dumps"));
        }
    }
    None
}

/// Generic keyed-row comparison: join by key, compare whole values, report
/// in key order. Duplicate keys within one side are a divergence (a row
/// identity that is not an identity).
fn compare_keyed<K: Ord + Clone, V: PartialEq + Clone>(
    left_rows: &[(K, V)],
    right_rows: &[(K, V)],
    key_display: impl Fn(&K) -> String,
    left_display: impl Fn(&V) -> String,
    right_display: impl Fn(&V) -> String,
    section: &str,
    out: &mut Vec<Divergence>,
) {
    let mut lmap: BTreeMap<K, V> = BTreeMap::new();
    for (k, v) in left_rows {
        if lmap.insert(k.clone(), v.clone()).is_some() {
            out.push(Divergence {
                section: section.to_string(),
                kind: "mismatch",
                key: format!("duplicate-key:{}", key_display(k)),
                left: Some(left_display(v)),
                right: None,
            });
        }
    }
    let mut rmap: BTreeMap<K, V> = BTreeMap::new();
    for (k, v) in right_rows {
        if rmap.insert(k.clone(), v.clone()).is_some() {
            out.push(Divergence {
                section: section.to_string(),
                kind: "mismatch",
                key: format!("duplicate-key:{}", key_display(k)),
                left: None,
                right: Some(right_display(v)),
            });
        }
    }
    for (k, lv) in &lmap {
        match rmap.get(k) {
            None => out.push(Divergence {
                section: section.to_string(),
                kind: "missing-right",
                key: key_display(k),
                left: Some(left_display(lv)),
                right: None,
            }),
            Some(rv) if rv != lv => out.push(Divergence {
                section: section.to_string(),
                kind: "mismatch",
                key: key_display(k),
                left: Some(left_display(lv)),
                right: Some(right_display(rv)),
            }),
            _ => {}
        }
    }
    for k in rmap.keys() {
        if !lmap.contains_key(k) {
            let rv = &rmap[k];
            out.push(Divergence {
                section: section.to_string(),
                kind: "missing-left",
                key: key_display(k),
                left: None,
                right: Some(right_display(rv)),
            });
        }
    }
}

/// Partition comparison (07 §4): within each dump, map every member span to
/// its class representative (the smallest member span sharing its hash);
/// compare the two member -> representative mappings for exact equality.
/// Hash strings are ignored — they differ between implementations by design.
fn partition_representatives(members: &[PartitionMember]) -> BTreeMap<SpanKey, SpanKey> {
    // class hash -> smallest member, sorted pass
    let mut by_hash: BTreeMap<&str, SpanKey> = BTreeMap::new();
    for m in members {
        let rep = by_hash.get(m.hash.as_str());
        match rep {
            None => {
                by_hash.insert(m.hash.as_str(), m.member.clone());
            }
            Some(existing) if m.member < *existing => {
                by_hash.insert(m.hash.as_str(), m.member.clone());
            }
            _ => {}
        }
    }
    let mut out = BTreeMap::new();
    for m in members {
        if let Some(rep) = by_hash.get(m.hash.as_str()) {
            out.insert(m.member.clone(), rep.clone());
        }
    }
    out
}

fn compare_partitions(left: &PartitionsFile, right: &PartitionsFile, out: &mut Vec<Divergence>) {
    let l: BTreeMap<&str, &PartitionFamily> = left
        .families
        .iter()
        .map(|f| (f.family.as_str(), f))
        .collect();
    let r: BTreeMap<&str, &PartitionFamily> = right
        .families
        .iter()
        .map(|f| (f.family.as_str(), f))
        .collect();
    for name in l
        .keys()
        .chain(r.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        match (l.get(name), r.get(name)) {
            (Some(lf), Some(rf)) => {
                let lmap = partition_representatives(&lf.members);
                let rmap = partition_representatives(&rf.members);
                compare_keyed(
                    &lmap.into_iter().collect::<Vec<_>>(),
                    &rmap.into_iter().collect::<Vec<_>>(),
                    |k: &SpanKey| k.display(),
                    |k: &SpanKey| k.display(),
                    |k: &SpanKey| k.display(),
                    &format!("partitions.{name}"),
                    out,
                );
            }
            (Some(_), None) | (None, Some(_)) => {
                out.push(Divergence {
                    section: "partitions".to_string(),
                    kind: "mismatch",
                    key: format!("family:{name}"),
                    left: l.get(name).map(|f| format!("{} members", f.members.len())),
                    right: r.get(name).map(|f| format!("{} members", f.members.len())),
                });
            }
            (None, None) => {}
        }
    }
}

/// Prompts: join by (functionId, round), compare the byte-exact fields;
/// the first differing byte offset reported with +/-80 bytes of context.
fn compare_prompts(left: &[PromptRecord], right: &[PromptRecord], out: &mut Vec<Divergence>) {
    // Deterministic dispatch order on both sides under warm replay; group
    // by (functionId, round) and compare the groups' rows in order.
    let key = |p: &PromptRecord| (p.function_id.clone(), p.round);
    let mut lgroups: BTreeMap<(String, u64), Vec<&PromptRecord>> = BTreeMap::new();
    let mut rgroups: BTreeMap<(String, u64), Vec<&PromptRecord>> = BTreeMap::new();
    for p in left {
        lgroups.entry(key(p)).or_default().push(p);
    }
    for p in right {
        rgroups.entry(key(p)).or_default().push(p);
    }
    for k in lgroups
        .keys()
        .chain(rgroups.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let (lg, rg) = (lgroups.get(k), rgroups.get(k));
        let (lg, rg) = (
            lg.map(|v| v.as_slice()).unwrap_or(&[]),
            rg.map(|v| v.as_slice()).unwrap_or(&[]),
        );
        if lg.len() != rg.len() {
            out.push(Divergence {
                section: "prompts".to_string(),
                kind: "mismatch",
                key: format!("{}#{}: row count {} vs {}", k.0, k.1, lg.len(), rg.len()),
                left: None,
                right: None,
            });
            continue;
        }
        for (li, ri) in lg.iter().zip(rg.iter()) {
            for field in ["systemPrompt", "userPrompt"] {
                let l = if field == "systemPrompt" {
                    &li.system_prompt
                } else {
                    &li.user_prompt
                };
                let r = if field == "systemPrompt" {
                    &ri.system_prompt
                } else {
                    &ri.user_prompt
                };
                if l != r {
                    let offset = l
                        .as_bytes()
                        .iter()
                        .zip(r.as_bytes())
                        .position(|(a, b)| a != b)
                        .unwrap_or(l.len().min(r.len()));
                    out.push(Divergence {
                        section: "prompts".to_string(),
                        kind: "mismatch",
                        key: format!(
                            "{}#{}#{}: first differing byte at {offset} (left {}/right {} bytes)",
                            k.0,
                            k.1,
                            field,
                            l.len(),
                            r.len()
                        ),
                        left: Some(byte_context(l, offset)),
                        right: Some(byte_context(r, offset)),
                    });
                }
            }
            if li.cache_key != ri.cache_key {
                out.push(Divergence {
                    section: "prompts".to_string(),
                    kind: "mismatch",
                    key: format!("{}#{}#cacheKey", k.0, k.1),
                    left: Some(li.cache_key.clone()),
                    right: Some(ri.cache_key.clone()),
                });
            }
        }
    }
}

/// +/-80 bytes around the first differing offset, for the report.
fn byte_context(text: &str, offset: usize) -> String {
    let bytes = text.as_bytes();
    let start = offset.saturating_sub(80);
    let end = (offset + 80).min(bytes.len());
    String::from_utf8_lossy(&bytes[start..end]).to_string()
}

/// The whole-value sections: jsonl rows load as one array; the rest as
/// JSON; equality is whole-value.
fn compare_whole_value_section(
    left_dir: &Path,
    right_dir: &Path,
    section: &str,
    out: &mut Vec<Divergence>,
) {
    let file = match section {
        "cache-keys" => "cache-keys.jsonl",
        "tree-manifest" => "tree-manifest.json",
        _ => "regions.json",
    };
    let load_value = |dir: &Path| -> Option<Value> {
        if file.ends_with(".jsonl") {
            let text = std::fs::read_to_string(dir.join(file)).ok()?;
            let lines: Vec<Value> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l).ok())
                .collect::<Option<_>>()?;
            Some(serde_json::to_value(lines).ok()?)
        } else {
            read_json(dir, file)
        }
    };
    match (load_value(left_dir), load_value(right_dir)) {
        (Some(l), Some(r)) => {
            if l != r {
                out.push(Divergence {
                    section: section.to_string(),
                    kind: "mismatch",
                    key: file.to_string(),
                    left: Some(value_size(&l)),
                    right: Some(value_size(&r)),
                });
            }
        }
        _ => out.push(file_missing(section)),
    }
}

/// The matches.json compare: pairs by (prior, fresh, cascade), rejections
/// by (prior, cascade).
fn compare_matches(left: &MatchesFile, right: &MatchesFile, out: &mut Vec<Divergence>) {
    compare_keyed(
        &left
            .pairs
            .iter()
            .map(|p| {
                (
                    (p.prior.clone(), p.fresh.clone(), p.cascade.clone()),
                    p.clone(),
                )
            })
            .collect::<Vec<_>>(),
        &right
            .pairs
            .iter()
            .map(|p| {
                (
                    (p.prior.clone(), p.fresh.clone(), p.cascade.clone()),
                    p.clone(),
                )
            })
            .collect::<Vec<_>>(),
        |k: &(SpanKey, SpanKey, String)| format!("{} {} {}", k.0.display(), k.1.display(), k.2),
        |p| format!("{} {} -> {}", p.cascade, p.tier, p.fresh.display()),
        |p| format!("{} {} -> {}", p.cascade, p.tier, p.fresh.display()),
        "matches.pairs",
        out,
    );
    // The key is (prior, cascade, KIND): a prior can carry two rejection
    // rows with different kinds (146 on 2.1.85-2.1.86 — a stillAmbiguous
    // AND a demoted for the same binding) — a two-field key manufactures
    // duplicate-key divergences on them.
    compare_keyed(
        &left
            .rejections
            .iter()
            .map(|p| {
                (
                    (p.prior.clone(), p.cascade.clone(), p.kind.clone()),
                    p.clone(),
                )
            })
            .collect::<Vec<_>>(),
        &right
            .rejections
            .iter()
            .map(|p| {
                (
                    (p.prior.clone(), p.cascade.clone(), p.kind.clone()),
                    p.clone(),
                )
            })
            .collect::<Vec<_>>(),
        |k: &(SpanKey, String, String)| format!("{} {} {}", k.0.display(), k.1, k.2),
        rejection_display,
        rejection_display,
        "matches.rejections",
        out,
    );
    // The two stat bags, whole-value (WP2.1's "resolutionStats identical").
    if left.resolution_stats != right.resolution_stats {
        out.push(Divergence {
            section: "matches.stats".to_string(),
            kind: "mismatch",
            key: "resolutionStats".to_string(),
            left: left.resolution_stats.as_ref().map(value_size),
            right: right.resolution_stats.as_ref().map(value_size),
        });
    }
    if left.binding_resolution_stats != right.binding_resolution_stats {
        out.push(Divergence {
            section: "matches.stats".to_string(),
            kind: "mismatch",
            key: "bindingResolutionStats".to_string(),
            left: left.binding_resolution_stats.as_ref().map(value_size),
            right: right.binding_resolution_stats.as_ref().map(value_size),
        });
    }
}

fn rejection_display(p: &MatchRejection) -> String {
    format!(
        "{} {} {}",
        p.cascade,
        p.kind,
        p.candidates.as_ref().map(|c| c.len()).unwrap_or(0)
    )
}

/// The matches-close.json compare (WP2.2's gate): candidates keyed by
/// (prior, fresh) span — whole value, so an outcome, rank or score-bits
/// change is caught — pairs keyed the same way; the stats bag and the two
/// skip counters whole-value.
fn compare_matches_close(
    left: &MatchesCloseFile,
    right: &MatchesCloseFile,
    out: &mut Vec<Divergence>,
) {
    compare_keyed(
        &left
            .candidates
            .iter()
            .map(|c| ((c.prior.clone(), c.fresh.clone()), c.clone()))
            .collect::<Vec<_>>(),
        &right
            .candidates
            .iter()
            .map(|c| ((c.prior.clone(), c.fresh.clone()), c.clone()))
            .collect::<Vec<_>>(),
        |k: &(SpanKey, SpanKey)| format!("{} {}", k.0.display(), k.1.display()),
        close_candidate_display,
        close_candidate_display,
        "matches.close.candidates",
        out,
    );
    compare_keyed(
        &left
            .pairs
            .iter()
            .map(|p| ((p.prior.clone(), p.fresh.clone()), p.clone()))
            .collect::<Vec<_>>(),
        &right
            .pairs
            .iter()
            .map(|p| ((p.prior.clone(), p.fresh.clone()), p.clone()))
            .collect::<Vec<_>>(),
        |k: &(SpanKey, SpanKey)| format!("{} {}", k.0.display(), k.1.display()),
        close_pair_display,
        close_pair_display,
        "matches.close.pairs",
        out,
    );
    if left.stats != right.stats {
        out.push(Divergence {
            section: "matches.close.stats".to_string(),
            kind: "mismatch",
            key: "stats".to_string(),
            left: Some(value_size(
                &serde_json::to_value(&left.stats).unwrap_or_default(),
            )),
            right: Some(value_size(
                &serde_json::to_value(&right.stats).unwrap_or_default(),
            )),
        });
    }
    if left.skipped_old != right.skipped_old {
        out.push(Divergence {
            section: "matches.close.skipped".to_string(),
            kind: "mismatch",
            key: "skippedOld".to_string(),
            left: Some(left.skipped_old.to_string()),
            right: Some(right.skipped_old.to_string()),
        });
    }
    if left.skipped_new != right.skipped_new {
        out.push(Divergence {
            section: "matches.close.skipped".to_string(),
            kind: "mismatch",
            key: "skippedNew".to_string(),
            left: Some(left.skipped_new.to_string()),
            right: Some(right.skipped_new.to_string()),
        });
    }
}

fn close_candidate_display(c: &CloseCandidateRow) -> String {
    format!(
        "rank={} outcome={} score={} bits={}",
        c.rank, c.outcome, c.score, c.score_bits
    )
}

fn close_pair_display(p: &ClosePairRow) -> String {
    format!(
        "verdict={} aligned={}/{} transfers={} hints={} snaps={}",
        p.verdict,
        p.aligned_statements,
        p.total_new_statements,
        p.transfers.len(),
        p.hints.len(),
        p.snaps.len()
    )
}

/// The one-row-shape compare the simple keyed sections share: rows of one
/// file, joined by a key extracted from each row, whole-value compare.
fn compare_rows<T, K, KF, KD, G>(
    left: &[T],
    right: &[T],
    key_of: KF,
    key_display: KD,
    display: G,
    section: &str,
    out: &mut Vec<Divergence>,
) where
    T: Clone + PartialEq,
    K: Ord + Clone,
    KF: Fn(&T) -> K,
    KD: Fn(&K) -> String,
    G: Fn(&T) -> String,
{
    compare_keyed(
        &left
            .iter()
            .map(|r| (key_of(r), r.clone()))
            .collect::<Vec<_>>(),
        &right
            .iter()
            .map(|r| (key_of(r), r.clone()))
            .collect::<Vec<_>>(),
        key_display,
        &display,
        &display,
        section,
        out,
    );
}

/// The functions.json compare: rows keyed by span, whole-value compare.
fn compare_function_rows(
    left: &FunctionsFile,
    right: &FunctionsFile,
    section: &str,
    out: &mut Vec<Divergence>,
) {
    // The WP1.4 GATE's fields: graph edges (internalCallees) + scope
    // parents, plus the row identity (key, kind, sessionId) and the
    // module-binding rows' names (the ORIGINAL minified names — module
    // bindings are never prior-transferred, so their names are graph-time
    // state). The function rows' `name` and `bindings` columns are
    // POST-PRIOR-TRANSFER state in the TS dump (captureGraphDump runs
    // after applyPriorVersionIfPresent — the transferred name sits in the
    // graph's live AST), so they are gating fields of the TRANSFER port
    // (WP2/WP3), not of the graph; the structuralHash BYTES differ by
    // design (02 §4a) — the hash classes are the partitions section's
    // structuralHash family.
    let projection = |f: &FunctionRow| FunctionGateFields {
        kind: f.kind.clone(),
        session_id: f.session_id.clone(),
        name: if f.kind == "module-binding" {
            Some(f.name.clone())
        } else {
            None
        },
        internal_callees: f.internal_callees.clone(),
        scope_parent: f.scope_parent.clone(),
    };
    compare_keyed(
        &left
            .functions
            .iter()
            .map(|f| (f.key.clone(), projection(f)))
            .collect::<Vec<_>>(),
        &right
            .functions
            .iter()
            .map(|f| (f.key.clone(), projection(f)))
            .collect::<Vec<_>>(),
        |k: &SpanKey| k.display(),
        function_gate_display,
        function_gate_display,
        section,
        out,
    );
}

/// The twins.json compare: the two inventories + the unique-tier pair
/// set (spans exact; the hash column is informational — digest bytes are
/// serializer artifacts).
fn compare_twins(left: &TwinsFile, right: &TwinsFile, out: &mut Vec<Divergence>) {
    if left.inventories != right.inventories {
        out.push(Divergence {
            section: "twins.inventories".to_string(),
            kind: "mismatch",
            key: "inventories".to_string(),
            left: Some(format!("{:?}", left.inventories)),
            right: Some(format!("{:?}", right.inventories)),
        });
    }
    if left.unique_tier.unique_twins != right.unique_tier.unique_twins {
        out.push(Divergence {
            section: "twins".to_string(),
            kind: "mismatch",
            key: "uniqueTwins".to_string(),
            left: Some(left.unique_tier.unique_twins.to_string()),
            right: Some(right.unique_tier.unique_twins.to_string()),
        });
    }
    // The pairs, keyed by fresh span — WITHOUT the hash column (digest
    // bytes are serializer artifacts, 02 §4a; the identity is the spans).
    compare_keyed(
        &left
            .unique_tier
            .pairs
            .iter()
            .map(|p| (p.fresh.clone(), p.prior.clone()))
            .collect::<Vec<_>>(),
        &right
            .unique_tier
            .pairs
            .iter()
            .map(|p| (p.fresh.clone(), p.prior.clone()))
            .collect::<Vec<_>>(),
        |k: &SpanKey| k.display(),
        |v: &SpanKey| v.display(),
        |v: &SpanKey| v.display(),
        "twins.pairs",
        out,
    );
}

/// The twin-gates.json compare: rows keyed by (tier, fresh span) on the
/// fields both implementations carry (outcome, slots, the pairs' names);
/// the stats bag + conflicts whole-value.
fn compare_twin_gates(left: &TwinsGatesFile, right: &TwinsGatesFile, out: &mut Vec<Divergence>) {
    if left.stats != right.stats {
        out.push(Divergence {
            section: "twins.gates.stats".to_string(),
            kind: "mismatch",
            key: "stats".to_string(),
            left: Some(value_size(
                &serde_json::to_value(&left.stats).unwrap_or_default(),
            )),
            right: Some(value_size(
                &serde_json::to_value(&right.stats).unwrap_or_default(),
            )),
        });
    }
    if left.conflicts != right.conflicts {
        out.push(Divergence {
            section: "twins.gates".to_string(),
            kind: "mismatch",
            key: "conflicts".to_string(),
            left: left.conflicts.as_ref().map(|c| c.len().to_string()),
            right: right.conflicts.as_ref().map(|c| c.len().to_string()),
        });
    }
    compare_keyed(
        &left
            .rows
            .iter()
            .map(|r| ((r.tier.clone(), r.fresh.clone()), r.clone()))
            .collect::<Vec<_>>(),
        &right
            .rows
            .iter()
            .map(|r| ((r.tier.clone(), r.fresh.clone()), r.clone()))
            .collect::<Vec<_>>(),
        |k: &(String, SpanKey)| format!("{} {}", k.0, k.1.display()),
        twin_gate_row_display,
        twin_gate_row_display,
        "twins.gates.rows",
        out,
    );
}

fn twin_gate_row_display(r: &TwinGateRow) -> String {
    format!(
        "prior={} outcome={} slots={} pairs={}",
        r.prior.display(),
        r.outcome,
        r.slots
            .map(|s| s.to_string())
            .unwrap_or_else(|| "-".to_string()),
        r.pairs
            .as_ref()
            .map(|p| p.len().to_string())
            .unwrap_or_else(|| "-".to_string())
    )
}

/// The modules.json compare (WP1.5's module-boundary sets): each site's
/// helper var, wrapper, then the factory rows keyed by span.
fn compare_bun_modules(
    left: &ModulesFile,
    right: &ModulesFile,
    section: &str,
    out: &mut Vec<Divergence>,
) {
    for (site, l, r) in [
        ("unpack", &left.unpack, &right.unpack),
        ("graph", &left.graph, &right.graph),
    ] {
        if l == r {
            continue;
        }
        match (l, r) {
            (Some(l), Some(r)) => compare_modules_site(l, r, section, site, out),
            _ => out.push(Divergence {
                section: section.to_string(),
                kind: "mismatch",
                key: format!("{site}:presence"),
                left: l.as_ref().map(|_| "present".to_string()),
                right: r.as_ref().map(|_| "present".to_string()),
            }),
        }
    }
}

fn compare_modules_site(
    left: &ModulesData,
    right: &ModulesData,
    section: &str,
    site: &str,
    out: &mut Vec<Divergence>,
) {
    if left.helper_var != right.helper_var {
        out.push(Divergence {
            section: section.to_string(),
            kind: "mismatch",
            key: format!("{site}:helperVar"),
            left: Some(left.helper_var.clone()),
            right: Some(right.helper_var.clone()),
        });
    }
    if left.wrapper != right.wrapper {
        out.push(Divergence {
            section: section.to_string(),
            kind: "mismatch",
            key: format!("{site}:wrapper"),
            left: Some(format!("{:?}", left.wrapper)),
            right: Some(format!("{:?}", right.wrapper)),
        });
    }
    // The rows compare WITHOUT the structuralHash bytes (02 §4a: hash
    // bytes differ between implementations by design) — then the hash
    // CLASSES are compared partition-style: member -> smallest member
    // sharing its hash, exactly (same rule as partitions.json).
    compare_keyed(
        &left
            .factories
            .iter()
            .map(|f| (f.key.clone(), factory_row_without_hash(f)))
            .collect::<Vec<_>>(),
        &right
            .factories
            .iter()
            .map(|f| (f.key.clone(), factory_row_without_hash(f)))
            .collect::<Vec<_>>(),
        |k: &SpanKey| format!("{site}:{}", k.display()),
        |r: &FactoryRowIdentity| {
            format!(
                "factory var={} lineRange={:?} contentHash={} bannerText={} bannerPackage={} bannerVersion={}",
                r.0,
                r.1,
                r.2,
                r.3.as_deref().unwrap_or("<none>"),
                r.4.as_deref().unwrap_or("<none>"),
                r.5.as_deref().unwrap_or("<none>")
            )
        },
        |r: &FactoryRowIdentity| {
            format!(
                "factory var={} lineRange={:?} contentHash={} bannerText={} bannerPackage={} bannerVersion={}",
                r.0,
                r.1,
                r.2,
                r.3.as_deref().unwrap_or("<none>"),
                r.4.as_deref().unwrap_or("<none>"),
                r.5.as_deref().unwrap_or("<none>")
            )
        },
        section,
        out,
    );
    let left_classes = factory_hash_representatives(&left.factories);
    let right_classes = factory_hash_representatives(&right.factories);
    compare_keyed(
        &left_classes.into_iter().collect::<Vec<_>>(),
        &right_classes.into_iter().collect::<Vec<_>>(),
        |k: &SpanKey| format!("{site}:class:{}", k.display()),
        |k: &SpanKey| k.display(),
        |k: &SpanKey| k.display(),
        section,
        out,
    );
}

/// The row's identity without the by-design-divergent hash bytes.
type FactoryRowIdentity = (
    String,
    (i64, i64),
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn factory_row_without_hash(f: &ModulesFactoryRow) -> FactoryRowIdentity {
    (
        f.factory_var.clone(),
        f.line_range,
        f.content_hash.clone(),
        f.banner_text.clone(),
        f.banner_package.clone(),
        f.banner_version.clone(),
    )
}

/// member key -> the smallest key sharing its structuralHash (the
/// partition-representative rule, over the factory rows).
fn factory_hash_representatives(rows: &[ModulesFactoryRow]) -> BTreeMap<SpanKey, SpanKey> {
    let mut by_hash: BTreeMap<&str, SpanKey> = BTreeMap::new();
    for f in rows {
        match by_hash.get(f.structural_hash.as_str()) {
            None => {
                by_hash.insert(f.structural_hash.as_str(), f.key.clone());
            }
            Some(existing) if f.key < *existing => {
                by_hash.insert(f.structural_hash.as_str(), f.key.clone());
            }
            _ => {}
        }
    }
    let mut out = BTreeMap::new();
    for f in rows {
        let rep = by_hash[f.structural_hash.as_str()].clone();
        out.insert(f.key.clone(), rep);
    }
    out
}

/// Load and compare one pair of dumps.
pub fn compare_dumps(
    left_dir: &Path,
    right_dir: &Path,
    sections: &[String],
    max_divergences: usize,
) -> Result<CompareOutcome, String> {
    let mut why = None;
    let left_meta: MetaFile =
        read_json_strict(left_dir, "meta.json", &mut why).ok_or_else(|| {
            why.clone()
                .unwrap_or_else(|| "meta.json unreadable".to_string())
        })?;
    let right_meta: MetaFile =
        read_json_strict(right_dir, "meta.json", &mut why).ok_or_else(|| {
            why.clone()
                .unwrap_or_else(|| "meta.json unreadable".to_string())
        })?;

    if let Some(reason) = anchors_comparable(&left_meta, &right_meta) {
        return Ok(CompareOutcome {
            comparable: false,
            not_comparable_reason: Some(reason),
            divergences: vec![],
        });
    }

    let mut outcome = CompareOutcome {
        comparable: true,
        not_comparable_reason: None,
        divergences: vec![],
    };

    for section in sections {
        compare_section(section, left_dir, right_dir, &mut outcome.divergences)?;
    }

    if outcome.divergences.len() > max_divergences {
        outcome.divergences.truncate(max_divergences);
        outcome.divergences.push(Divergence {
            section: "report".to_string(),
            kind: "truncated",
            key: format!("further divergences suppressed (max {max_divergences})"),
            left: None,
            right: None,
        });
    }
    Ok(outcome)
}

/// One section's comparison — [`compare_dumps`]'s dispatch body, extracted
/// to keep each function under the complexity gate.
fn compare_section(
    section: &str,
    left_dir: &Path,
    right_dir: &Path,
    divergences: &mut Vec<Divergence>,
) -> Result<(), String> {
    match section {
        "functions" => {
            let (l, r) =
                both::<FunctionsFile>(left_dir, right_dir, "functions.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_function_rows(&l, &r, section, divergences);
            }
        }
        "twins" => {
            let (l, r) = both::<TwinsFile>(left_dir, right_dir, "twins.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_twins(&l, &r, divergences);
            }
            // The gates half: twin-gates.json, optional on both sides
            // (the dump predates it — absent on both is agreement).
            let gl: Option<TwinsGatesFile> = read_json(left_dir, "twin-gates.json");
            let gr: Option<TwinsGatesFile> = read_json(right_dir, "twin-gates.json");
            match (gl, gr) {
                (Some(l), Some(r)) => compare_twin_gates(&l, &r, divergences),
                (None, None) => {}
                _ => divergences.push(file_missing("twins.gates")),
            }
        }
        "modules" => {
            let (l, r) =
                both::<ModulesFile>(left_dir, right_dir, "modules.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_bun_modules(&l, &r, section, divergences);
            }
        }
        "partitions" => {
            let l: Option<PartitionsFile> = read_json(left_dir, "partitions.json");
            let r: Option<PartitionsFile> = read_json(right_dir, "partitions.json");
            match (l, r) {
                (Some(l), Some(r)) => compare_partitions(&l, &r, divergences),
                _ => divergences.push(file_missing(section)),
            }
        }
        "matches" => {
            let (l, r) =
                both::<MatchesFile>(left_dir, right_dir, "matches.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_matches(&l, &r, divergences);
            }
        }
        "matches.close" => {
            // WP2.2's gate: the close tier's candidates' fates +
            // corroboration verdicts. Optional on both sides (the TS
            // records nothing when one side has no unmatched
            // functions — absent-on-both is agreement).
            let l: Option<MatchesCloseFile> = read_json(left_dir, "matches-close.json");
            let r: Option<MatchesCloseFile> = read_json(right_dir, "matches-close.json");
            match (l, r) {
                (Some(l), Some(r)) => compare_matches_close(&l, &r, divergences),
                (None, None) => {}
                _ => divergences.push(file_missing(section)),
            }
        }
        "transfers" => {
            let (l, r) =
                both::<TransfersFile>(left_dir, right_dir, "transfers.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_rows(
                    &l.transfers,
                    &r.transfers,
                    |t| t.target.clone(),
                    |k: &SpanKey| k.display(),
                    transfer_display,
                    section,
                    divergences,
                );
            }
        }
        "votes" => {
            let (l, r) = both::<VotesFile>(left_dir, right_dir, "votes.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_rows(
                    &l.votes,
                    &r.votes,
                    |v| (v.target.clone(), v.target_kind.clone()),
                    |k: &(SpanKey, String)| format!("{} {}", k.0.display(), k.1),
                    vote_display,
                    section,
                    divergences,
                );
            }
        }
        "names" => {
            let (l, r) = both::<NamesFile>(left_dir, right_dir, "names.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_rows(
                    &l.names,
                    &r.names,
                    |n| n.target.clone(),
                    |k: &SpanKey| k.display(),
                    name_display,
                    section,
                    divergences,
                );
            }
        }
        "placement" => {
            let (l, r) =
                both::<PlacementFile>(left_dir, right_dir, "placement.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_rows(
                    &l.placements,
                    &r.placements,
                    |p| p.key.clone(),
                    |k: &SpanKey| k.display(),
                    placement_display,
                    section,
                    divergences,
                );
            }
        }
        "emit" => {
            let (l, r) =
                both::<EmitLayoutFile>(left_dir, right_dir, "emit.json", divergences, section);
            if let (Some(l), Some(r)) = (l, r) {
                compare_rows(
                    &l.files,
                    &r.files,
                    |f| f.path.clone(),
                    |k: &String| k.clone(),
                    emit_display,
                    section,
                    divergences,
                );
            }
        }
        "prompts" => {
            let l = read_prompts(left_dir);
            let r = read_prompts(right_dir);
            match (l, r) {
                (Some(l), Some(r)) => compare_prompts(&l, &r, divergences),
                _ => divergences.push(file_missing(section)),
            }
        }
        "cache-keys" | "tree-manifest" | "regions" => {
            compare_whole_value_section(left_dir, right_dir, section, divergences);
        }
        other => {
            return Err(format!(
                "unknown section '{other}' (known: {})",
                ALL_SECTIONS.join(",")
            ));
        }
    }
    Ok(())
}

fn both<T: serde::de::DeserializeOwned>(
    left_dir: &Path,
    right_dir: &Path,
    file: &str,
    out: &mut Vec<Divergence>,
    section: &str,
) -> (Option<T>, Option<T>) {
    let l: Option<T> = read_json(left_dir, file);
    let r: Option<T> = read_json(right_dir, file);
    // Absence on BOTH sides is agreement (a non-Bun fixture has no
    // modules.json on either side; a Rust failure to produce a file the TS
    // wrote is still caught — one-sided absence diverges).
    if l.is_none() && r.is_none() {
        return (None, None);
    }
    if l.is_none() || r.is_none() {
        out.push(file_missing(section));
    }
    (l, r)
}

fn file_missing(section: &str) -> Divergence {
    Divergence {
        section: section.to_string(),
        kind: "missing-file",
        key: section.to_string(),
        left: None,
        right: None,
    }
}

fn value_size(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("array of {}", a.len()),
        Value::Object(o) => format!("object with {} keys", o.len()),
        other => format!("{other}"),
    }
}

/// The gate's field projection for one functions.json row.
#[derive(Clone, PartialEq, Eq, Debug)]
struct FunctionGateFields {
    kind: String,
    session_id: String,
    name: Option<String>,
    internal_callees: Vec<SpanKey>,
    scope_parent: Option<SpanKey>,
}

fn function_gate_display(f: &FunctionGateFields) -> String {
    format!(
        "kind={} id={} name={} callees={} scopeParent={}",
        f.kind,
        f.session_id,
        f.name.as_deref().unwrap_or("<n/a>"),
        f.internal_callees.len(),
        f.scope_parent
            .as_ref()
            .map(|s| s.display())
            .unwrap_or_else(|| "null".to_string())
    )
}

fn transfer_display(t: &TransferRow) -> String {
    format!(
        "{} -> {:?} settledBy={:?} attempts={}",
        t.old_name,
        t.final_name,
        t.settled_by,
        t.attempts.len()
    )
}

fn vote_display(v: &VoteRow) -> String {
    format!(
        "{} outcome={:?} tally={:?}",
        v.target_kind,
        v.outcome,
        v.tally
            .iter()
            .map(|t| format!("{}x{}", t.name, t.total))
            .collect::<Vec<_>>()
    )
}

fn name_display(n: &NameRecord) -> String {
    format!(
        "{} {} -> {:?} classified={}",
        n.kind, n.old_name, n.new_name, n.classified
    )
}

fn placement_display(p: &PlacementRow) -> String {
    format!("{} -> {} ({})", p.index, p.file, p.placed_by)
}

fn emit_display(e: &EmitFileRow) -> String {
    format!(
        "{} statements={} alias={:?}",
        e.path,
        e.statements.len(),
        e.alias
    )
}

fn read_prompts(dir: &Path) -> Option<Vec<PromptRecord>> {
    let text = std::fs::read_to_string(dir.join("prompts.jsonl")).ok()?;
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<PromptRecord>(line) {
            Ok(p) => rows.push(p),
            Err(_) => return None,
        }
    }
    Some(rows)
}
