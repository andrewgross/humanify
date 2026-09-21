//! Statement-level content alignment for close-matched function pairs
//! (WP2.2 part 2) — TS original: `src/prior-version/statement-align.ts`
//! (539 LOC), ported whole. TS `computeBodyLocalTransfers` (:493) is the
//! single entry point, here [`compute_body_local_transfers`].
//!
//! Close matches transfer only function name + params through signature
//! position; their body locals were the largest remaining rename-noise
//! population. This module aligns top-level body statements between the
//! prior and new versions on rename-invariant content and bridges names
//! through per-statement placeholder slots — the same mechanism exact
//! matches use, applied per statement.
//!
//! Precision gates (a wrong transfer is worse than a missed one):
//! - Statements pair only within equal-count same-hash groups (unique
//!   hashes are the size-1 case). An unequal group means an edit landed
//!   inside it, so the whole group is skipped.
//! - Declaration anchor: a function-owned binding transfers only when its
//!   declaration lives inside an aligned statement — its defining content
//!   is provably unchanged. Use-sites alone never carry a name.
//! - Unanimity: conflicting prior names for one identifier drop it.
//! - Bindings owned by nested functions are skipped entirely (the nested
//!   function has its own match); outer bindings pass through as candidate
//!   pairs for vote propagation, which applies its own binding-identity
//!   gate and vote floor.
//!
//! # SUBSTRATE — the hash owner, and why
//!
//! The TS hashes every alignment unit with `hashPathWithMapping`
//! (structural-hash.ts :977 = `hashAndMapPath(path, false)`): binding
//! identifiers slotted, literals BLURRED, property names and free
//! identifiers verbatim. NOT the split's `hash::statement_hash` (that walk
//! masks property names and keeps literals verbatim — a different
//! equivalence relation for a different consumer; the task brief named it,
//! and this module doc is the correction).
//!
//! This module owns its own token walk ([`Tokenizer`], used by
//! [`walk_unit`]) rather than calling `hash::serialize`'s
//! `canonical_serialize`, for two reasons parity testing pinned:
//! - CHILD ORDER is load-bearing. The TS walks `Object.keys(babelNode)` —
//!   babel's field order (`callee` before `arguments`); a serde BTreeMap
//!   walks keys alphabetically. Hash EQUALITY classes survive any fixed
//!   order (a relabeling, applied to both sides), but the slot ORDINALS
//!   are assigned by first occurrence in the walk — they decide the
//!   evidence map's insertion order (the transfer/hint output order the
//!   frozen probe pins) — and the content SHINGLES are k-grams over the
//!   token stream, so the overlap between a changed initializer and its
//!   prior version depends on WHERE the change lands in the stream. Both
//!   observables moved in parity testing (hint order o,m,k vs o,k,m; a
//!   snap verdict 8/17 → 10/20 across the 0.5 floor), so the walk follows
//!   babel's VISITOR_KEYS order ([`BABEL_CHILD_KEYS`]).
//! - oxc's ESTree JSON carries fields babel omits (`optional: false` on
//!   every call/member) and vice versa; the walk skips what babel's nodes
//!   do not carry (pinned by test/parity/wp22-tok-debug.mjs).
//!
//! The literal classes come from `hash::serialize`'s now-`pub(crate)`
//! helpers (`string_literal_token`, `template_element_token`,
//! `numeric_magnitude`, `volatile_literal_token`) — one owner, no copied
//! regexes. `identifier_role` and the bare-position block unwrap remain
//! local copies (KEEP-IN-SYNC with `hash::serialize`).
//!
//! Digests are compared ACROSS sides here (prior hash == fresh hash is
//! the alignment test), so this module leans on the same-walk-same-order
//! property: structurally identical subtrees of the two sides hash equal.
//! Only equality classes matter, not bytes — though with the shared walk
//! the unit hashes actually match the TS's own sha256-16 digests when the
//! streams do.
//!
//! Policies, all through the one walk:
//! - unit hashes and the snap gate's definition hash: BLURRED literals
//!   (`preserveLiterals: false`);
//! - switch-case pairing's test signature (TS `computeStructuralSignature`
//!   :1013 = `hashAndMapPath(path, true)`) — `case "open"` and
//!   `case "data"` blur to the same statement hash, so a reordered case
//!   would cross-pair bodies without a literal-preserving gate;
//! - the content shingles (`computeContentShingles` binding-role.ts :44 =
//!   `serializePathTokens(path, {preserveLiterals: true})`), with slot
//!   ordinals BLINDED after the walk (`$3`→`$`, `L1`→`L`): ordinals are
//!   assigned in walk order, so one inserted declaration inside a large
//!   initializer would renumber every later slot and crater the
//!   similarity of otherwise unchanged content.
//!
//! # SUBSTRATE — babel paths vs oxc
//!
//! The TS walks NodePaths and resolves Babel `Binding`s. The Rust side
//! walks the row's oxc ESTree JSON structurally (no span lookups on the
//! alignment path — the units are reached by field navigation from the
//! row JSON, so an ExpressionStatement and its sole expression, which
//! share a span when no semicolon follows, can never be confused) and
//! resolves oxc `SymbolId`s through `hash::serialize::SymbolTables`
//! (07 §1's span-keyed identity).
//!
//! Positional questions (decl-inside-statement, decl-inside-function,
//! write-inside-statement) are span containment on:
//! - `scoping.symbol_span(symbol)` — the BindingIdentifier span — for
//!   PARAMS (see `decl_span_of`);
//! - `nodes.get_node(scoping.symbol_declaration(symbol)).span()` — the
//!   declaration node's span — otherwise. This mirrors the TS
//!   `binding.path` per kind: the VariableDeclarator for vars, the
//!   FunctionDeclaration/ClassDeclaration for decl-form bindings, the
//!   param identifier for params. The one oxc wrinkle is params:
//!   `symbol_declaration` can answer the FUNCTION node for a param, so
//!   the param case is detected off the declaration identifier's parent
//!   chain (`FormalParameter`) and falls back to the identifier span.
//! - the owning-scope test (TS `binding.scope.getFunctionParent() !==
//!   fn.path.scope`) walks `scope_ancestors` to the first
//!   Function/Arrow/StaticBlock scope and compares that scope's owner
//!   node span with the ROW's function span (the row JSON's `value`
//!   function for method rows, the row span otherwise — babel has ONE
//!   node for a method; oxc nests a Function under the MethodDefinition).
//!
//! JSON values are carried OWNED (cloned) rather than borrowed: the two
//! sides' program JSONs have independent lifetimes, and an alignment pair
//! holds one unit from each. The close tier runs on the unmatched
//! remnant only — a few pairs per version hop — so the clone cost is
//! noise.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::{GetSpan, Span};
use oxc_syntax::reference::ReferenceFlags;
use oxc_syntax::scope::ScopeFlags;
use oxc_syntax::symbol::SymbolId;
use serde_json::Value;

use sha2::{Digest, Sha256};

use crate::hash::serialize::{
    SymbolTables, numeric_magnitude, string_literal_token, template_element_token,
};

use super::SHINGLE_SIMILARITY_FLOOR;
use super::jaccard_similarity;

/// TS `MAX_ALIGN_DEPTH` (:123): recursion budget for descending into
/// changed container statements. Each else-if link of a chain costs one
/// level (if → alternate-if → ...), and real bundles nest
/// transport-style chains inside try blocks 5-6 branches deep — the old
/// budget of 4 stopped exactly there, so locals in the tail branches
/// were never anchored and the LLM re-named them every version hop.
/// Descent only follows type-unique remainders, so a deep budget adds no
/// ambiguity, and each level hashes a strictly smaller subtree.
pub const MAX_ALIGN_DEPTH: usize = 16;

/// Shingle k-gram size (binding-role.ts `SHINGLE_K`).
const SHINGLE_K: usize = 4;
/// Deterministic cap on shingles kept per binding (binding-role.ts
/// `SHINGLE_CAP`, walk order).
const SHINGLE_CAP: usize = 2048;

/// One side's context for a close pair's alignment: the side's semantic
/// tables, its program ESTree JSON (for the snap gate's content lookups),
/// the function ROW's own ESTree JSON (the alignment walk navigates this
/// structurally — see the module doc) and the row/function spans.
pub struct AlignSide<'a, 'j> {
    semantic: &'a Semantic<'a>,
    tables: &'a SymbolTables,
    /// (start, end) → the side's JSON nodes at that span, in the same
    /// pre-order the old full-DFS lookup walked (object keys in serde's
    /// BTreeMap order, arrays in order); the type filter picks the first
    /// matching entry, which reproduces `find_json_by_span` exactly. Built
    /// ONCE per side by the caller ([`build_json_index`]) — a per-pair
    /// build cost 2.5 s × 2 sides × 720 pairs (the close dump's ~2h/pair;
    /// the alignment itself is sub-millisecond).
    json_by_span: &'j JsonSpanIndex<'j>,
    row_json: Value,
    /// The graph row's span (the MethodDefinition for method rows).
    row_span: Span,
    /// The span of the node that owns the row's function scope (the row
    /// itself, or the `value` function for method rows) — the owning-
    /// scope test's comparison target.
    fn_span: Span,
    /// Arena node id by span (keep-first: parents precede children in the
    /// arena walk, so a parent that shares a span with a child — an
    /// ExpressionStatement over a bare semicolon-less expression — wins).
    node_by_span: HashMap<(u32, u32), oxc_semantic::NodeId>,
}

