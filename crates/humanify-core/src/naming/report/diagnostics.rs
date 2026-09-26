//! `buildDiagnosticsReport` (src/rename/diagnostics.ts) — the
//! `--diagnostics` JSON: per-identifier outcomes from the rename reports,
//! the strategy trail with its funnel (the TS `StrategyTrailReport` shape:
//! raw UTF-16 declaration spans and Babel `line:col` locs in each row's
//! anchored text), the contention events, the totals-first identifier
//! ledger and the pattern roll-ups. `placementTrails` is the split's
//! (WP5.x) and is appended by it.

use std::collections::HashMap;

use humanify_model::js::{JsObject, JsValue, math_round};
use humanify_model::jsshape::JsType;
use humanify_model::stats::{CoverageSummary, TransferStatsByTier};

use super::{ContentionEvent, IdentifierOutcome, RenameReport, Status};
use crate::babel_view::BabelLines;
use crate::trail::{Anchor, StrategyTrail, TrailEntry};

/// The anchored texts a trail row's spans index into.
#[derive(Clone, Copy, Default)]
pub struct AnchorTexts<'t> {
    pub fresh: &'t str,
    pub generated: Option<&'t str>,
    pub reconciled: Option<&'t str>,
    pub shipped: Option<&'t str>,
}

impl<'t> AnchorTexts<'t> {
    fn text(&self, anchor: Anchor) -> &'t str {
        match anchor {
            Anchor::Fresh => Some(self.fresh),
            Anchor::Generated => self.generated,
            Anchor::Reconciled => self.reconciled,
            Anchor::Shipped => self.shipped,
        }
        .unwrap_or(self.fresh)
    }
}

/// Byte → UTF-16 offsets of one text, for a known set of positions.
struct Utf16Map(HashMap<u32, u32>);

impl Utf16Map {
    fn build(text: &str, mut positions: Vec<u32>) -> Utf16Map {
        positions.sort_unstable();
        positions.dedup();
        let mut map = HashMap::with_capacity(positions.len());
        let (mut byte, mut units) = (0usize, 0u32);
        let mut chars = text.chars();
        for p in positions {
            while byte < p as usize {
                let Some(c) = chars.next() else { break };
                byte += c.len_utf8();
                units += c.len_utf16() as u32;
            }
            map.insert(p, units);
        }
        Utf16Map(map)
    }

    fn get(&self, p: u32) -> u32 {
        self.0[&p]
    }
}

fn num(n: impl Into<f64>) -> JsValue {
    JsValue::Number(n.into())
}

fn obj(entries: Vec<(&str, Option<JsValue>)>) -> JsValue {
    let mut o = JsObject::new();
    for (k, v) in entries {
        o.insert_opt(k, v);
    }
    JsValue::Object(o)
}

/// The report entries' shared provenance (`functionId`, `strategy`,
/// `structuralHash`, `trail`).
fn provenance(
    report: &RenameReport,
    outcome: &IdentifierOutcome,
) -> Vec<(&'static str, Option<JsValue>)> {
    vec![
        ("functionId", Some(JsValue::str(&report.target_id))),
        ("strategy", Some(JsValue::str(report.strategy.as_str()))),
        (
            "structuralHash",
            report.structural_hash.as_deref().map(JsValue::str),
        ),
        (
            "trail",
            outcome.trail.as_ref().map(|t| {
                JsValue::Array(
                    t.iter()
                        .map(|a| {
                            obj(vec![
                                ("round", Some(num(a.round as f64))),
                                ("proposed", a.proposed.as_deref().map(JsValue::str)),
                                ("result", Some(JsValue::str(a.result.as_str()))),
                            ])
                        })
                        .collect(),
                )
            }),
        ),
    ]
}

/// `Map<K, number>` bump in insertion order.
fn bump<K: PartialEq + Clone>(m: &mut Vec<(K, u64)>, k: &K) {
    match m.iter_mut().find(|(x, _)| x == k) {
        Some((_, n)) => *n += 1,
        None => m.push((k.clone(), 1)),
    }
}

#[derive(Default)]
struct Buckets {
    unchanged: Vec<JsValue>,
    missing: Vec<JsValue>,
    duplicate: Vec<JsValue>,
    invalid: Vec<JsValue>,
    renamed: Vec<JsValue>,
    renamed_names: Vec<String>,
    unrenamed_names: [Vec<String>; 4],
    collision_targets: Vec<(String, u64)>,
    unchanged_names: Vec<String>,
    by_attempts: Vec<(u64, u64)>,
    missing_by_finish: Vec<(String, u64)>,
}

