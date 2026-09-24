//! The statement-twin machinery's cascade-INDEPENDENT subset (WP2.3) — TS
//! original: `src/prior-version/statement-twin.ts`.
//!
//! A statement twin = a pair of top-level wrapper-body statements (one from
//! the prior humanified bundle, one from the fresh minified bundle) whose
//! statementHash matches; every binding inside the fresh statement can then
//! inherit its prior name. The full machinery runs INSIDE matchPriorVersion
//! (prior-version.ts :402-421), downstream of the cascade; THIS module ports
//! only the cascade-independent part:
//!
//! - the two statement inventories (`topLevelStatements` :151 +
//!   `buildSideInventory` :227, with `hashCounts`/`uniqueIndex` :239-242),
//!   including the graph rows' enclosing-statement assignment
//!   (`assignToStatements` :209) that the later gates read;
//! - the UNIQUE-tier proposal set — the 1:1 hash join (:1135-1150,
//!   `stats.uniqueTwins` :1140): pairs where the hash is count-1 on BOTH
//!   sides.
//!
//! NOT here (their dependencies are later work packages): the module-scoped
//! tier (exp073; needs split/fossil-map.ts — WP5.2) and the bucket tier
//! (needs the cascade's fnMatches). The proposal GATES (candidacy, callee
//! identity, binding roles, the placeholder-walk structural check, the
//! per-slot owner gate — statement-twin.ts :18-36) all read the cascade's
//! results; they join this module's consumers, not its scope.
//!
//! Parity posture (WP1.4's gate): the statementHash DIGEST BYTES differ by
//! design (oxc's ESTree field order vs babel's VISITOR_KEYS order). The
//! equivalence CLASSES transfer: for any two trees, byte-equality of a
//! deterministic-order serialization implies byte-equality under any other
//! fixed order (the reordering is applied identically to both sides), so
//! the partition — and therefore the unique-tier join — is order-independent.
//! The probe's frozen expectations (test/parity/wp23-probe.mjs →
//! wp23-unique-twin-index.json) assert exactly that: counts, the
//! bucket-size histogram, and sample-pair SPANS, never digest strings.
//!
//! Anchors: the fresh side is the PRE-rename minified text (the graph the
//! cascade runs on); the prior side is the from-version's humanified
//! output. `statement_inventory`'s `anchor` parameter names which text the
//! inventory was taken on — the dump's member key.

use std::collections::HashMap;

use oxc_span::Span;
use serde_json::Value;

pub mod fossil;
pub mod gates;
pub mod role;

use crate::graph::UnifiedGraph;
use crate::hash::statement_hash::statement_hash;
use crate::ingest::Ingest;
use crate::modules::wrapper::find_wrapper_function;

/// The fresh side's anchor: the pre-rename minified text.
pub const FRESH_ANCHOR: &str = "fresh";
/// The prior side's anchor: the from-version's humanified output.
pub const PRIOR_ANCHOR: &str = "prior";

/// One top-level statement of an inventory (TS: the statement path's node +
/// its `statementHash`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRecord {
    /// The statement's span (UTF-8 byte offsets — oxc spans need no
    /// conversion; the TS probe converts babel's to bytes before freezing).
    pub span: Span,
    /// `statementHash` (split/statement-hash.ts :90) — the masked,
    /// rename-invariant content key.
    pub hash: String,
}

/// One side's statement inventory (TS `SideInventory` :167-175). The hash
/// bookkeeping is what the unique tier reads; the row assignment is what
/// the proposal gates read downstream.
#[derive(Debug, Default)]
pub struct SideInventory {
    /// Which text the inventory was taken on (`fresh` / `prior`).
    pub anchor: &'static str,
    /// The wrapper-body statements in bundle order.
    pub statements: Vec<StatementRecord>,
    /// hash → occurrence count (TS `hashCounts` :237-238).
    pub hash_counts: HashMap<String, u32>,
    /// hash → statement index, only for count-1 hashes (TS `uniqueIndex`
    /// :239-242).
    pub unique_index: HashMap<String, usize>,
    /// Statement index → graph function-row indices, in row order (TS
    /// `fnsByStatement`; `assignToStatements` :209 over `fn.path`).
    pub fns_by_statement: HashMap<usize, Vec<usize>>,
    /// Statement index → graph module-binding-row indices, in row order (TS
    /// `bindingsByStatement` over the binding's declaration path).
    pub bindings_by_statement: HashMap<usize, Vec<usize>>,
    /// Rows whose enclosing statement is none of this side's (TS
    /// `assignToStatements` skips them: the wrapper itself, and rows
    /// declared outside the container when one exists).
    pub unassigned_functions: usize,
    pub unassigned_bindings: usize,
}