impl<'a, 'j> AlignSide<'a, 'j> {
    /// Build one side. `json_index` is the side's whole-program ESTree
    /// index ([`build_json_index`] over the
    /// `program.to_estree_json(false, true)` output, built ONCE per side);
    /// `row_json` the row's own ESTree JSON
    /// (`matching::features::row_estree_json`, parsed); `row_span` the
    /// graph row's span.
    pub fn build(
        semantic: &'a Semantic<'a>,
        tables: &'a SymbolTables,
        json_index: &'j JsonSpanIndex<'j>,
        row_json: Value,
        row_span: Span,
    ) -> AlignSide<'a, 'j> {
        let node_by_span = semantic
            .nodes()
            .iter()
            .fold(HashMap::new(), |mut acc, node| {
                acc.entry((node.span().start, node.span().end))
                    .or_insert(node.id());
                acc
            });
        let fn_span = row_function_span(&row_json, row_span);
        AlignSide {
            semantic,
            tables,
            json_by_span: json_index,
            row_json,
            row_span,
            fn_span,
            node_by_span,
        }
    }

    /// The graph row's span (the MethodDefinition for method rows).
    pub fn row_span(&self) -> Span {
        self.row_span
    }
}

/// Parse an ESTree JSON string (a `program.to_estree_json` output or a
/// `matching::features::row_estree_json` row) into a `Value`. Unbounded
/// depth: the AST nests hundreds deep, and the input is oxc's own
/// serialization of a program that parsed. COPIED from
/// `matching::statement_context`'s private helper (keep-in-sync; a
/// follow-up can lift it into a shared spot when a third caller appears).
pub fn parse_json_unbounded(text: &str) -> Value {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap_or(Value::Null)
}

/// TS `alignmentUnits`' body indirection for method rows: the row node is
/// a MethodDefinition/ObjectProperty whose function lives under `value`;
/// a plain function row IS the function. Matches
/// `matching::features`' body lookup.
fn row_function_span(row_json: &Value, row_span: Span) -> Span {
    let ty = json_type(row_json);
    if matches!(ty, "MethodDefinition" | "ObjectProperty")
        && let Some(span) = json_span_opt(row_json.get("value"))
    {
        return span;
    }
    row_span
}

// ---------------------------------------------------------------------------
// The alignment walk (statement-align.ts :40-299)
// ---------------------------------------------------------------------------

/// One hashed alignment unit — TS `HashedStatement` (:40). The unit is a
/// statement JSON node (or an arrow's returned expression); `mapping` is
/// the canonical walk's placeholder table in FIRST-OCCURRENCE order (the
/// TS's `mapping` Map insertion order) with the slot's resolved symbol.
#[derive(Debug, Clone)]
struct HashedUnit {
    value: Value,
    /// The unit's span — the statement path for the containment tests.
    span: Span,
    /// The unit's node type (`path.node.type` for the type-unique
    /// remainders).
    type_name: String,
    hash: String,
    /// (slot, symbol, name) in first-occurrence order.
    mapping: Vec<(String, Option<SymbolId>, String)>,
    /// slot string → index into `mapping`.
    slot_index: HashMap<String, usize>,
}

/// One aligned (prior, fresh) unit pair — TS `AlignedPair` (:74).
#[derive(Debug, Clone)]
struct AlignedUnitPair {
    prior: HashedUnit,
    next: HashedUnit,
}

fn json_type(value: &Value) -> &str {
    value.get("type").and_then(Value::as_str).unwrap_or("")
}

fn json_span_opt(value: Option<&Value>) -> Option<Span> {
    let value = value?;
    let start = value.get("start").and_then(Value::as_u64)? as u32;
    let end = value.get("end").and_then(Value::as_u64)? as u32;
    Some(Span::new(start, end))
}

/// The paths a body aligns on (TS `alignmentUnits` :55). An arrow
/// expression body and a block that is exactly `return <expr>;` are the
/// same function written two ways — both normalize to the returned
/// expression, so a style change between versions doesn't zero out the
/// pair's alignment evidence.
fn alignment_units(row_json: &Value) -> Vec<Value> {
    // Method rows carry the function under `value` (see
    // `row_function_span`); plain rows under `body`.
    let body = row_json
        .get("value")
        .and_then(|v| v.get("body"))
        .or_else(|| row_json.get("body"));
    let Some(body) = body else {
        return vec![];
    };
    if json_type(body) != "BlockStatement" {
        return vec![body.clone()]; // expression body
    }
    let Some(statements) = body.get("body").and_then(Value::as_array) else {
        return vec![];
    };
    if statements.len() == 1 && json_type(&statements[0]) == "ReturnStatement" {
        // TS `if (!Array.isArray(argument) && argument.node)` — a bare
        // `return;` (argument null/absent) keeps the statement.
        let argument = statements[0].get("argument");
        if let Some(arg) = argument.filter(|v| !v.is_null()) {
            return vec![arg.clone()];
        }
    }
    statements.to_vec()
}

/// Hash every alignment unit of a function body (TS `hashBodyStatements`
/// :70 / `hashUnits` :200).
fn hash_units(units: Vec<Value>, tables: &SymbolTables) -> Vec<HashedUnit> {
    units
        .into_iter()
        .map(|value| {
            // The unit hash is THIS module's own walk (below) — TS
            // `hashPathWithMapping` = the same token walk the content
            // shingles use, under `preserveLiterals: false`. It used to go
            // through `hash::serialize::canonical_serialize`, whose walk
            // order (serde BTreeMap: `arguments` before `callee`) and
            // null-field skipping diverge from the TS — the order decides
            // slot ordinals, and the ordinals decide both the evidence
            // output order and (cross-side) which k-grams two different
            // contents share.
            let walk = walk_unit(&value, tables, false);
            let span = json_span_opt(Some(&value)).unwrap_or(Span::new(u32::MAX, 0));
            let type_name = json_type(&value).to_string();
            let mut slot_index = HashMap::with_capacity(walk.mapping.len());
            for (i, (slot, _, _)) in walk.mapping.iter().enumerate() {
                slot_index.insert(slot.clone(), i);
            }
            HashedUnit {
                value,
                span,
                type_name,
                hash: sha256_16(walk.parts.join("").as_bytes()),
                mapping: walk.mapping,
                slot_index,
            }
        })
        .collect()
}

/// One walk's outputs — the TS `SerializeState` (`parts` + `mapping`).
struct UnitWalk {
    parts: Vec<String>,
    mapping: Vec<(String, Option<SymbolId>, String)>,
}

/// The token walk over one node ([`Tokenizer`]) — TS
/// `serializePathTokens`/`hashAndMapPath`'s `serializeValue(node, null,
/// "root", state)`.
fn walk_unit(value: &Value, tables: &SymbolTables, keep: bool) -> UnitWalk {
    let mut tokenizer = Tokenizer::new(tables, keep);
    tokenizer.serialize_value(value, None, "");
    UnitWalk {
        parts: tokenizer.parts,
        mapping: tokenizer.mapping,
    }
}

/// The walk's 16-hex digest (TS `createHash("sha256").update(parts.join("")).digest("hex").slice(0, 16)`; the same digest
/// `hash::serialize` computes for its own streams).
fn sha256_16(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(16);
    for byte in &digest[..8] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The unit hash for one node — [`hash_units`]'s per-unit step, also the
/// snap gate's definition hash and the switch-case test signature.
fn unit_hash(value: &Value, tables: &SymbolTables, keep: bool) -> String {
    let walk = walk_unit(value, tables, keep);
    sha256_16(walk.parts.join("").as_bytes())
}

/// Insertion-ordered groups of unit indices by hash (TS `groupByHash`
/// :79 — a Map keyed by hash, so iteration order is first-seen order).
fn group_indices(units: &[HashedUnit]) -> Vec<(&str, Vec<usize>)> {
    let mut order: Vec<(&str, Vec<usize>)> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, unit) in units.iter().enumerate() {
        match index.get(unit.hash.as_str()) {
            Some(&g) => order[g].1.push(i),
            None => {
                index.insert(unit.hash.as_str(), order.len());
                order.push((unit.hash.as_str(), vec![i]));
            }
        }
    }
    order
}