fn classify(reports: &[RenameReport]) -> Buckets {
    let mut b = Buckets::default();
    for report in reports {
        for (name, outcome) in report.outcomes.iter() {
            let prov = provenance(report, outcome);
            let head = |mut v: Vec<(&'static str, Option<JsValue>)>| {
                v.extend(prov.clone());
                obj(v)
            };
            match &outcome.status {
                Status::Renamed { new_name, round } => {
                    b.renamed.push(head(vec![
                        ("name", Some(JsValue::str(name))),
                        ("newName", Some(JsValue::str(new_name))),
                        ("round", Some(num(*round as f64))),
                    ]));
                    b.renamed_names.push(new_name.clone());
                }
                Status::Unchanged {
                    attempts,
                    suggestion,
                } => {
                    b.unchanged.push(head(vec![
                        ("name", Some(JsValue::str(name))),
                        ("suggestion", suggestion.as_deref().map(JsValue::str)),
                        ("reason", Some(JsValue::str("LLM returned original name"))),
                        ("attempts", Some(num(*attempts as f64))),
                    ]));
                    b.unchanged_names.push(name.clone());
                    b.unrenamed_names[0].push(name.clone());
                    bump(&mut b.by_attempts, attempts);
                }
                Status::Missing {
                    attempts,
                    last_finish_reason,
                } => {
                    let detail = last_finish_reason
                        .as_deref()
                        .filter(|r| !r.is_empty())
                        .map(|r| JsValue::str(format!("finish_reason: {r}")));
                    b.missing.push(head(vec![
                        ("name", Some(JsValue::str(name))),
                        (
                            "reason",
                            Some(JsValue::str("LLM did not return this identifier")),
                        ),
                        ("attempts", Some(num(*attempts as f64))),
                        ("detail", detail),
                    ]));
                    b.unrenamed_names[1].push(name.clone());
                    bump(&mut b.by_attempts, attempts);
                    let fr = last_finish_reason
                        .clone()
                        .unwrap_or_else(|| "unknown".into());
                    bump(&mut b.missing_by_finish, &fr);
                }
                Status::Duplicate {
                    conflicted_with,
                    attempts,
                    suggestion,
                } => {
                    b.duplicate.push(head(vec![
                        ("name", Some(JsValue::str(name))),
                        ("suggestion", suggestion.as_deref().map(JsValue::str)),
                        ("reason", Some(JsValue::str("Name collision unresolved"))),
                        ("attempts", Some(num(*attempts as f64))),
                        (
                            "detail",
                            Some(JsValue::str(format!("conflicted with: {conflicted_with}"))),
                        ),
                    ]));
                    b.unrenamed_names[2].push(name.clone());
                    bump(&mut b.collision_targets, conflicted_with);
                    bump(&mut b.by_attempts, attempts);
                }
                Status::Invalid {
                    attempts,
                    suggestion,
                } => {
                    b.invalid.push(head(vec![
                        ("name", Some(JsValue::str(name))),
                        ("suggestion", suggestion.as_deref().map(JsValue::str)),
                        ("reason", Some(JsValue::str("Invalid identifier returned"))),
                        ("attempts", Some(num(*attempts as f64))),
                    ]));
                    b.unrenamed_names[3].push(name.clone());
                    bump(&mut b.by_attempts, attempts);
                }
            }
        }
    }
    b
}

/// Rows recorded into the run's trail by a pass over ANOTHER text than
/// the four anchored ones (the post-split reconcile, per split file).
#[derive(Clone, Debug, Default)]
pub struct ExtraText {
    /// The split file's tree-relative path — the rows' text label (07 §1's
    /// path key space; finding #50: these rows used to say "generated").
    pub file: String,
    /// The text the rows' spans index (the file as the pass parsed it).
    pub text: String,
    /// The rows, in record order.
    pub rows: Vec<TrailEntry>,
}

/// Every extra text's rows, in record order.
pub type ExtraTrail = [ExtraText];