impl SideInventory {
    /// The statement spans, in bundle order (the assignment's join key).
    fn spans(&self) -> Vec<Span> {
        self.statements.iter().map(|s| s.span).collect()
    }

    /// Rows on a statement, totalled (the counts' assigned half).
    pub fn assigned_functions_total(&self) -> usize {
        self.fns_by_statement.values().map(Vec::len).sum()
    }

    /// Rows on a statement, totalled (the counts' assigned half).
    pub fn assigned_bindings_total(&self) -> usize {
        self.bindings_by_statement.values().map(Vec::len).sum()
    }
}

/// The statement inventory of one bundle text (TS `buildSideInventory`).
///
/// `graph` is the side's own unified graph (fresh: the graph the cascade
/// runs on; prior: prior-version.ts :284's build). When given, the rows are
/// assigned to their enclosing statements; the unique-tier join needs only
/// the hash bookkeeping and may pass `None` (e.g. a probe gate on the big
/// bundles, where the row assignment is verified on the fixtures).
///
/// Statement SELECTION is the TS `topLevelStatements` semantics: the
/// wrapper body when the wrapper gate passes (`graph.wrapperPath` —
/// modules::wrapper's 50-binding threshold is that gate's owner), else the
/// program body. The wrapper's body block is located in the ESTree
/// serialization BY SPAN (the gate's `body_span`), so no second
/// wrapper-form walk exists to fall out of date with the owner.
pub fn statement_inventory(
    text: &str,
    anchor: &'static str,
    graph: Option<&UnifiedGraph>,
) -> Result<SideInventory, String> {
    statement_inventory_inner(text, anchor, graph, false).map(|(inv, _)| inv)
}

/// [`statement_inventory`] plus the statements' ESTree JSON subtrees — the
/// structural gate's serialization substrate (the gate re-walks the exact
/// subtree whose hash the inventory computed; a second parse would give a
/// second parse's JSON, one more thing to fall out of date with the
/// inventory's own). The values are index-aligned with
/// `inventory.statements`. The plain entry point drops them (the probe /
/// dump path never needs the megabytes).
pub fn statement_inventory_with_values(
    text: &str,
    anchor: &'static str,
    graph: Option<&UnifiedGraph>,
) -> Result<(SideInventory, Vec<Value>), String> {
    statement_inventory_inner(text, anchor, graph, true)
}

fn statement_inventory_inner(
    text: &str,
    anchor: &'static str,
    graph: Option<&UnifiedGraph>,
    retain_values: bool,
) -> Result<(SideInventory, Vec<Value>), String> {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, text, anchor);
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse the {anchor} text: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }

    // The wrapper gate (TS `graph.wrapperPath`): presence decides between
    // the wrapper body and the program body.
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic());

    // The ESTree substrate per statement: serialize the PROGRAM once (one
    // JSON, the partition dump's own settings), then take each side's
    // statement list out of it.
    let estree = ingest.program.to_estree_json(false, true);
    let program_json = parse_unbounded(&estree)?;
    statement_inventory_from_json(
        &program_json,
        wrapper.map(|w| w.body_span),
        anchor,
        graph,
        retain_values,
    )
}