/// Content-aligns statements (TS `alignStatements` :97): same-hash groups
/// with equal counts on both sides pair by ordinal (source order).
/// Unequal counts mean an insertion or removal landed inside the group
/// and every pairing after it would shift — skip the group.
fn align_statements(prior: &[HashedUnit], next: &[HashedUnit]) -> Vec<(usize, usize)> {
    let prior_groups = group_indices(prior);
    let next_groups = group_indices(next);
    let next_by: HashMap<&str, &Vec<usize>> = next_groups.iter().map(|(h, v)| (*h, v)).collect();
    let mut pairs = Vec::new();
    for (hash, prior_list) in prior_groups {
        let Some(next_list) = next_by.get(hash) else {
            continue;
        };
        if next_list.len() != prior_list.len() {
            continue;
        }
        for i in 0..prior_list.len() {
            pairs.push((prior_list[i], next_list[i]));
        }
    }
    pairs
}

/// Insertion-ordered groups of unit indices by TYPE (the `byType` helper
/// inside TS `typeUniquePairs` :133).
fn indices_by_type<'u>(units: &[&'u HashedUnit]) -> Vec<(&'u str, Vec<usize>)> {
    let mut order: Vec<(&'u str, Vec<usize>)> = Vec::new();
    let mut index: HashMap<&'u str, usize> = HashMap::new();
    for (i, unit) in units.iter().enumerate() {
        let ty = unit.type_name.as_str();
        match index.get(ty) {
            Some(&g) => order[g].1.push(i),
            None => {
                index.insert(ty, order.len());
                order.push((ty, vec![i]));
            }
        }
    }
    order
}

/// Unaligned-remainder pairs whose statement node type appears exactly
/// once on each side (TS `typeUniquePairs` :133) — the only pairing that
/// is unambiguous without content evidence. Two changed same-type
/// siblings (e.g. two edited if statements) stay unpaired: positional
/// pairing there would be a guess, and a wrong container pair could
/// align generic same-hash inner statements across unrelated code.
fn type_unique_pairs<'u>(
    rest_prior: &[&'u HashedUnit],
    rest_next: &[&'u HashedUnit],
) -> Vec<(&'u HashedUnit, &'u HashedUnit)> {
    let prior_by_type = indices_by_type(rest_prior);
    let next_by_type = indices_by_type(rest_next);
    let next_single: HashMap<&str, usize> = next_by_type
        .iter()
        .filter(|(_, list)| list.len() == 1)
        .map(|(ty, list)| (*ty, list[0]))
        .collect();
    let mut pairs = Vec::new();
    for (ty, prior_list) in prior_by_type {
        if prior_list.len() == 1
            && let Some(&next_index) = next_single.get(ty)
        {
            // The UNITS, not their rest positions: TS `typeUniquePairs`
            // returns the filtered statement OBJECTS, and the caller
            // descends them directly. Indexing the ORIGINAL vectors with
            // rest positions shifted every descent whose unpaired unit
            // sits after a hash-paired one — the descent entered the
            // already-paired unit, minting phantom aligned pairs and
            // dropping the true pair's evidence (found by the WP2.2
            // close-dump gate, 2026-09-21; probe
            // test/parity/wp22-align-red-probe.mjs).
            pairs.push((rest_prior[prior_list[0]], rest_next[next_index]));
        }
    }
    pairs
}

/// Aligns two unit lists, then descends into unaligned container
/// statements that pair unambiguously by node type (TS
/// `collectAlignedPairs` :165) — an edit nested inside an if/try/loop/
/// switch leaves the container's hash changed while its untouched inner
/// statements still align. A tentative pair that is actually unrelated
/// aligns nothing inside and contributes nothing.
fn collect_aligned_pairs(
    prior: Vec<HashedUnit>,
    next: Vec<HashedUnit>,
    depth: usize,
    prior_tables: &SymbolTables,
    next_tables: &SymbolTables,
) -> Vec<AlignedUnitPair> {
    let index_pairs = align_statements(&prior, &next);
    let mut pairs: Vec<AlignedUnitPair> = index_pairs
        .iter()
        .map(|&(p, n)| AlignedUnitPair {
            prior: prior[p].clone(),
            next: next[n].clone(),
        })
        .collect();
    if depth >= MAX_ALIGN_DEPTH {
        return pairs;
    }

    let aligned_prior: HashSet<usize> = index_pairs.iter().map(|&(p, _)| p).collect();
    let aligned_next: HashSet<usize> = index_pairs.iter().map(|&(_, n)| n).collect();
    let rest_prior: Vec<&HashedUnit> = prior
        .iter()
        .enumerate()
        .filter(|(i, _)| !aligned_prior.contains(i))
        .map(|(_, u)| u)
        .collect();
    let rest_next: Vec<&HashedUnit> = next
        .iter()
        .enumerate()
        .filter(|(i, _)| !aligned_next.contains(i))
        .map(|(_, u)| u)
        .collect();

    for (prior_unit, next_unit) in type_unique_pairs(&rest_prior, &rest_next) {
        let block_pairs = corresponding_blocks(
            &prior_unit.value,
            &next_unit.value,
            prior_tables,
            next_tables,
        );
        for (prior_block, next_block) in block_pairs {
            let prior_children = hash_units(prior_block, prior_tables);
            let next_children = hash_units(next_block, next_tables);
            pairs.extend(collect_aligned_pairs(
                prior_children,
                next_children,
                depth + 1,
                prior_tables,
                next_tables,
            ));
        }
    }
    pairs
}

/// Child statement lists of a block value, or the single statement
/// itself (TS `unitsOf` :194).
fn units_of(value: Option<&Value>) -> Vec<Value> {
    let Some(value) = value else {
        return vec![];
    };
    if value.is_null() {
        return vec![];
    }
    if json_type(value) == "BlockStatement" {
        return value
            .get("body")
            .and_then(Value::as_array)
            .map(|a| a.to_vec())
            .unwrap_or_default();
    }
    vec![value.clone()]
}

/// Walks dotted keys one segment at a time (TS `child` :222):
/// `get("handler.body")` must not throw when `handler` is null (a
/// try/finally with no catch); an array or null hit answers None.
fn child<'j>(value: &'j Value, key: &str) -> Option<&'j Value> {
    let mut current = Some(value);
    for part in key.split('.') {
        current = current
            .and_then(|v| v.get(part))
            .filter(|v| !v.is_null() && !v.is_array());
    }
    current
}

/// The corresponding child-block unit lists of two same-typed container
/// statements (TS `correspondingBlocks` :212): if/else branches,
/// loop bodies, try/catch/finally blocks. Non-container statements yield
/// nothing.
/// The corresponding child-block unit lists of two same-typed container
/// statements (TS `correspondingBlocks` :212): if/else branches,
/// loop bodies, try/catch/finally blocks. Non-container statements yield
/// nothing.
fn corresponding_blocks(
    prior: &Value,
    next: &Value,
    prior_tables: &SymbolTables,
    next_tables: &SymbolTables,
) -> Vec<(Vec<Value>, Vec<Value>)> {
    fn zip(a: Option<&Value>, b: Option<&Value>) -> (Vec<Value>, Vec<Value>) {
        (units_of(a), units_of(b))
    }
    let ptype = json_type(prior);
    let ntype = json_type(next);
    if ptype == "IfStatement" && ntype == "IfStatement" {
        return vec![
            zip(child(prior, "consequent"), child(next, "consequent")),
            zip(child(prior, "alternate"), child(next, "alternate")),
        ];
    }
    if ptype == "TryStatement" && ntype == "TryStatement" {
        return vec![
            zip(child(prior, "block"), child(next, "block")),
            zip(child(prior, "handler.body"), child(next, "handler.body")),
            zip(child(prior, "finalizer"), child(next, "finalizer")),
        ];
    }
    if ptype == "SwitchStatement" && ntype == "SwitchStatement" {
        return switch_case_pairs(prior, next, prior_tables, next_tables);
    }
    if ptype == ntype
        && matches!(
            ptype,
            "ForStatement"
                | "ForOfStatement"
                | "ForInStatement"
                | "WhileStatement"
                | "DoWhileStatement"
                | "BlockStatement"
                | "LabeledStatement"
        )
    {
        // TS :258 — `key = prior.isBlockStatement() ? null : "body"`; the
        // block case zips the blocks themselves (their `body` ARRAYS are
        // the unit lists — `child(block, "body")` would hit an array and
        // answer null).
        if ptype == "BlockStatement" {
            return vec![zip(Some(prior), Some(next))];
        }
        return vec![zip(child(prior, "body"), child(next, "body"))];
    }
    vec![]
}