/// The strategy trail as `StrategyTrailReport` (`trails` + `funnel`): the
/// run's rows, then the extra rows (the TS's one trail keyed by binding
/// NODE: a later pass's rows are new entries, appended), each converted in
/// its own text.
pub fn trail_report(trail: &StrategyTrail, texts: &AnchorTexts<'_>, extra: &ExtraTrail) -> JsValue {
    let anchors = [
        Anchor::Fresh,
        Anchor::Generated,
        Anchor::Reconciled,
        Anchor::Shipped,
    ];
    let mut maps: Vec<(Anchor, Utf16Map, BabelLines<'_>)> = Vec::new();
    for anchor in anchors {
        let text = texts.text(anchor);
        let positions =
            entry_positions(trail.entries().iter().filter(|e| e.target.anchor == anchor));
        if positions.is_empty() {
            continue;
        }
        maps.push((
            anchor,
            Utf16Map::build(text, positions),
            BabelLines::new(text),
        ));
    }
    let mut trails: Vec<JsValue> = trail
        .entries()
        .iter()
        .map(|e| {
            let (_, map, lines) = maps
                .iter()
                .find(|(a, _, _)| *a == e.target.anchor)
                .expect("an anchored text for every row");
            trail_entry(e, e.target.anchor.as_str(), map, lines)
        })
        .collect();
    for x in extra {
        let map = Utf16Map::build(&x.text, entry_positions(x.rows.iter()));
        let lines = BabelLines::new(&x.text);
        trails.extend(x.rows.iter().map(|e| trail_entry(e, &x.file, &map, &lines)));
    }
    let mut funnel = JsObject::new();
    for (tier, outcomes) in funnel_of(all_entries(trail, extra)) {
        let mut o = JsObject::new();
        for (outcome, n) in outcomes {
            o.insert(outcome.as_str(), num(n as f64));
        }
        funnel.insert(tier.as_str(), JsValue::Object(o));
    }
    obj(vec![
        ("trails", Some(JsValue::Array(trails))),
        ("funnel", Some(JsValue::Object(funnel))),
    ])
}

/// Every position a row's report reads (declaration + scope blocks).
fn entry_positions<'e>(entries: impl Iterator<Item = &'e TrailEntry>) -> Vec<u32> {
    let mut positions: Vec<u32> = Vec::new();
    for e in entries {
        positions.push(e.target.decl_span.start);
        positions.push(e.target.decl_span.end);
        for a in &e.attempts {
            if let Some(s) = a.scope_block {
                positions.push(s.start);
                positions.push(s.end);
            }
        }
    }
    positions
}

/// The run's rows then the extra rows, in record order.
fn all_entries<'e>(
    trail: &'e StrategyTrail,
    extra: &'e ExtraTrail,
) -> impl Iterator<Item = &'e TrailEntry> + Clone {
    trail
        .entries()
        .iter()
        .chain(extra.iter().flat_map(|x| x.rows.iter()))
}

/// `report().funnel`: per tier (first-seen order), per outcome (first-seen
/// order), the attempt count.
fn funnel_of<'e>(
    entries: impl Iterator<Item = &'e TrailEntry>,
) -> Vec<(crate::trail::Tier, Vec<(crate::trail::Outcome, u64)>)> {
    let mut funnel: Vec<(crate::trail::Tier, Vec<(crate::trail::Outcome, u64)>)> = Vec::new();
    for attempt in entries.flat_map(|e| &e.attempts) {
        let at = match funnel.iter().position(|(t, _)| *t == attempt.tier) {
            Some(i) => i,
            None => {
                funnel.push((attempt.tier, Vec::new()));
                funnel.len() - 1
            }
        };
        let row = &mut funnel[at].1;
        match row.iter_mut().find(|(o, _)| *o == attempt.outcome) {
            Some((_, n)) => *n += 1,
            None => row.push((attempt.outcome, 1)),
        }
    }
    funnel
}

