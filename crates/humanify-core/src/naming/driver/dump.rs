//! The naming stage's rows in the `--dump-artifacts` catalog: the names
//! table (names.json), the dispatch rows (prompts.jsonl +
//! cache-keys.jsonl) and the wave-boundary names.

use humanify_model::dump::SpanKey;
use humanify_model::llm::CacheKeyParams;
use serde_json::{Value, json};

use super::library::RecordedName;
use crate::babel_view::BabelLines;
use crate::naming::passes::sweep::SweepDispatch;
use crate::naming::report::diagnostics::AnchorTexts;
use crate::naming::waves::processor::{DispatchRecord, NameRecord};
use crate::trail::{Anchor, StrategyTrail};

/// A trail row recorded over a text other than the four anchored ones (the
/// post-split reconcile's, per split file), keyed as the TS dump keys it:
/// its label's text (`generated`) converting its RAW offsets — which index
/// the split file — as if they indexed that text (finding #50).
pub struct ExtraNameRow<'e> {
    pub key: SpanKey,
    /// `line:col` in the row's own text.
    pub loc: String,
    pub entry: &'e crate::trail::TrailEntry,
}

/// `writeNames`: the trail rows (every anchored text; functionId =
/// `line:col (tier)` in the row's own text) merged with the recorded rows
/// (LLM applies, uniquify, identity, library prefix) — the recorded row
/// wins on a span collision; sorted by span key.
pub fn names_table(
    trail: &StrategyTrail,
    waves: &[NameRecord],
    library: &[RecordedName],
    texts: &AnchorTexts<'_>,
    extra: &[ExtraNameRow<'_>],
) -> Vec<Value> {
    let lines: Vec<(Anchor, BabelLines<'_>)> = [
        Anchor::Fresh,
        Anchor::Generated,
        Anchor::Reconciled,
        Anchor::Shipped,
    ]
    .into_iter()
    .map(|a| {
        let text = match a {
            Anchor::Fresh => Some(texts.fresh),
            Anchor::Generated => texts.generated,
            Anchor::Reconciled => texts.reconciled,
            Anchor::Shipped => texts.shipped,
        };
        (a, BabelLines::new(text.unwrap_or(texts.fresh)))
    })
    .collect();
    let mut rows: Vec<(SpanKey, Value)> = Vec::new();
    let mut index: std::collections::HashMap<(String, i64, i64), usize> =
        std::collections::HashMap::new();
    let mut put = |key: SpanKey, row: Value| {
        let k = (key.text.clone(), key.start, key.end);
        match index.get(&k) {
            Some(&i) => rows[i].1 = row,
            None => {
                index.insert(k, rows.len());
                rows.push((key, row));
            }
        }
    };
    for e in trail.entries() {
        let Some(final_name) = &e.final_name else {
            continue;
        };
        let lines = &lines
            .iter()
            .find(|(a, _)| *a == e.target.anchor)
            .expect("anchor")
            .1;
        let (line, col) = lines.loc(e.target.decl_span.start);
        let tier = e
            .terminal_by
            .or(e.settled_by)
            .map(|t| t.as_str())
            .unwrap_or("?");
        let key = SpanKey {
            text: e.target.anchor.as_str().to_string(),
            start: i64::from(e.target.decl_span.start),
            end: i64::from(e.target.decl_span.end),
        };
        let row = json!({
            "target": key_json(&key),
            "oldName": e.old_name,
            "newName": final_name,
            "kind": "function",
            "classified": "renamed",
            "functionId": format!("{line}:{col} ({tier})"),
        });
        put(key, row);
    }
    for x in extra {
        let e = x.entry;
        let Some(final_name) = &e.final_name else {
            continue;
        };
        let tier = e
            .terminal_by
            .or(e.settled_by)
            .map(|t| t.as_str())
            .unwrap_or("?");
        let row = json!({
            "target": key_json(&x.key),
            "oldName": e.old_name,
            "newName": final_name,
            "kind": "function",
            "classified": "renamed",
            "functionId": format!("{} ({tier})", x.loc),
        });
        put(x.key.clone(), row);
    }
    let recorded = waves
        .iter()
        .map(|r| RecordedName {
            span: Some(r.span),
            old_name: r.old_name.clone(),
            new_name: Some(r.new_name.clone()),
            module: r.module,
            classified: "renamed",
            function_id: r.function_id.clone(),
        })
        .chain(library.iter().cloned());
    for r in recorded {
        let key = SpanKey {
            text: "fresh".to_string(),
            start: r.span.map_or(-1, |s| i64::from(s.start)),
            end: r.span.map_or(-1, |s| i64::from(s.end)),
        };
        let row = json!({
            "target": key_json(&key),
            "oldName": r.old_name,
            "newName": r.new_name,
            "kind": if r.module { "module-binding" } else { "function" },
            "classified": r.classified,
            "functionId": r.function_id,
        });
        put(key, row);
    }
    rows.sort_by(|a, b| {
        (a.0.text.as_str(), a.0.start, a.0.end).cmp(&(b.0.text.as_str(), b.0.start, b.0.end))
    });
    rows.into_iter().map(|(_, v)| v).collect()
}

fn key_json(k: &SpanKey) -> Value {
    json!({"text": k.text, "start": k.start, "end": k.end})
}

/// prompts.jsonl + cache-keys.jsonl: the waves' dispatches (`site:
/// "naming"`) then the sweep's (`site: "sweep"`, rounds counted per
/// functionId from 1), seq in that order — the artifact dump's owner
/// ([`crate::artifact_dump::dispatch_rows`]) over the naming sites only.
pub fn dispatch_rows(
    waves: &[DispatchRecord],
    sweeps: &[(Anchor, &SweepDispatch)],
    params: &CacheKeyParams,
) -> (String, String) {
    use crate::artifact_dump::Dispatch;
    let dispatches: Vec<Dispatch<'_>> = waves
        .iter()
        .map(Dispatch::Naming)
        .chain(sweeps.iter().map(|(a, d)| Dispatch::Sweep(*a, d)))
        .collect();
    crate::artifact_dump::dispatch_rows(&dispatches, params)
}