/// Case-body unit lists of two switch statements (TS `switchCasePairs`
/// :272). Cases pair positionally, gated on an exact-literal test match
/// (or both default) — the length-normalized statement hash would treat
/// `case "open"` and `case "data"` as equal, so a reordered case could
/// cross-pair bodies without the literal-preserving signature. A
/// mismatched position pairs nothing.
fn switch_case_pairs(
    prior: &Value,
    next: &Value,
    prior_tables: &SymbolTables,
    next_tables: &SymbolTables,
) -> Vec<(Vec<Value>, Vec<Value>)> {
    let Some(prior_cases) = prior.get("cases").and_then(Value::as_array) else {
        return vec![];
    };
    let Some(next_cases) = next.get("cases").and_then(Value::as_array) else {
        return vec![];
    };
    if prior_cases.len() != next_cases.len() {
        return vec![];
    }
    let test_signature = |case: &Value, tables: &SymbolTables| -> Option<String> {
        let test = case.get("test")?;
        if test.is_null() {
            return None; // default:
        }
        // TS `computeStructuralSignature` = `hashAndMapPath(path, true)` —
        // the verbatim-literal walk, this module's own.
        Some(unit_hash(test, tables, true))
    };
    let mut pairs = Vec::new();
    for i in 0..prior_cases.len() {
        if test_signature(&prior_cases[i], prior_tables)
            != test_signature(&next_cases[i], next_tables)
        {
            continue;
        }
        let prior_body = prior_cases[i]
            .get("consequent")
            .and_then(Value::as_array)
            .map(|a| a.to_vec())
            .unwrap_or_default();
        let next_body = next_cases[i]
            .get("consequent")
            .and_then(Value::as_array)
            .map(|a| a.to_vec())
            .unwrap_or_default();
        pairs.push((prior_body, next_body));
    }
    pairs
}

// ---------------------------------------------------------------------------
// Occurrence classification (statement-align.ts :301-365)
// ---------------------------------------------------------------------------

/// TS `OccurrenceKind` (:301).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceKind {
    /// Binding owned by THIS function (not a nested one) and declared
    /// inside the aligned statement — safe to auto-transfer AND hint.
    Anchored,
    /// Binding declared outside the function — candidate for vote
    /// propagation downstream (transfer channel only, not an own local).
    Outer,
    /// Use-site of an own-scope binding whose declaration lives elsewhere
    /// in this function — hint only (see the module doc and
    /// `defined_by_aligned_write` for the exception).
    LocalUse,
    /// A nested function's binding, or a param/name that belongs to the
    /// signature transfer — never carried here at all.
    Nested,
}

/// The declaration span the positional tests run on (see the module doc):
/// the param identifier for params (oxc's `symbol_declaration` can answer
/// the FUNCTION for a param — babel's `binding.path` is the identifier),
/// else the declaration node's span (babel's `binding.path` per kind).
fn decl_span_of(side: &AlignSide<'_, '_>, symbol: SymbolId) -> Span {
    let scoping = side.semantic.scoping();
    if is_param_symbol(side, symbol) {
        return scoping.symbol_span(symbol);
    }
    let decl_id = scoping.symbol_declaration(symbol);
    side.semantic.nodes().get_node(decl_id).span()
}

/// Is this symbol a function PARAMETER? Detected off the declaration
/// identifier's parent chain — the walk stops at the owning function
/// (a param sits under a `FormalParameter` before that).
fn is_param_symbol(side: &AlignSide<'_, '_>, symbol: SymbolId) -> bool {
    let scoping = side.semantic.scoping();
    let ident_span = scoping.symbol_span(symbol);
    let Some(&node_id) = side.node_by_span.get(&(ident_span.start, ident_span.end)) else {
        return false;
    };
    let nodes = side.semantic.nodes();
    let mut cur = node_id;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return false;
        }
        match nodes.get_node(parent).kind() {
            AstKind::FormalParameter(_) | AstKind::FormalParameterRest(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => cur = parent,
        }
    }
}

/// TS `binding.scope.getFunctionParent()`'s span: walk the scope chain to
/// the first Function/Arrow/StaticBlock scope and take its owner node's
/// span (babel's `isFunctionParent` alias covers functions, arrows,
/// methods and class static blocks). None at the program root — which the
/// TS reads as `null !== fn.path.scope` → nested.
fn owning_function_span(side: &AlignSide<'_, '_>, symbol: SymbolId) -> Option<Span> {
    let scoping = side.semantic.scoping();
    let mut scope = scoping.symbol_scope_id(symbol);
    loop {
        let flags = scoping.scope_flags(scope);
        if flags.contains(ScopeFlags::Function)
            || flags.contains(ScopeFlags::Arrow)
            || flags.contains(ScopeFlags::ClassStaticBlock)
        {
            let owner = scoping.get_node_id(scope);
            return Some(side.semantic.nodes().get_node(owner).span());
        }
        scope = scoping.scope_parent_id(scope)?;
    }
}

/// The spans of the symbol's WRITE references — the oxc analogue of
/// babel's `binding.constantViolations`: assignments (Write), update
/// expressions (read_write), for-in/of assignment targets. Member-write
/// OBJECTS (`a.b = 1`'s `a`) carry no Write flag, matching babel's
/// `getAssignmentIdentifiers` (a MemberExpression target contributes no
/// binding identifiers). Order is reference registration order — source
/// order for the FIRST write, which is all the content resolution reads.
fn write_spans(side: &AlignSide<'_, '_>, symbol: SymbolId) -> Vec<Span> {
    let scoping = side.semantic.scoping();
    let nodes = side.semantic.nodes();
    scoping
        .get_resolved_reference_ids(symbol)
        .iter()
        .filter_map(|&reference_id| {
            let reference = scoping.get_reference(reference_id);
            if !reference.flags().contains(ReferenceFlags::Write) {
                return None;
            }
            Some(nodes.get_node(reference.node_id()).span())
        })
        .collect()
}

/// Is this binding DEFINED by a write inside the aligned statement? (TS
/// `definedByAlignedWrite` :354.)
///
/// `let x;` declares nothing about content — the assignment that follows
/// is the definition. Anchoring only on the declaration means a statement
/// that declares several locals at once decides all their fates together:
/// the failing case declares 24 locals in one `let`, the next release
/// inserts two, the declarator count changes, that one statement stops
/// aligning, and NONE of the two dozen bindings can auto-transfer even
/// where their own defining assignment is byte-identical modulo the name.
///
/// Requires the SOLE write. A binding assigned in several places has no
/// single defining statement, so one aligned write does not prove its
/// content unchanged — that is the condition the local-use refusal exists
/// for, and it still holds.
fn defined_by_aligned_write(
    side: &AlignSide<'_, '_>,
    symbol: SymbolId,
    statement_span: Span,
) -> bool {
    let scoping = side.semantic.scoping();
    let nodes = side.semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol);
    let AstKind::VariableDeclarator(decl) = nodes.get_node(decl_id).kind() else {
        return false;
    };
    if decl.init.is_some() {
        return false;
    }
    let writes = write_spans(side, symbol);
    if writes.len() != 1 {
        return false;
    }
    let write = writes[0];
    write.start >= statement_span.start && write.end <= statement_span.end
}

/// TS `classifyOccurrence` (:320). See [`OccurrenceKind`].
fn classify_occurrence(
    side: &AlignSide<'_, '_>,
    symbol: SymbolId,
    statement_span: Span,
) -> OccurrenceKind {
    let decl_span = decl_span_of(side, symbol);
    // TS `!declPath.isDescendant(fn.path)`: the declaration is not inside
    // the function. Span equality (the row's own FunctionDeclaration
    // node) is NOT a descendant — babel's isDescendant excludes self.
    // The comparison target is the FUNCTION node (the `value` function
    // for method rows), not the row node — babel has one node for a
    // method, oxc nests.
    let fn_span = side.fn_span;
    let inside_row = decl_span.start >= fn_span.start
        && decl_span.end <= fn_span.end
        && !(decl_span.start == fn_span.start && decl_span.end == fn_span.end);
    if !inside_row {
        return OccurrenceKind::Outer;
    }
    // TS `binding.scope.getFunctionParent() !== fn.path.scope` → nested.
    match owning_function_span(side, symbol) {
        None => return OccurrenceKind::Nested,
        Some(owner) if owner == side.fn_span => {}
        Some(_) => return OccurrenceKind::Nested,
    }
    // TS `declPath === statementPath || declPath.isDescendant(statementPath)`.
    if decl_span.start >= statement_span.start && decl_span.end <= statement_span.end {
        return OccurrenceKind::Anchored;
    }
    if defined_by_aligned_write(side, symbol, statement_span) {
        return OccurrenceKind::Anchored;
    }
    OccurrenceKind::LocalUse
}

// ---------------------------------------------------------------------------
// Evidence + the snap gate (statement-align.ts :367-465)
// ---------------------------------------------------------------------------

/// TS `BindingEvidence` (:367) — keyed by the slot's resolved BINDING
/// (oxc `SymbolId`, stored on the entry); name-keyed evidence collapsed
/// same-named sibling bindings onto one key — their (different) prior
/// names then failed unanimity and ALL of them were dropped. The slot
/// walk already resolved the exact binding, which also covers
/// block-scoped declarations inside aligned container statements that a
/// scope lookup from the statement path cannot see. The Vec keeps the
/// TS Map's insertion order (first occurrence across the pair walk).
struct BindingEvidence {
    /// The new binding this entry is about (the TS Map key).
    binding: SymbolId,
    new_name: String,
    /// Prior names from anchored/outer occurrences — the auto-transfer
    /// set. Insertion-ordered (the TS's Set).
    transfer_prior_names: Vec<String>,
    /// Prior names from anchored/local-use occurrences — the LLM-hint
    /// set.
    hint_prior_names: Vec<String>,
    /// The prior-side symbol for this slot — for the snap content gate.
    prior_symbol: Option<SymbolId>,
}