/// One diag.json trail row; `text` labels the text its spans index (an
/// anchor, or a split file's path).
fn trail_entry(e: &TrailEntry, text: &str, map: &Utf16Map, lines: &BabelLines<'_>) -> JsValue {
    let (line, col) = lines.loc(e.target.decl_span.start);
    let attempts: Vec<JsValue> = e
        .attempts
        .iter()
        .map(|a| {
            obj(vec![
                ("strategy", Some(JsValue::str(a.tier.as_str()))),
                ("outcome", Some(JsValue::str(a.outcome.as_str()))),
                ("reason", a.reason.as_deref().map(JsValue::str)),
                ("newName", a.proposed_name.as_deref().map(JsValue::str)),
                ("refCount", a.ref_count.map(num)),
                (
                    "scopeBlock",
                    a.scope_block
                        .map(|s| JsValue::str(format!("{}:{}", map.get(s.start), map.get(s.end)))),
                ),
            ])
        })
        .collect();
    let mut v = vec![
        ("oldName", Some(JsValue::str(&e.old_name))),
        ("loc", Some(JsValue::str(format!("{line}:{col}")))),
        (
            "declSpan",
            Some(obj(vec![
                ("start", Some(num(map.get(e.target.decl_span.start)))),
                ("end", Some(num(map.get(e.target.decl_span.end)))),
            ])),
        ),
        ("declText", Some(JsValue::str(text))),
        ("trail", Some(JsValue::Array(attempts))),
        ("postSettleAttempts", Some(num(e.post_settle_attempts))),
        ("postSettleVotes", Some(num(e.post_settle_votes))),
    ];
    if let Some(s) = e.settled_by {
        v.push(("settledBy", Some(JsValue::str(s.as_str()))));
    }
    if let Some(t) = e.terminal_by {
        v.push(("terminalBy", Some(JsValue::str(t.as_str()))));
        v.push(("finalName", e.final_name.as_deref().map(JsValue::str)));
    }
    obj(v)
}

/// `creditPools` + `buildTerminalState` + `buildIdentifierLedger`.
fn identifier_ledger<'e>(
    coverage: &CoverageSummary,
    entries: impl Iterator<Item = &'e TrailEntry> + Clone,
    renamed: &[String],
    unrenamed: &[String],
) -> JsValue {
    let mut transfer_settled: Vec<(String, u64)> = Vec::new();
    let mut named_by_tier: Vec<(String, u64)> = Vec::new();
    let mut credits: Vec<(String, u64)> = Vec::new();
    for e in entries.clone() {
        if let Some(s) = e.settled_by {
            bump(&mut transfer_settled, &s.as_str().to_string());
        }
        if let Some(t) = e.terminal_by.or(e.settled_by) {
            bump(&mut named_by_tier, &t.as_str().to_string());
        }
    }
    let mut credit_map: HashMap<String, u64> = HashMap::new();
    for e in entries {
        let last = e
            .attempts
            .iter()
            .rev()
            .find(|a| a.outcome == crate::trail::Outcome::Applied);
        match last
            .and_then(|a| a.proposed_name.as_deref())
            .filter(|n| !n.is_empty())
        {
            Some(n) => *credit_map.entry(n.to_string()).or_default() += 1,
            None if !e.attempts.is_empty() => {
                *credit_map.entry(e.old_name.clone()).or_default() += 1;
            }
            None => {}
        }
    }
    for n in renamed.iter().chain(unrenamed) {
        *credit_map.entry(n.clone()).or_default() += 1;
    }
    credits.clear();
    let census = coverage.minted_census.as_ref();
    let mut accounted = 0u64;
    let mut unaccounted: Vec<String> = Vec::new();
    for name in census.and_then(|c| c.names.clone()).unwrap_or_default() {
        match credit_map.get_mut(&name) {
            Some(n) if *n > 0 => {
                *n -= 1;
                accounted += 1;
            }
            _ => unaccounted.push(name),
        }
    }
    let counts = |m: &[(String, u64)]| {
        let mut o = JsObject::new();
        for (k, n) in m {
            o.insert(k.clone(), num(*n as f64));
        }
        JsValue::Object(o)
    };
    let total = census.and_then(|c| c.total_bindings).map(num);
    let terminal = obj(vec![
        ("totalBindings", total.clone()),
        ("namedByTier", Some(counts(&named_by_tier))),
        ("llmNamed", Some(num(coverage.identifiers.llm))),
        ("mintedAccounted", Some(num(accounted as f64))),
        ("mintedUnaccounted", Some(JsValue::str_array(&unaccounted))),
    ]);
    obj(vec![
        ("totalBindings", total),
        ("transferSettled", Some(counts(&transfer_settled))),
        ("llmNamed", Some(num(coverage.identifiers.llm))),
        (
            "libraryPrefix",
            Some(num(coverage.identifiers.library_prefix)),
        ),
        ("fallback", Some(num(coverage.identifiers.fallback))),
        ("notRenamed", Some(num(coverage.identifiers.not_renamed))),
        ("remainingMinted", census.map(|c| num(c.total))),
        ("terminalState", Some(terminal)),
    ])
}

