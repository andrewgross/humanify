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
pub const ALL_SECTIONS: [&str; 11] = [
    "functions",
    "partitions",
    "matches",
    "transfers",
    "votes",
    "names",
    "placement",
    "emit",
    "prompts",
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
        match section.as_str() {
            "functions" => {
                let (l, r) = both::<FunctionsFile>(
                    left_dir,
                    right_dir,
                    "functions.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.functions
                            .iter()
                            .map(|f| (f.key.clone(), f.clone()))
                            .collect::<Vec<_>>(),
                        &r.functions
                            .iter()
                            .map(|f| (f.key.clone(), f.clone()))
                            .collect::<Vec<_>>(),
                        |k: &SpanKey| k.display(),
                        function_display,
                        function_display,
                        section,
                        &mut outcome.divergences,
                    );
                }
            }
            "partitions" => {
                let l: Option<PartitionsFile> = read_json(left_dir, "partitions.json");
                let r: Option<PartitionsFile> = read_json(right_dir, "partitions.json");
                if let (Some(l), Some(r)) = (l, r) {
                    compare_partitions(&l, &r, &mut outcome.divergences);
                } else {
                    outcome.divergences.push(file_missing(section));
                }
            }
            "matches" => {
                let (l, r) = both::<MatchesFile>(
                    left_dir,
                    right_dir,
                    "matches.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.pairs
                            .iter()
                            .map(|p| {
                                (
                                    (p.prior.clone(), p.fresh.clone(), p.cascade.clone()),
                                    p.clone(),
                                )
                            })
                            .collect::<Vec<_>>(),
                        &r.pairs
                            .iter()
                            .map(|p| {
                                (
                                    (p.prior.clone(), p.fresh.clone(), p.cascade.clone()),
                                    p.clone(),
                                )
                            })
                            .collect::<Vec<_>>(),
                        |k: &(SpanKey, SpanKey, String)| {
                            format!("{} {} {}", k.0.display(), k.1.display(), k.2)
                        },
                        |p| format!("{} {} -> {}", p.cascade, p.tier, p.fresh.display()),
                        |p| format!("{} {} -> {}", p.cascade, p.tier, p.fresh.display()),
                        "matches.pairs",
                        &mut outcome.divergences,
                    );
                    compare_keyed(
                        &l.rejections
                            .iter()
                            .map(|p| ((p.prior.clone(), p.cascade.clone()), p.clone()))
                            .collect::<Vec<_>>(),
                        &r.rejections
                            .iter()
                            .map(|p| ((p.prior.clone(), p.cascade.clone()), p.clone()))
                            .collect::<Vec<_>>(),
                        |k: &(SpanKey, String)| format!("{} {}", k.0.display(), k.1),
                        |p| {
                            format!(
                                "{} {} {}",
                                p.cascade,
                                p.kind,
                                p.candidates.as_ref().map(|c| c.len()).unwrap_or(0)
                            )
                        },
                        |p| {
                            format!(
                                "{} {} {}",
                                p.cascade,
                                p.kind,
                                p.candidates.as_ref().map(|c| c.len()).unwrap_or(0)
                            )
                        },
                        "matches.rejections",
                        &mut outcome.divergences,
                    );
                }
            }
            "transfers" => {
                let (l, r) = both::<TransfersFile>(
                    left_dir,
                    right_dir,
                    "transfers.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.transfers
                            .iter()
                            .map(|t| (t.target.clone(), t.clone()))
                            .collect::<Vec<_>>(),
                        &r.transfers
                            .iter()
                            .map(|t| (t.target.clone(), t.clone()))
                            .collect::<Vec<_>>(),
                        |k: &SpanKey| k.display(),
                        transfer_display,
                        transfer_display,
                        section,
                        &mut outcome.divergences,
                    );
                }
            }
            "votes" => {
                let (l, r) = both::<VotesFile>(
                    left_dir,
                    right_dir,
                    "votes.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.votes
                            .iter()
                            .map(|v| ((v.target.clone(), v.target_kind.clone()), v.clone()))
                            .collect::<Vec<_>>(),
                        &r.votes
                            .iter()
                            .map(|v| ((v.target.clone(), v.target_kind.clone()), v.clone()))
                            .collect::<Vec<_>>(),
                        |k: &(SpanKey, String)| format!("{} {}", k.0.display(), k.1),
                        vote_display,
                        vote_display,
                        section,
                        &mut outcome.divergences,
                    );
                }
            }
            "names" => {
                let (l, r) = both::<NamesFile>(
                    left_dir,
                    right_dir,
                    "names.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.names
                            .iter()
                            .map(|n| (n.target.clone(), n.clone()))
                            .collect::<Vec<_>>(),
                        &r.names
                            .iter()
                            .map(|n| (n.target.clone(), n.clone()))
                            .collect::<Vec<_>>(),
                        |k: &SpanKey| k.display(),
                        name_display,
                        name_display,
                        section,
                        &mut outcome.divergences,
                    );
                }
            }
            "placement" => {
                let (l, r) = both::<PlacementFile>(
                    left_dir,
                    right_dir,
                    "placement.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.placements
                            .iter()
                            .map(|p| (p.key.clone(), p.clone()))
                            .collect::<Vec<_>>(),
                        &r.placements
                            .iter()
                            .map(|p| (p.key.clone(), p.clone()))
                            .collect::<Vec<_>>(),
                        |k: &SpanKey| k.display(),
                        placement_display,
                        placement_display,
                        section,
                        &mut outcome.divergences,
                    );
                }
            }
            "emit" => {
                let (l, r) = both::<EmitLayoutFile>(
                    left_dir,
                    right_dir,
                    "emit.json",
                    &mut outcome.divergences,
                    section,
                );
                if let (Some(l), Some(r)) = (l, r) {
                    compare_keyed(
                        &l.files
                            .iter()
                            .map(|f| (f.path.clone(), f.clone()))
                            .collect::<Vec<_>>(),
                        &r.files
                            .iter()
                            .map(|f| (f.path.clone(), f.clone()))
                            .collect::<Vec<_>>(),
                        |k: &String| k.clone(),
                        emit_display,
                        emit_display,
                        section,
                        &mut outcome.divergences,
                    );
                }
            }
            "prompts" => {
                let l = read_prompts(left_dir);
                let r = read_prompts(right_dir);
                match (l, r) {
                    (Some(l), Some(r)) => compare_prompts(&l, &r, &mut outcome.divergences),
                    _ => outcome.divergences.push(file_missing(section)),
                }
            }
            "tree-manifest" | "regions" => {
                let file = if section == "tree-manifest" {
                    "tree-manifest.json"
                } else {
                    "regions.json"
                };
                let l: Option<Value> = read_json(left_dir, file);
                let r: Option<Value> = read_json(right_dir, file);
                match (l, r) {
                    (Some(l), Some(r)) => {
                        if l != r {
                            outcome.divergences.push(Divergence {
                                section: section.to_string(),
                                kind: "mismatch",
                                key: file.to_string(),
                                left: Some(value_size(&l)),
                                right: Some(value_size(&r)),
                            });
                        }
                    }
                    _ => outcome.divergences.push(file_missing(section)),
                }
            }
            other => {
                return Err(format!(
                    "unknown section '{other}' (known: {})",
                    ALL_SECTIONS.join(",")
                ));
            }
        }
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

fn both<T: serde::de::DeserializeOwned>(
    left_dir: &Path,
    right_dir: &Path,
    file: &str,
    out: &mut Vec<Divergence>,
    section: &str,
) -> (Option<T>, Option<T>) {
    let l: Option<T> = read_json(left_dir, file);
    let r: Option<T> = read_json(right_dir, file);
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

fn function_display(f: &FunctionRow) -> String {
    format!(
        "{} kind={} hash={} callees={} bindings={}",
        f.kind,
        f.name,
        f.structural_hash,
        f.internal_callees.len(),
        f.bindings.len()
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