/// Record one slot pair's evidence (TS `recordSlotEvidence` :389). The
/// transfer and hint prior-name sets are kept separate so a use-site
/// (hint-only) occurrence never pollutes the auto-transfer unanimity
/// check — transfers stay exactly as strict as before this hint channel
/// was added.
fn record_slot_evidence(
    evidence: &mut Vec<BindingEvidence>,
    pair: &AlignedUnitPair,
    slot: &str,
    binding: SymbolId,
    new_name: &str,
    fresh: &AlignSide<'_, '_>,
) {
    let Some(&prior_idx) = pair.prior.slot_index.get(slot) else {
        return;
    };
    let (_, prior_symbol, prior_name) = &pair.prior.mapping[prior_idx];
    // Equal statement hashes guarantee aligned slot sets — same
    // serialization walk, ordinals by first occurrence — so the slot
    // string lookup lands on the matching prior occurrence. A missing or
    // IDENTICAL name carries nothing.
    if prior_name.is_empty() || prior_name == new_name {
        return;
    }
    let prior_symbol = *prior_symbol;
    let kind = classify_occurrence(fresh, binding, pair.next.span);
    if kind == OccurrenceKind::Nested {
        return;
    }

    let entry_index = match evidence.iter().position(|e| e.binding == binding) {
        Some(index) => {
            if evidence[index].prior_symbol.is_none() {
                evidence[index].prior_symbol = prior_symbol;
            }
            index
        }
        None => {
            evidence.push(BindingEvidence {
                binding,
                new_name: new_name.to_string(),
                transfer_prior_names: Vec::new(),
                hint_prior_names: Vec::new(),
                prior_symbol,
            });
            evidence.len() - 1
        }
    };
    let entry = &mut evidence[entry_index];
    if matches!(kind, OccurrenceKind::Anchored | OccurrenceKind::Outer)
        && !entry.transfer_prior_names.iter().any(|n| n == prior_name)
    {
        entry.transfer_prior_names.push(prior_name.clone());
    }
    if matches!(kind, OccurrenceKind::Anchored | OccurrenceKind::LocalUse)
        && !entry.hint_prior_names.iter().any(|n| n == prior_name)
    {
        entry.hint_prior_names.push(prior_name.clone());
    }
}

/// A per-identifier prior-name hint that failed the auto-transfer gate
/// (TS `NameHint` :425).
#[derive(Debug, Clone, PartialEq)]
pub struct NameHint {
    /// Minified name in the NEW version.
    pub new_name: String,
    /// The name its prior-version counterpart carried.
    pub prior_name: String,
    /// True when the new binding's DEFINITION still corroborates its
    /// prior counterpart (rename-identical hash or shingle overlap ≥ the
    /// single-vote floor). Only these may be force-snapped post-LLM (A2)
    /// — a hint whose definition changed is prompt guidance the model may
    /// override, never a forced snap, so a repurposed binding is never
    /// mispinned.
    pub snap_eligible: bool,
}

/// TS `TransferPair` (rename/lifecycle.ts) as this module emits it:
/// `old_name` is the minified name in the NEW version and `new_name` the
/// PRIOR name it inherits — the TS field names, kept verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferPair {
    pub old_name: String,
    pub new_name: String,
    /// The new binding (oxc symbol) the transfer applies to.
    pub binding: Option<SymbolId>,
}

/// TS `BodyAlignment` (:467).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BodyAlignment {
    /// Binding-carried pairs for anchored, per-binding-unanimous names.
    pub transfers: Vec<TransferPair>,
    /// Per-identifier prior-name hints for own-scope locals resolved from
    /// aligned use-sites that did NOT meet the auto-transfer gate.
    /// Superset of `transfers` restricted to this function's own bindings
    /// — LLM prompt material, never applied directly.
    pub hints: Vec<NameHint>,
    /// Content-aligned statement pairs — the pair's corroboration
    /// evidence.
    pub aligned_statements: usize,
    /// Top-level statements in the NEW body (denominator for coverage).
    pub total_new_statements: usize,
}

/// Computes body-local name transfers for a close-matched pair by
/// aligning statements on rename-invariant content and bridging
/// placeholder slots within each aligned pair (TS
/// `computeBodyLocalTransfers` :493). Transfers cover names that are
/// declaration-anchored (or outer-binding candidates) and unanimous
/// across all aligned statements. The aligned-statement count doubles as
/// the pair's content corroboration — a close pair sharing ZERO identical
/// normalized statements is a shape coincidence, and callers must not
/// transfer anything for it.
pub fn compute_body_local_transfers(
    prior: &AlignSide<'_, '_>,
    fresh: &AlignSide<'_, '_>,
) -> BodyAlignment {
    let next_units = hash_units(alignment_units(&fresh.row_json), fresh.tables);
    let prior_units = hash_units(alignment_units(&prior.row_json), prior.tables);
    let total_new_statements = next_units.len();
    let pairs = collect_aligned_pairs(prior_units, next_units, 0, prior.tables, fresh.tables);
    let mut result = BodyAlignment {
        transfers: Vec::new(),
        hints: Vec::new(),
        aligned_statements: pairs.len(),
        total_new_statements,
    };
    if pairs.is_empty() {
        return result;
    }

    let mut evidence: Vec<BindingEvidence> = Vec::new();
    for pair in &pairs {
        // Equal statement hashes guarantee aligned slot sets — same
        // serialization walk, ordinals by first occurrence.
        for (slot, binding, name) in &pair.next.mapping {
            let Some(binding) = binding else {
                continue;
            };
            record_slot_evidence(&mut evidence, pair, slot, *binding, name, fresh);
        }
    }

    for entry in &evidence {
        // Transfers: anchored/outer, unanimous — auto-applied downstream.
        if entry.transfer_prior_names.len() == 1 {
            result.transfers.push(TransferPair {
                old_name: entry.new_name.clone(),
                new_name: entry.transfer_prior_names[0].clone(),
                binding: Some(entry.binding),
            });
        }
        // Hints: own-scope names known (possibly only from use-sites),
        // unanimous.
        if entry.hint_prior_names.len() == 1 {
            result.hints.push(NameHint {
                new_name: entry.new_name.clone(),
                prior_name: entry.hint_prior_names[0].clone(),
                snap_eligible: binding_content_agrees(
                    prior,
                    entry.prior_symbol,
                    fresh,
                    entry.binding,
                ),
            });
        }
    }
    result
}

// ---------------------------------------------------------------------------
// The snap gate's content comparison (binding-role.ts + function-graph.ts)
// ---------------------------------------------------------------------------

/// TS `resolveBindingContentPath` (function-graph.ts :409): the node
/// holding a binding's hashable content — a class declaration's own body,
/// a declarator's init, or — for forward-declared vars — the RHS of the
/// first assignment. `None` when the binding has no content (declared,
/// never initialized or assigned).
fn resolve_binding_content(side: &AlignSide<'_, '_>, symbol: SymbolId) -> Option<Value> {
    let scoping = side.semantic.scoping();
    let nodes = side.semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol);
    let decl_node = nodes.get_node(decl_id);
    let declarator_init = match decl_node.kind() {
        AstKind::Class(class) if class.is_declaration() => {
            return find_json_by_span_index(side, class.span(), Some("ClassDeclaration")).cloned();
        }
        AstKind::VariableDeclarator(decl) => {
            // Navigate the DECLARATOR's JSON structurally, not the init's
            // span: a parenthesized init (`x = (y)`) carries oxc-only
            // ParenthesizedExpression nodes babel does not have, so a
            // span lookup of the unparen'd AST node would miss (babel has
            // no such node; the TS resolves the init PATH directly).
            find_json_by_span_index(side, decl.span(), Some("VariableDeclarator"))
                .and_then(|json| json.get("init"))
                .filter(|v| !v.is_null())
                .map(unparen_json)
                .cloned()
        }
        _ => None,
    };
    if let Some(init) = declarator_init {
        return Some(init);
    }
    // Not a declarator, or a declarator with a NULL init: the TS falls
    // through to the FIRST constant violation, when it is an assignment —
    // its right side is the content.
    let writes = write_spans(side, symbol);
    let first = writes.first().copied()?;
    let assignment_span = enclosing_assignment_span(side, first)?;
    let assignment = find_json_by_span_index(side, assignment_span, Some("AssignmentExpression"))?;
    assignment.get("right").filter(|v| !v.is_null()).cloned()
}