fn patterns(reports: &[RenameReport], b: &Buckets) -> JsValue {
    let mut targets = b.collision_targets.clone();
    targets.sort_by_key(|x| std::cmp::Reverse(x.1));
    let top: Vec<JsValue> = targets
        .iter()
        .take(20)
        .map(|(n, c)| {
            obj(vec![
                ("name", Some(JsValue::str(n))),
                ("count", Some(num(*c as f64))),
            ])
        })
        .collect();
    let mut lowest: Vec<(f64, JsValue)> = reports
        .iter()
        .filter(|r| r.total_identifiers > 0)
        .map(|r| {
            let pct = math_round(r.renamed_count as f64 / r.total_identifiers as f64 * 100.0);
            (
                pct,
                obj(vec![
                    ("functionId", Some(JsValue::str(&r.target_id))),
                    ("total", Some(num(r.total_identifiers as f64))),
                    ("renamed", Some(num(r.renamed_count as f64))),
                    ("pct", Some(num(pct))),
                ]),
            )
        })
        .collect();
    lowest.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut attempts = JsObject::new();
    for (a, n) in &b.by_attempts {
        attempts.insert(a.to_string(), num(*n as f64));
    }
    let mut finish = JsObject::new();
    for (r, n) in &b.missing_by_finish {
        finish.insert(r.clone(), num(*n as f64));
    }
    obj(vec![
        ("topCollisionTargets", Some(JsValue::Array(top))),
        (
            "unchangedIdentifiers",
            Some(JsValue::str_array(&b.unchanged_names)),
        ),
        (
            "lowestCoverageFunctions",
            Some(JsValue::Array(
                lowest.into_iter().take(20).map(|(_, v)| v).collect(),
            )),
        ),
        ("failuresByAttempts", Some(JsValue::Object(attempts))),
        ("missingByFinishReason", Some(JsValue::Object(finish))),
    ])
}

/// What `buildDiagnosticsReport` reads.
pub struct DiagnosticsInputs<'a, 't> {
    pub timestamp: String,
    pub reports: &'a [RenameReport],
    pub coverage: &'a CoverageSummary,
    pub transfer_stats: Option<&'a TransferStatsByTier>,
    pub trail: &'a StrategyTrail,
    pub texts: AnchorTexts<'t>,
    pub contention: &'a [ContentionEvent],
    /// The post-split reconcile's rows (per split file), appended.
    pub extra_trail: &'a ExtraTrail,
}

/// `buildDiagnosticsReport` (without `placementTrails`, the split's).
pub fn build_diagnostics_report(inp: &DiagnosticsInputs<'_, '_>) -> JsValue {
    let b = classify(inp.reports);
    let unrenamed_names: Vec<String> = b.unrenamed_names.iter().flatten().cloned().collect();
    let events: Vec<JsValue> = inp
        .contention
        .iter()
        .map(|e| {
            obj(vec![
                ("requested", Some(JsValue::str(&e.requested))),
                ("resolvedTo", Some(JsValue::str(&e.resolved_to))),
                ("oldName", Some(JsValue::str(&e.old_name))),
                ("site", Some(JsValue::str(e.site))),
            ])
        })
        .collect();
    let ledger = identifier_ledger(
        inp.coverage,
        all_entries(inp.trail, inp.extra_trail),
        &b.renamed_names,
        &unrenamed_names,
    );
    let pats = patterns(inp.reports, &b);
    obj(vec![
        ("timestamp", Some(JsValue::str(&inp.timestamp))),
        ("coverage", Some(inp.coverage.to_js())),
        ("transferStats", inp.transfer_stats.map(JsType::to_js)),
        (
            "unrenamed",
            Some(obj(vec![
                ("unchanged", Some(JsValue::Array(b.unchanged))),
                ("missing", Some(JsValue::Array(b.missing))),
                ("duplicate", Some(JsValue::Array(b.duplicate))),
                ("invalid", Some(JsValue::Array(b.invalid))),
            ])),
        ),
        ("renamed", Some(JsValue::Array(b.renamed))),
        (
            "strategyTrails",
            Some(trail_report(inp.trail, &inp.texts, inp.extra_trail)),
        ),
        (
            "nameContention",
            Some(obj(vec![("events", Some(JsValue::Array(events)))])),
        ),
        ("identifierLedger", Some(ledger)),
        ("patterns", Some(pats)),
    ])
}