/// The inventory over a side's already-parsed program JSON
/// ([`crate::ingest::program_estree_json`]) and its wrapper gate's body
/// span — the dump shares one program JSON per side across every
/// consumer. The per-statement hashes (and the retained values' copies)
/// are pure per-statement maps and run on the pool, in statement order.
pub fn statement_inventory_from_json(
    program_json: &Value,
    wrapper_body: Option<Span>,
    anchor: &'static str,
    graph: Option<&UnifiedGraph>,
    retain_values: bool,
) -> Result<(SideInventory, Vec<Value>), String> {
    let statement_values: Vec<&Value> = match wrapper_body {
        Some(body_span) => {
            let body = block_body_by_span(program_json, body_span).ok_or_else(|| {
                format!(
                    "wrapper detected (body span {}..{}) but that block is absent from the estree json",
                    body_span.start, body_span.end
                )
            })?;
            body.iter().collect()
        }
        None => program_json
            .get("body")
            .and_then(Value::as_array)
            .ok_or("estree json program body missing")?
            .iter()
            .collect(),
    };

    let mut inventory = SideInventory {
        anchor,
        ..SideInventory::default()
    };
    let hashes = crate::par::map_ordered(&statement_values, |stmt| statement_hash(stmt));
    for (stmt, hash) in statement_values.iter().zip(hashes) {
        let start = stmt
            .get("start")
            .and_then(Value::as_u64)
            .ok_or("statement start missing from the estree json")?;
        let end = stmt
            .get("end")
            .and_then(Value::as_u64)
            .ok_or("statement end missing from the estree json")?;
        inventory.statements.push(StatementRecord {
            span: Span::new(start as u32, end as u32),
            hash,
        });
    }
    let values: Vec<Value> = if retain_values {
        crate::par::map_ordered(&statement_values, |stmt| (*stmt).clone())
    } else {
        Vec::new()
    };

    // TS :237-242 — counts, then the count-1 index.
    for record in &inventory.statements {
        *inventory
            .hash_counts
            .entry(record.hash.clone())
            .or_default() += 1;
    }
    for (i, record) in inventory.statements.iter().enumerate() {
        if inventory.hash_counts.get(&record.hash) == Some(&1) {
            inventory.unique_index.insert(record.hash.clone(), i);
        }
    }

    // The graph rows' enclosing-statement assignment (TS
    // `assignToStatements` :209-260). Span containment is the port's
    // equivalent of the babel parent-chain walk: statements are
    // non-overlapping siblings, so exactly one can contain a row's span —
    // and a row whose span IS a statement's (a top-level function
    // declaration) assigns to it, as the TS walk (ancestor-or-self) does.
    if let Some(graph) = graph {
        let spans = inventory.spans();
        for (i, f) in graph.functions.iter().enumerate() {
            match statement_containing(&spans, f.span) {
                Some(idx) => inventory.fns_by_statement.entry(idx).or_default().push(i),
                None => inventory.unassigned_functions += 1,
            }
        }
        for (j, b) in graph.module_bindings.iter().enumerate() {
            match statement_containing(&spans, b.span) {
                Some(idx) => inventory
                    .bindings_by_statement
                    .entry(idx)
                    .or_default()
                    .push(j),
                None => inventory.unassigned_bindings += 1,
            }
        }
    }

    Ok((inventory, values))
}

/// The unique-tier proposal set (TS :1135-1150): fresh statements whose
/// hash is count-1 on the fresh side AND present in the prior side's
/// count-1 index, in fresh bundle order. `unique_twins` is TS
/// `stats.uniqueTwins` — incremented for every joined pair before the
/// (out-of-scope) gates, so it equals `pairs.len()` by construction.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UniqueTwinProposals {
    pub unique_twins: usize,
    /// (fresh statement index, prior statement index), fresh order.
    pub pairs: Vec<(usize, usize)>,
}

/// The 1:1 hash join between the two inventories.
pub fn unique_twin_proposals(prior: &SideInventory, fresh: &SideInventory) -> UniqueTwinProposals {
    let mut out = UniqueTwinProposals::default();
    for (i, record) in fresh.statements.iter().enumerate() {
        if fresh.hash_counts.get(&record.hash) != Some(&1) {
            continue;
        }
        let Some(&prior_idx) = prior.unique_index.get(&record.hash) else {
            continue;
        };
        out.unique_twins += 1;
        out.pairs.push((i, prior_idx));
    }
    out
}

/// The ESTree JSON's parse: the AST nests hundreds deep, past serde_json's
/// default recursion limit — unbounded depth is safe (the input is oxc's
/// own serialization of a program that parsed; same as the partition dump).
fn parse_unbounded(json: &str) -> Result<Value, String> {
    let mut de = serde_json::Deserializer::from_str(json);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).map_err(|e| format!("estree json: {e}"))
}

/// The block statement whose span is `target`, as its body array — the
/// wrapper-gate's body block, located in the serialization by the span the
/// gate already computed. Iterative (the AST's depth).
fn block_body_by_span(root: &Value, target: Span) -> Option<&Vec<Value>> {
    let mut stack: Vec<&Value> = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("BlockStatement")
                    && map.get("start").and_then(Value::as_u64) == Some(u64::from(target.start))
                    && map.get("end").and_then(Value::as_u64) == Some(u64::from(target.end))
                {
                    match map.get("body") {
                        Some(body @ Value::Array(_)) => return body.as_array(),
                        _ => return None,
                    }
                }
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    None
}