/// babel has no paren nodes — strip oxc-only ParenthesizedExpression
/// wrappers at the JSON level.
fn unparen_json(value: &Value) -> &Value {
    let mut value = value;
    while let Value::Object(map) = value
        && map.get("type").and_then(Value::as_str) == Some("ParenthesizedExpression")
        && let Some(inner) = map.get("expression")
        && !inner.is_null()
    {
        value = inner;
    }
    value
}

/// The enclosing AssignmentExpression of a write reference's identifier —
/// the TS violation PATH is the assignment expression itself. Stops
/// (refuses) at an UpdateExpression (`x++` is a violation but not an
/// assignment — TS `t.isAssignmentExpression(first.node)` fails), at any
/// statement boundary and at the program root.
fn enclosing_assignment_span(side: &AlignSide<'_, '_>, write_span: Span) -> Option<Span> {
    let nodes = side.semantic.nodes();
    // The write's identifier node — located by span (identifier spans are
    // unique among arena nodes: parents are strictly wider).
    let mut cur = *side.node_by_span.get(&(write_span.start, write_span.end))?;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return None;
        }
        match nodes.get_node(parent).kind() {
            AstKind::AssignmentExpression(assignment) => return Some(assignment.span()),
            AstKind::UpdateExpression(_)
            | AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return None,
            kind if kind.is_statement() => return None,
            _ => cur = parent,
        }
    }
}

/// The span index over one side's whole-program ESTree JSON.
pub type JsonSpanIndex<'j> = HashMap<(u32, u32), Vec<&'j Value>>;

/// Index the whole-program ESTree JSON ONCE per side — the content
/// lookups ([`find_json_by_span_index`]) answer from it in O(1).
pub fn build_json_index(program_json: &Value) -> JsonSpanIndex<'_> {
    let mut map = HashMap::new();
    index_json_nodes(program_json, &mut map);
    map
}

/// Index every JSON object node by its (start, end) span, in the same
/// deterministic pre-order the old full-DFS lookup walked (object keys in
/// serde's BTreeMap order — a fixed total order, never insertion order —
/// arrays in order). The per-span Vec keeps walk order so a type-filtered
/// lookup reproduces the DFS's "first match" exactly.
fn index_json_nodes<'j>(root: &'j Value, map: &mut HashMap<(u32, u32), Vec<&'j Value>>) {
    match root {
        Value::Object(map_fields) => {
            if let (Some(s), Some(e)) = (
                map_fields.get("start").and_then(Value::as_u64),
                map_fields.get("end").and_then(Value::as_u64),
            ) {
                map.entry((s as u32, e as u32)).or_default().push(root);
            }
            for (_, v) in map_fields.iter() {
                index_json_nodes(v, map);
            }
        }
        Value::Array(items) => {
            for v in items {
                index_json_nodes(v, map);
            }
        }
        _ => {}
    }
}

/// Find a JSON node by its (start, end) span in the side's index —
/// optionally requiring a node type (the
/// ExpressionStatement-over-a-sole-expression collision, when no semicolon
/// follows, is disambiguated by the type). First match in walk order.
fn find_json_by_span_index<'j>(
    side: &AlignSide<'_, 'j>,
    span: Span,
    want_type: Option<&str>,
) -> Option<&'j Value> {
    side.json_by_span
        .get(&(span.start, span.end))?
        .iter()
        .copied()
        .find(|v| want_type.is_none_or(|w| json_type(v) == w))
}

/// Whether a hint's new binding still plays its prior role (TS
/// `bindingContentAgrees` :446): identical definition modulo names
/// (strongest), or literal-preserving shingle overlap at the same floor
/// the single-vote pin uses. Missing content is a refusal — no snap
/// without positive corroboration.
fn binding_content_agrees(
    prior_side: &AlignSide<'_, '_>,
    prior_symbol: Option<SymbolId>,
    fresh_side: &AlignSide<'_, '_>,
    fresh_symbol: SymbolId,
) -> bool {
    let Some(prior_symbol) = prior_symbol else {
        return false;
    };
    let Some(prior_content) = resolve_binding_content(prior_side, prior_symbol) else {
        return false;
    };
    let Some(fresh_content) = resolve_binding_content(fresh_side, fresh_symbol) else {
        return false;
    };
    // The strongest signal: the definitions are IDENTICAL modulo names —
    // the same walk the unit hashes use (blurred literals; TS
    // `hashPathWithMapping` on the content paths).
    if unit_hash(&prior_content, prior_side.tables, false)
        == unit_hash(&fresh_content, fresh_side.tables, false)
    {
        return true;
    }
    let prior_shingles = content_shingles(prior_side, &prior_content);
    let fresh_shingles = content_shingles(fresh_side, &fresh_content);
    if prior_shingles.is_empty() || fresh_shingles.is_empty() {
        return false;
    }
    jaccard_similarity(&prior_shingles, &fresh_shingles) >= SHINGLE_SIMILARITY_FLOOR
}

/// Slot-blind k-gram shingles over a value's serialized token stream (TS
/// `computeContentShingles`, binding-role.ts :44). Streams shorter than k
/// yield one shingle of the whole stream, so tiny contents (`null`, a
/// single literal) still compare.
fn content_shingles(side: &AlignSide<'_, '_>, content: &Value) -> BTreeSet<String> {
    let mut tokenizer = Tokenizer::new(side.tables, true);
    tokenizer.serialize_value(content, None, "");
    let tokens: Vec<String> = tokenizer
        .parts
        .into_iter()
        .map(blind_slot_ordinal)
        .collect();
    let mut shingles = BTreeSet::new();
    if tokens.len() <= SHINGLE_K {
        shingles.insert(tokens.join("\u{0}"));
        return shingles;
    }
    for i in 0..=(tokens.len() - SHINGLE_K) {
        shingles.insert(tokens[i..i + SHINGLE_K].join("\u{0}"));
        if shingles.len() >= SHINGLE_CAP {
            break;
        }
    }
    shingles
}

/// Binding-slot (`$3`) and label-slot (`L1`) tokens, ordinal-blinded
/// (binding-role.ts `blindSlotOrdinal` :37).
fn blind_slot_ordinal(token: String) -> String {
    if token.len() >= 2 && token.starts_with('$') && token[1..].bytes().all(|b| b.is_ascii_digit())
    {
        return "$".to_string();
    }
    if token.len() >= 2 && token.starts_with('L') && token[1..].bytes().all(|b| b.is_ascii_digit())
    {
        return "L".to_string();
    }
    token
}

// ---------------------------------------------------------------------------
// The content token walk — serializePathTokens(preserveLiterals: true)
// ---------------------------------------------------------------------------
//
// KEEP-IN-SYNC with `hash::serialize` (structural-hash.ts's walk): the
// identifier-role rules, the literal classes, the bare-position block
// unwrap and the skip keys are copied there from, because serialize.rs's
// `parts` string has no token delimiters and its helpers are private. A
// follow-up that makes `identifier_role` / the literal helpers
// `pub(crate)` can delete the copies below.

struct Tokenizer<'a> {
    parts: Vec<String>,
    /// symbol id → slot (first occurrence wins; the ordinal the token
    /// carries is arbitrary, because `blind_slot_ordinal` erases it —
    /// only the slot IDENTITY per symbol matters).
    slot_by_symbol: HashMap<SymbolId, String>,
    /// (slot, symbol, source name) in FIRST-OCCURRENCE order — the TS
    /// `mapping` Map. Binding slots only (labels and free identifiers
    /// never occupy slots). The order is load-bearing twice: the aligned
    /// pair's `next.mapping` iteration order is the evidence map's
    /// insertion order, so it decides the transfer/hint output order the
    /// frozen probe pins; and equal-hash units on both sides must have
    /// aligned ordinals.
    mapping: Vec<(String, Option<SymbolId>, String)>,
    label_slots: HashMap<String, String>,
    counter: u32,
    /// `preserveLiterals`: unit hashes and the snap gate hash BLURRED
    /// literals (false); content shingles and the switch-case test
    /// signature keep them verbatim (true).
    keep: bool,
    tables: &'a SymbolTables,
}

impl<'a> Tokenizer<'a> {
    fn new(tables: &'a SymbolTables, keep: bool) -> Tokenizer<'a> {
        Tokenizer {
            parts: Vec::with_capacity(256),
            slot_by_symbol: HashMap::new(),
            mapping: Vec::with_capacity(16),
            label_slots: HashMap::new(),
            counter: 0,
            keep,
            tables,
        }
    }

    fn serialize_value(&mut self, value: &Value, parent: Option<&Value>, key: &str) {
        match value {
            Value::Null => self.parts.push("null".to_string()),
            Value::Bool(b) => self
                .parts
                .push(if *b { "true" } else { "false" }.to_string()),
            Value::Number(n) => self.parts.push(n.to_string()),
            Value::String(s) => self.parts.push(json_escape(s)),
            Value::Array(items) => {
                self.parts.push("[".to_string());
                for item in items {
                    self.serialize_value(item, parent, key);
                    self.parts.push(",".to_string());
                }
                self.parts.push("]".to_string());
            }
            Value::Object(_) => self.serialize_node(value, parent, key),
        }
    }

    fn serialize_node(&mut self, node: &Value, parent: Option<&Value>, key: &str) {
        let Some(map) = node.as_object() else {
            self.serialize_value(node, parent, key);
            return;
        };
        let node_type = map
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("<no-type>")
            .to_string();

        if node_type == "Identifier" {
            self.serialize_identifier(map, parent, key);
            return;
        }

        // Private names: member keys, not scope bindings (verbatim — the
        // shingle walk runs with `privateNamesAsSlots` off).
        if node_type == "PrivateIdentifier" {
            let name = map.get("name").and_then(Value::as_str).unwrap_or("");
            self.parts.push(format!("P=#{name}"));
            return;
        }

        if let Some(tokens) = literal_tokens(map, &node_type, self.keep) {
            self.parts.extend(tokens);
            return;
        }

        // Single-statement blocks at bare-statement positions unwrap.
        if node_type == "BlockStatement"
            && let Some(inner) = unwrappable_block(map, parent, key)
        {
            self.serialize_node(inner, parent, key);
            return;
        }

        self.parts.push(format!("{node_type}{{"));
        // Child order is load BEARING, not cosmetic: the slot ordinals are
        // assigned by first occurrence in this walk, and the content
        // shingles are k-grams over the token STREAM — the overlap a
        // changed initializer shares with its prior version depends on
        // WHERE in the stream the change lands, which the key order
        // decides. The TS walks `Object.keys(babelNode)` — babel's field
        // order (callee before arguments); an alphabetical walk reorders
        // the stream and FLIPPED a snap-eligibility verdict in parity
        // testing (14/24 = 0.583 instead of the TS's ~0.36). So the walk
        // follows babel's VISITOR_KEYS order for every known type
        // ([`babel_child_keys`]); keys oxc carries that babel's visitor
        // list does not (e.g. `optional`) follow, alphabetically — the
        // same tail position babel's non-visitor fields land in.
        for k in ordered_child_keys(&node_type, map) {
            if SKIP_KEYS.contains(&k) || k == "innerComments" || k == "shorthand" {
                continue;
            }
            // oxc emits `optional: false` on EVERY call/member expression;
            // babel omits the field entirely for non-optional nodes (pinned
            // by test/parity/wp22-tok-debug.mjs: babel's CallExpression keys
            // are [type, start, end, loc, callee, arguments]). Carrying it
            // inflates both streams' scaffolding and inflated the shared
            // shingle count — an 8/17 TS jaccard read 14/24 here, flipping a
            // snap-eligibility verdict. `optional: true` stays (babel emits
            // it on the Optional* nodes).
            if k == "optional" && map[k] == Value::Bool(false) {
                continue;
            }
            // ONE token, `${k}:` — TS pushes the key and its colon as a
            // single part (`state.parts.push(`${k}:`)`), and the k-gram
            // windows sit on token boundaries: two tokens instead of one
            // shifts every later window and moved a 8/17 jaccard to 10/20,
            // across the 0.5 floor.
            self.parts.push(format!("{k}:"));
            self.serialize_value(&map[k], Some(node), k);
            self.parts.push(";".to_string());
        }
        self.parts.push("}".to_string());
    }

    fn serialize_identifier(
        &mut self,
        node: &serde_json::Map<String, Value>,
        parent: Option<&Value>,
        key: &str,
    ) {
        let name = node.get("name").and_then(Value::as_str).unwrap_or("");
        match identifier_role(parent, key) {
            "verbatim" => self.parts.push(format!("I={name}")),
            "label" => {
                let size = self.label_slots.len();
                let slot = self
                    .label_slots
                    .entry(name.to_string())
                    .or_insert_with(|| format!("L{size}"));
                self.parts.push(slot.clone());
            }
            _ => {
                let start = node
                    .get("start")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::MAX) as u32;
                let symbol = self
                    .tables
                    .decl_by_start
                    .get(&start)
                    .or_else(|| self.tables.ref_by_start.get(&start))
                    .copied();
                match symbol {
                    Some(symbol_id) => {
                        let counter = self.counter;
                        let slot = self
                            .slot_by_symbol
                            .entry(symbol_id)
                            .or_insert_with(|| format!("${counter}"))
                            .clone();
                        if slot == format!("${counter}") {
                            self.counter += 1;
                            // First occurrence: the slot is born here, so
                            // this is where the TS `mapping` records
                            // `slot → node.name` — in walk order.
                            self.mapping
                                .push((slot.clone(), Some(symbol_id), name.to_string()));
                        }
                        self.parts.push(slot);
                    }
                    None => {
                        // Free identifier — version-stable content.
                        self.parts.push(format!("I={name}"));
                    }
                }
            }
        }
    }
}

/// The skip keys — KEEP-IN-SYNC with `hash::serialize::SKIP_KEYS`.
const SKIP_KEYS: [&str; 8] = [
    "type",
    "loc",
    "start",
    "end",
    "range",
    "extra",
    "leadingComments",
    "trailingComments",
];

/// Babel's `VISITOR_KEYS` field order (generated from @babel/types) — the
/// child-key order of [`Tokenizer::serialize_node`]'s walk. Types oxc
/// emits under a different ESTree name fall back to alphabetical order,
/// which is still a fixed total order (never oxc's JSON insertion order,
/// which serde_json's BTreeMap has already discarded at parse time).
static BABEL_CHILD_KEYS: &[(&str, &[&str])] = &[
    ("ArrayExpression", &["elements"]),
    ("AssignmentExpression", &["left", "right"]),
    ("BinaryExpression", &["left", "right"]),
    ("Directive", &["value"]),
    ("BlockStatement", &["directives", "body"]),
    ("BreakStatement", &["label"]),
    (
        "CallExpression",
        &["callee", "typeParameters", "typeArguments", "arguments"],
    ),
    ("CatchClause", &["param", "body"]),
    (
        "ConditionalExpression",
        &["test", "consequent", "alternate"],
    ),
    ("ContinueStatement", &["label"]),
    ("DoWhileStatement", &["body", "test"]),
    ("ExpressionStatement", &["expression"]),
    ("File", &["program"]),
    ("ForInStatement", &["left", "right", "body"]),
    ("ForStatement", &["init", "test", "update", "body"]),
    (
        "FunctionDeclaration",
        &[
            "id",
            "typeParameters",
            "params",
            "predicate",
            "returnType",
            "body",
        ],
    ),
    (
        "FunctionExpression",
        &[
            "id",
            "typeParameters",
            "params",
            "predicate",
            "returnType",
            "body",
        ],
    ),
    ("Identifier", &["typeAnnotation", "decorators"]),
    ("IfStatement", &["test", "consequent", "alternate"]),
    ("LabeledStatement", &["label", "body"]),
    ("LogicalExpression", &["left", "right"]),
    ("MemberExpression", &["object", "property"]),
    (
        "NewExpression",
        &["callee", "typeParameters", "typeArguments", "arguments"],
    ),
    ("Program", &["directives", "body"]),
    ("ObjectExpression", &["properties"]),
    (
        "ObjectMethod",
        &[
            "decorators",
            "key",
            "typeParameters",
            "params",
            "returnType",
            "body",
        ],
    ),
    ("ObjectProperty", &["decorators", "key", "value"]),
    ("RestElement", &["argument", "typeAnnotation"]),
    ("ReturnStatement", &["argument"]),
    ("SequenceExpression", &["expressions"]),
    ("ParenthesizedExpression", &["expression"]),
    ("SwitchCase", &["test", "consequent"]),
    ("SwitchStatement", &["discriminant", "cases"]),
    ("ThrowStatement", &["argument"]),
    ("TryStatement", &["block", "handler", "finalizer"]),
    ("UnaryExpression", &["argument"]),
    ("UpdateExpression", &["argument"]),
    ("VariableDeclaration", &["declarations"]),
    ("VariableDeclarator", &["id", "init"]),
    ("WhileStatement", &["test", "body"]),
    ("WithStatement", &["object", "body"]),
    ("AssignmentPattern", &["left", "right", "decorators"]),
    ("ArrayPattern", &["elements", "typeAnnotation"]),
    (
        "ArrowFunctionExpression",
        &[
            "typeParameters",
            "params",
            "predicate",
            "returnType",
            "body",
        ],
    ),
    ("ClassBody", &["body"]),
    (
        "ClassExpression",
        &[
            "decorators",
            "id",
            "typeParameters",
            "superClass",
            "superTypeParameters",
            "mixins",
            "implements",
            "body",
        ],
    ),
    (
        "ClassDeclaration",
        &[
            "decorators",
            "id",
            "typeParameters",
            "superClass",
            "superTypeParameters",
            "mixins",
            "implements",
            "body",
        ],
    ),
    (
        "ExportAllDeclaration",
        &["source", "attributes", "assertions"],
    ),
    ("ExportDefaultDeclaration", &["declaration"]),
    (
        "ExportNamedDeclaration",
        &[
            "declaration",
            "specifiers",
            "source",
            "attributes",
            "assertions",
        ],
    ),
    ("ExportSpecifier", &["local", "exported"]),
    ("ForOfStatement", &["left", "right", "body"]),
    (
        "ImportDeclaration",
        &["specifiers", "source", "attributes", "assertions"],
    ),
    ("ImportDefaultSpecifier", &["local"]),
    ("ImportNamespaceSpecifier", &["local"]),
    ("ImportSpecifier", &["imported", "local"]),
    ("ImportExpression", &["source", "options"]),
    ("MetaProperty", &["meta", "property"]),
    (
        "ClassMethod",
        &[
            "decorators",
            "key",
            "typeParameters",
            "params",
            "returnType",
            "body",
        ],
    ),
    (
        "ObjectPattern",
        &["decorators", "properties", "typeAnnotation"],
    ),
    ("SpreadElement", &["argument"]),
    (
        "TaggedTemplateExpression",
        &["tag", "typeParameters", "quasi"],
    ),
    ("TemplateLiteral", &["quasis", "expressions"]),
    ("YieldExpression", &["argument"]),
    ("AwaitExpression", &["argument"]),
    ("ExportNamespaceSpecifier", &["exported"]),
    ("OptionalMemberExpression", &["object", "property"]),
    (
        "OptionalCallExpression",
        &["callee", "typeParameters", "typeArguments", "arguments"],
    ),
    (
        "ClassProperty",
        &["decorators", "variance", "key", "typeAnnotation", "value"],
    ),
    (
        "ClassAccessorProperty",
        &["decorators", "key", "typeAnnotation", "value"],
    ),
    (
        "ClassPrivateProperty",
        &["decorators", "variance", "key", "typeAnnotation", "value"],
    ),
    (
        "ClassPrivateMethod",
        &[
            "decorators",
            "key",
            "typeParameters",
            "params",
            "returnType",
            "body",
        ],
    ),
    ("PrivateName", &["id"]),
    ("StaticBlock", &["body"]),
    ("ImportAttribute", &["key", "value"]),
];