/// The index of the statement containing `row` — the unique candidate is
/// the last statement starting at or before the row (`partition_point`),
/// since the statements are non-overlapping siblings.
fn statement_containing(spans: &[Span], row: Span) -> Option<usize> {
    let idx = spans.partition_point(|s| s.start <= row.start);
    if idx == 0 {
        return None;
    }
    let candidate = spans[idx - 1];
    (candidate.start <= row.start && row.end <= candidate.end).then_some(idx - 1)
}

/// The WP2.3 gate's dump: rebuild the TS twins.json's rows from a TS dump's
/// two texts — the two inventories + the unique-tier 1:1 join (the
/// cascade-independent subset). Migration scaffolding — deleted at phase 6
/// with the TS core (02 §9).
pub mod twins_dump {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::{statement_inventory, unique_twin_proposals};

    /// One side's inventory, as the dump's scalars (the TS
    /// twinInventorySnapshot's shape).
    fn inventory_json(inv: &super::SideInventory) -> Value {
        let mut histogram: BTreeMap<u32, u32> = BTreeMap::new();
        let mut distinct = 0u32;
        let mut unique = 0u32;
        let mut max_bucket = 0u32;
        // The counts are read as a MULTISET (sums only — order-free).
        #[allow(clippy::iter_over_hash_type)]
        for count in inv.hash_counts.values() {
            distinct += 1;
            max_bucket = max_bucket.max(*count);
            if *count == 1 {
                unique += 1;
            }
            *histogram.entry(*count).or_default() += 1;
        }
        json!({
            "statements": inv.statements.len(),
            "distinctHashes": distinct,
            "uniqueHashes": unique,
            "maxBucket": max_bucket,
            "bucketHistogram": histogram.iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<BTreeMap<String, u32>>()
        })
    }

    pub fn dump_twins(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
        let meta_text = fs::read_to_string(ts_dump_dir.join("meta.json"))
            .map_err(|e| format!("meta.json: {e}"))?;
        let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
        let fresh = fs::read_to_string(ts_dump_dir.join("text").join("fresh.js"))
            .map_err(|e| format!("fresh: {e}"))?;
        let prior = fs::read_to_string(ts_dump_dir.join("text").join("prior.js"))
            .map_err(|e| format!("prior: {e}"))?;

        // The inventories' fresh side = the pre-rename text (the graph the
        // cascade runs on); prior = the from-version's humanified output.
        let fresh_inv = statement_inventory(&fresh, "fresh", None)?;
        let prior_inv = statement_inventory(&prior, "prior", None)?;
        let proposals = unique_twin_proposals(&prior_inv, &fresh_inv);

        let mut pairs: Vec<Value> = proposals
            .pairs
            .iter()
            .map(|(fresh_idx, prior_idx)| {
                let fresh_stmt = &fresh_inv.statements[*fresh_idx];
                let prior_stmt = &prior_inv.statements[*prior_idx];
                json!({
                    "prior": {"text": "prior", "start": prior_stmt.span.start, "end": prior_stmt.span.end},
                    "fresh": {"text": "fresh", "start": fresh_stmt.span.start, "end": fresh_stmt.span.end},
                    "hash": fresh_stmt.hash,
                })
            })
            .collect();
        pairs.sort_by(|a, b| {
            let key = |v: &Value| {
                (
                    v["fresh"]["start"].as_u64().unwrap_or(u64::MAX),
                    v["fresh"]["end"].as_u64().unwrap_or(u64::MAX),
                )
            };
            key(a).cmp(&key(b))
        });

        fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
        fs::write(
            out_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .map_err(|e| format!("write meta: {e}"))?;
        fs::write(
            out_dir.join("twins.json"),
            serde_json::to_string(&json!({
                "schemaVersion": 1,
                "inventories": {
                    "prior": inventory_json(&prior_inv),
                    "fresh": inventory_json(&fresh_inv)
                },
                "uniqueTier": {
                    "uniqueTwins": proposals.unique_twins,
                    "pairs": pairs
                }
            }))
            .unwrap(),
        )
        .map_err(|e| format!("write twins: {e}"))?;
        Ok(proposals.unique_twins)
    }
}