/// A node's child keys in the walk order: the babel visitor keys it
/// carries, then any remaining keys alphabetically (oxc extras such as
/// `optional` — babel's non-visitor fields sit in the same tail).
fn ordered_child_keys<'m>(
    node_type: &str,
    map: &'m serde_json::Map<String, Value>,
) -> Vec<&'m str> {
    let mut keys: Vec<&'m str> = Vec::with_capacity(map.len());
    let visitor = BABEL_CHILD_KEYS
        .iter()
        .find(|(t, _)| *t == node_type)
        .map(|(_, ks)| *ks);
    let mut remaining: Vec<&'m str> = map.keys().map(|k| k.as_str()).collect();
    if let Some(ks) = visitor {
        for k in ks {
            if map.contains_key(*k) {
                keys.push(k);
            }
            remaining.retain(|r| r != k);
        }
    }
    keys.extend(remaining);
    keys
}

/// The identifier-role rules (serialize.rs's copy of
/// structural-hash.ts:554-590) over the ESTree parent/key context —
/// oxc's ESTree type names (Property / MethodDefinition /
/// PropertyDefinition), NOT babel's. KEEP-IN-SYNC.
fn identifier_role(parent: Option<&Value>, key: &str) -> &'static str {
    let Some(parent) = parent else {
        return "slot";
    };
    let Some(map) = parent.as_object() else {
        return "slot";
    };
    let ptype = map.get("type").and_then(Value::as_str).unwrap_or("");
    let computed = map
        .get("computed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let positional = matches!(
        (ptype, key),
        ("MemberExpression", "property")
            | ("OptionalMemberExpression", "property")
            | ("Property", "key")
            | ("MethodDefinition", "key")
            | ("PropertyDefinition", "key")
    );
    if positional && !computed {
        return "verbatim";
    }
    if ptype == "MetaProperty" {
        return "verbatim";
    }
    if matches!(
        (ptype, key),
        ("ExportSpecifier", "exported") | ("ImportSpecifier", "imported")
    ) {
        return "verbatim";
    }
    if matches!(
        (ptype, key),
        ("LabeledStatement", "label")
            | ("BreakStatement", "label")
            | ("ContinueStatement", "label")
    ) {
        return "label";
    }
    "slot"
}

/// The literal tokens under the VERBATIM policy the shingles run under —
/// KEEP-IN-SYNC with `hash::serialize::literal_token` (same arms; `keep`
/// is fixed true here) EXCEPT TemplateElement, which the TS pushes as TWO
/// parts (`templateElementToken(...)` then `,tail=${node.tail}`) — the
/// shingle stream keeps that token boundary where serialize.rs's hash
/// walk merges it into one (the hash concatenates, so the merge is
/// byte-identical there; k-gram windows are NOT).
fn literal_tokens(
    map: &serde_json::Map<String, Value>,
    node_type: &str,
    keep: bool,
) -> Option<Vec<String>> {
    match node_type {
        // oxc's ESTree emits the STANDARD name "Literal" for every literal
        // (babel names StringLiteral/NumericLiteral/BigIntLiteral/
        // RegExpLiteral separately); classify by the value's JSON type the
        // way babel's type names classify.
        "Literal" => {
            if let Some(pattern) = map
                .get("regex")
                .and_then(|r| r.get("pattern"))
                .and_then(Value::as_str)
            {
                let flags = map
                    .get("regex")
                    .and_then(|r| r.get("flags"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Some(vec![format!("R={pattern}/{flags}")]);
            }
            if let Some(bigint) = map.get("bigint").and_then(Value::as_str) {
                return Some(vec![if keep {
                    format!("B={bigint}")
                } else {
                    "B=0".to_string()
                }]);
            }
            match map.get("value") {
                Some(Value::String(v)) => Some(vec![string_literal_token(v, keep)]),
                Some(Value::Number(n)) => Some(vec![if keep {
                    format!("N={n}")
                } else {
                    numeric_magnitude(n.as_f64().unwrap_or(0.0))
                }]),
                // Booleans and null are NOT literal-classed in the TS
                // either (babel's BooleanLiteral/NullLiteral fall through
                // to the generic walk).
                _ => None,
            }
        }
        "StringLiteral" | "DirectiveLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(vec![string_literal_token(value, keep)])
        }
        "NumericLiteral" => {
            let value = map.get("value")?.as_f64()?;
            Some(vec![if keep {
                format!("N={value}")
            } else {
                numeric_magnitude(value)
            }])
        }
        "BigIntLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(vec![if keep {
                format!("B={value}")
            } else {
                "B=0".to_string()
            }])
        }
        "RegExpLiteral" => {
            let pattern = map.get("pattern")?.as_str()?;
            let flags = map.get("flags")?.as_str()?;
            Some(vec![format!("R={pattern}/{flags}")])
        }
        "TemplateElement" => {
            let raw = map
                .get("value")
                .and_then(|v| v.get("raw"))
                .and_then(Value::as_str)?;
            let tail = map.get("tail").and_then(Value::as_bool).unwrap_or(false);
            Some(vec![
                template_element_token(raw, keep),
                format!(",tail={tail}"),
            ])
        }
        _ => None,
    }
}

/// Bare-statement positions (serialize.rs's copy of
/// structural-hash.ts:748-758). KEEP-IN-SYNC.
fn is_bare_statement_position(parent: Option<&Value>, key: &str) -> bool {
    let Some(parent) = parent else { return false };
    let Some(map) = parent.as_object() else {
        return false;
    };
    let ptype = map.get("type").and_then(Value::as_str).unwrap_or("");
    matches!(
        (ptype, key),
        ("IfStatement", "consequent")
            | ("IfStatement", "alternate")
            | ("ForStatement", "body")
            | ("ForInStatement", "body")
            | ("ForOfStatement", "body")
            | ("WhileStatement", "body")
            | ("DoWhileStatement", "body")
            | ("LabeledStatement", "body")
            | ("WithStatement", "body")
    )
}

/// A single-statement block at a bare position unwraps — unless the lone
/// statement is scoping-relevant (let/const/class/function), where the
/// braces change where the binding lives. KEEP-IN-SYNC with
/// `hash::serialize::unwrappable_block`.
fn unwrappable_block<'a>(
    node: &'a serde_json::Map<String, Value>,
    parent: Option<&Value>,
    key: &str,
) -> Option<&'a Value> {
    if !is_bare_statement_position(parent, key) {
        return None;
    }
    let body = node.get("body")?.as_array()?;
    if body.len() != 1 {
        return None;
    }
    let only = body.first()?;
    let otype = only.get("type").and_then(Value::as_str)?;
    if matches!(
        otype,
        "VariableDeclaration" | "FunctionDeclaration" | "ClassDeclaration"
    ) {
        return None;
    }
    Some(only)
}

/// The stream's string escaping — KEEP-IN-SYNC with
/// `hash::serialize::json_escape` (Node's `JSON.stringify` semantics for
/// the BMP).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod statement_align_test;
