//! The fingerprint index (WP2.1's match surface) — TS originals:
//! `src/analysis/fingerprint-index.ts` (`buildFingerprintIndex` :91,
//! `buildBindingFingerprintIndex` :121) and the full-fingerprint builders it
//! consumes from `src/analysis/function-fingerprint.ts`
//! (`buildFullFingerprint` :196, `buildBindingFullFingerprint` :263,
//! `computeEdgeNgrams` :319, `SHINGLE_SIMILARITY_FLOOR` :343,
//! `computeShingleSet` :355, `jaccardSimilarity` :384, `computeCalleeShape`
//! :25, `classifyCfgType` :46, `serializeCalleeShape` :64,
//! `calleeShapesEqual` :71, `extractMemberKey` :93).
//!
//! THE CASCADE is ported in `matching::cascade` (WP2.1 part 2:
//! fingerprint-index.ts's disambiguation cascade, demote/revoke post-passes
//! and the enclosing-statement rung), with the rung's statement contexts in
//! `matching::statement_context`. This module ports the surface the cascade
//! reads — the per-node fingerprints and the byStructuralHash buckets — plus
//! the shared similarity helpers.
//!
//! Data flow (WP2.1's scope decision): the builders take the UnifiedGraph
//! plus the Semantic and its SymbolTables. The TS reads
//! `fn.fingerprint.features` (computed once per function at graph build,
//! bound-identifier-aware) and walks babel paths; the Rust computes the
//! equivalent features table here — each function row's subtree serialized
//! to oxc ESTree JSON under graph.rs's serializer settings (no TS fields, no
//! ranges) and walked exactly like the TS's generic `Object.keys` child walk
//! (matching::features), with the bound-identifier test answered by the
//! symbol tables (the TS's `bindingByIdentifier` cache). Memory: one row's
//! JSON is alive at a time (the wrapper row's subtree is the whole bundle;
//! the TS pays the same by holding every babel AST for the run).
//!
//! Identity: entries are keyed by ROW SPAN (07 §1's span identity), not by
//! session-id string; the session ids ride along for parity with the TS
//! dump. Iteration order: the TS iterates the `functions` Map in insertion
//! order (= buildFunctionGraph's row order); the Rust iterates
//! `graph.functions` / `graph.module_bindings` in Vec order (the same build
//! order), so `entries` IS that order — no hash-map iteration anywhere in
//! the build (house rule: hash-map order must never reach a decision).
//!
//! Sort orders, proven byte-identical to the TS (the WP2.1 probe asserts
//! `sortsAgree` over every CalleeShape serialization for arity 0-16 x
//! complexity 0-40 x 4 cfg types x both booleans —
//! test/parity/wp21-probe.mjs): calleeShapes/callerShapes sort by their
//! SERIALIZED string — a pure-ASCII alphabet, where a byte sort equals
//! babel's localeCompare; calleeHashes/twoHopShapes sort with the default JS
//! sort (UTF-16 code units), which equals a UTF-8 byte sort for every
//! well-formed Unicode string (the surrogate range holds no scalars).

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use crate::graph::{GraphFunction, UnifiedGraph};
use crate::hash::serialize::SymbolTables;

pub mod alternation;
pub mod cascade;
pub mod features;
pub mod matches_dump;
pub mod member_key;
pub mod statement_context;

// ---------------------------------------------------------------------------
// CalleeShape (function-fingerprint.ts :25-76)
// ---------------------------------------------------------------------------

/// The blurred callee description (TS `CalleeShape`): structure without
/// identity, so matching a callee cannot cascade a wrong name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalleeShape {
    pub arity: u32,
    pub complexity: u32,
    pub cfg_type: CfgType,
    pub has_external_calls: bool,
}

/// TS `classifyCfgType` (:46): loop>0 && branch>0 → complex, loop>0 →
/// looping, branch>0 → branching, else linear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfgType {
    Linear,
    Branching,
    Looping,
    Complex,
}

impl CfgType {
    /// The TS union's serialized spelling (`serializeCalleeShape` :64).
    pub fn as_str(self) -> &'static str {
        match self {
            CfgType::Linear => "linear",
            CfgType::Branching => "branching",
            CfgType::Looping => "looping",
            CfgType::Complex => "complex",
        }
    }
}

impl CalleeShape {
    /// TS `computeCalleeShape` (:25) from a features row.
    pub fn of_features(features: &StructuralFeatures) -> CalleeShape {
        CalleeShape {
            arity: features.arity,
            complexity: features.complexity,
            cfg_type: classify_cfg_type(features),
            has_external_calls: !features.external_calls.is_empty(),
        }
    }

    /// TS `serializeCalleeShape` (:64) — the string the shapes are sorted
    /// and compared by.
    pub fn serialized(&self) -> String {
        format!(
            "({},{},{},{})",
            self.arity,
            self.complexity,
            self.cfg_type.as_str(),
            self.has_external_calls
        )
    }
}

/// TS `classifyCfgType` (:46).
pub fn classify_cfg_type(features: &StructuralFeatures) -> CfgType {
    if features.loop_count > 0 && features.branch_count > 0 {
        CfgType::Complex
    } else if features.loop_count > 0 {
        CfgType::Looping
    } else if features.branch_count > 0 {
        CfgType::Branching
    } else {
        CfgType::Linear
    }
}

/// Sort shapes by their serialized form — the TS sorts with
/// `serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))`
/// (:217-219); the output alphabet is ASCII, so a byte sort agrees (probe
/// proven, module doc).
pub fn sort_callee_shapes(shapes: &mut [CalleeShape]) {
    shapes.sort_by_key(|a| a.serialized());
}

/// TS `calleeShapesEqual` (:71): equal length and elementwise equal
/// serialized forms after sorting both sides — order-insensitive equality.
pub fn callee_shapes_equal(a: &[CalleeShape], b: &[CalleeShape]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut sa: Vec<String> = a.iter().map(CalleeShape::serialized).collect();
    let mut sb: Vec<String> = b.iter().map(CalleeShape::serialized).collect();
    sa.sort_unstable();
    sb.sort_unstable();
    sa == sb
}

// ---------------------------------------------------------------------------
// StructuralFeatures (structural-hash.ts types / types.ts StructuralFeatures)
// ---------------------------------------------------------------------------

/// The TS `StructuralFeatures` (types.ts) as the features walk fills it.
/// The array fields are deduped + sorted by the walk's tail (Set semantics).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructuralFeatures {
    pub arity: u32,
    pub has_rest_param: bool,
    pub return_count: u32,
    /// Starts at 1 (the base cyclomatic complexity, :286).
    pub complexity: u32,
    pub cfg_shape: String,
    pub loop_count: u32,
    pub branch_count: u32,
    pub try_count: u32,
    pub string_literals: Vec<String>,
    pub numeric_literals: Vec<f64>,
    pub external_calls: Vec<String>,
    pub property_accesses: Vec<String>,
}

// ---------------------------------------------------------------------------
// Fingerprints (types.ts FunctionFingerprint union; the builders :196/:263)
// ---------------------------------------------------------------------------

/// The function side (TS `FunctionSideFingerprint`). `features` is REQUIRED
/// here — the TS types it optional, but every index-path fingerprint comes
/// from computeFingerprintAndPlaceholders, which always fills it (the probe
/// prints features for all 13 rows).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSideFingerprint {
    pub structural_hash: String,
    pub features: StructuralFeatures,
    pub member_key: Option<String>,
    pub callee_shapes: Vec<CalleeShape>,
    pub caller_shapes: Vec<CalleeShape>,
    pub callee_hashes: Vec<String>,
    pub two_hop_shapes: Vec<String>,
}

/// The binding side (TS `kind: "binding"`): no features, no member key —
/// shape concepts like arity do not apply to a binding (:254-262).
#[derive(Debug, Clone, PartialEq)]
pub struct BindingSideFingerprint {
    pub structural_hash: String,
    pub callee_shapes: Vec<CalleeShape>,
    pub caller_shapes: Vec<CalleeShape>,
    pub callee_hashes: Vec<String>,
    pub two_hop_shapes: Vec<String>,
}

/// The TS `FunctionFingerprint` union (types.ts) — discriminated by `kind`.
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionFingerprint {
    Function(FunctionSideFingerprint),
    Binding(BindingSideFingerprint),
}

impl FunctionFingerprint {
    /// TS `kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            FunctionFingerprint::Function(_) => "function",
            FunctionFingerprint::Binding(_) => "binding",
        }
    }

    pub fn structural_hash(&self) -> &str {
        match self {
            FunctionFingerprint::Function(f) => &f.structural_hash,
            FunctionFingerprint::Binding(f) => &f.structural_hash,
        }
    }

    /// TS `fingerprintMemberKey` (types.ts): undefined for bindings.
    pub fn member_key(&self) -> Option<&str> {
        match self {
            FunctionFingerprint::Function(f) => f.member_key.as_deref(),
            FunctionFingerprint::Binding(_) => None,
        }
    }

    /// TS `fingerprintFeatures` (types.ts): undefined for bindings.
    pub fn features(&self) -> Option<&StructuralFeatures> {
        match self {
            FunctionFingerprint::Function(f) => Some(&f.features),
            FunctionFingerprint::Binding(_) => None,
        }
    }

    pub fn callee_shapes(&self) -> &[CalleeShape] {
        match self {
            FunctionFingerprint::Function(f) => &f.callee_shapes,
            FunctionFingerprint::Binding(f) => &f.callee_shapes,
        }
    }

    pub fn caller_shapes(&self) -> &[CalleeShape] {
        match self {
            FunctionFingerprint::Function(f) => &f.caller_shapes,
            FunctionFingerprint::Binding(f) => &f.caller_shapes,
        }
    }

    pub fn callee_hashes(&self) -> &[String] {
        match self {
            FunctionFingerprint::Function(f) => &f.callee_hashes,
            FunctionFingerprint::Binding(f) => &f.callee_hashes,
        }
    }

    pub fn two_hop_shapes(&self) -> &[String] {
        match self {
            FunctionFingerprint::Function(f) => &f.two_hop_shapes,
            FunctionFingerprint::Binding(f) => &f.two_hop_shapes,
        }
    }
}

// ---------------------------------------------------------------------------
// The index (fingerprint-index.ts :91 / :121)
// ---------------------------------------------------------------------------

/// Which row an index entry came from — the TS's two node kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexNode {
    /// Index into `graph.functions`.
    Function(usize),
    /// Index into `graph.module_bindings`.
    Binding(usize),
}

/// Which node kind the index was built over. PART-2 EXTENSION (the cascade
/// port): the TS cascade answers several questions by testing
/// `index.functions` / `index.moduleBindings` for presence
/// (`tryShingleResolve` :241, `shingleUnconsultable` :731,
/// `distinctStatements` :507, `getEnclosingStmtHash` :349) — the Rust index
/// carries the kind explicitly instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    Functions,
    Bindings,
}

/// One index row.
#[derive(Debug)]
pub struct IndexEntry {
    pub node: IndexNode,
    /// The TS key (`input.js:L:C` / `module:<name>`) — parity with the dump.
    pub session_id: String,
    pub fingerprint: FunctionFingerprint,
}

/// The built index — `FingerprintIndex` (types.ts) as the cascade reads it:
/// fingerprints per node + the byStructuralHash buckets (the uniqueHash
/// tier's candidate pools).
#[derive(Debug)]
pub struct FingerprintIndex<'g> {
    pub graph: &'g UnifiedGraph,
    /// Which node kind the index was built over (part-2 extension, see the
    /// enum's doc).
    pub kind: IndexKind,
    /// The function-row features table, parallel to `graph.functions` — the
    /// shapes' input and the shingle tiebreaker's token source. Built for
    /// both index kinds (the binding fingerprints' shapes read it too).
    pub features: Vec<StructuralFeatures>,
    /// TS `fingerprints` Map, as a Vec in build order (see module doc).
    pub entries: Vec<IndexEntry>,
    /// TS `byStructuralHash`: hash → entry indices, in insertion order.
    pub by_structural_hash: HashMap<String, Vec<usize>>,
    /// Session id → entry index (part-2 extension: the observation rows and
    /// the tail tiers address entries by session id, the TS by Map key).
    session_to_entry: HashMap<String, usize>,
    /// Row span → entry index (the cascade's node lookup; 07 §1's span
    /// identity in place of the TS's sessionId keys).
    by_span: HashMap<(u32, u32), usize>,
    /// Function-row span → `graph.functions` index (the callee/caller joins;
    /// also the shingle helpers' callee lookup).
    fn_idx_by_span: HashMap<(u32, u32), usize>,
}

impl FingerprintIndex<'_> {
    /// The bucket for one structural hash (the uniqueHash tier's pool).
    pub fn bucket(&self, hash: &str) -> Option<&[usize]> {
        self.by_structural_hash.get(hash).map(Vec::as_slice)
    }

    /// The entry index for a session id (part-2 extension; the TS keys its
    /// maps by session id directly).
    pub fn entry_of_session(&self, session_id: &str) -> Option<usize> {
        self.session_to_entry.get(session_id).copied()
    }

    /// The session id of the FUNCTION row whose span is `span` — the
    /// scope-parent lookups (`recordParentAgreement` :524,
    /// `crossedContainerIds` :564). PART-2 EXTENSION: the TS reads
    /// `fn.scopeParent?.sessionId` off the node it holds; the Rust graph
    /// stores the parent as a span, so the row join is the index's job.
    pub fn function_session_of_span(&self, span: Span) -> Option<&str> {
        self.fn_idx_by_span
            .get(&(span.start, span.end))
            .map(|&i| self.graph.functions[i].session_id.as_str())
    }

    /// The entry index for a row span (function or binding).
    pub fn entry_of_span(&self, span: Span) -> Option<usize> {
        self.by_span.get(&(span.start, span.end)).copied()
    }

    /// A copy of the index over a SUBSET of its entries, in entry order,
    /// with the hash buckets and the session/span lookups rebuilt. Consumes
    /// the index (the graph borrow and the features table carry over).
    ///
    /// Why the filter lands AFTER the build: the TS filters the binding
    /// LIST before `buildBindingFingerprintIndex`
    /// (prior-version.ts:1615-1617's matchable filter), while the Rust
    /// builder runs over the whole graph — and must keep doing so, because
    /// the WP2.1 probe's TS caller (fingerprint-index.test.ts:1088) passes
    /// the graph's UNfiltered bindings and the frozen snapshot pins that
    /// content. Filtering here reproduces the pipeline's effective
    /// behavior without touching the builder.
    pub fn retain_entries(mut self, keep: impl Fn(&IndexEntry) -> bool) -> Self {
        self.entries.retain(keep);
        let mut by_structural_hash: HashMap<String, Vec<usize>> = HashMap::new();
        let mut session_to_entry: HashMap<String, usize> =
            HashMap::with_capacity(self.entries.len());
        let mut by_span: HashMap<(u32, u32), usize> = HashMap::with_capacity(self.entries.len());
        for (i, entry) in self.entries.iter().enumerate() {
            by_structural_hash
                .entry(entry.fingerprint.structural_hash().to_string())
                .or_default()
                .push(i);
            session_to_entry.insert(entry.session_id.clone(), i);
            let span = match entry.node {
                IndexNode::Function(row) => self.graph.functions[row].span,
                IndexNode::Binding(row) => self.graph.module_bindings[row].span,
            };
            by_span.insert((span.start, span.end), i);
        }
        self.by_structural_hash = by_structural_hash;
        self.session_to_entry = session_to_entry;
        self.by_span = by_span;
        self
    }

    /// TS `computeEdgeNgrams` (:319) for a function row: one
    /// `<myHash>→<calleeId>` pair per internal callee, exact (callee hash)
    /// or blurred (serialized callee shape).
    pub fn compute_edge_ngrams(&self, fn_idx: usize, mode: EdgeNgramMode) -> Vec<String> {
        let f = &self.graph.functions[fn_idx];
        let my_hash = &f.structural_hash;
        f.internal_callees
            .iter()
            .filter_map(|span| self.fn_idx_by_span.get(&(span.start, span.end)).copied())
            .map(|callee_idx| {
                let callee_id = match mode {
                    EdgeNgramMode::Exact => {
                        self.graph.functions[callee_idx].structural_hash.clone()
                    }
                    EdgeNgramMode::Blurred => {
                        CalleeShape::of_features(&self.features[callee_idx]).serialized()
                    }
                };
                format!("{my_hash}→{callee_id}")
            })
            .collect()
    }

    /// TS `computeShingleSet` (:355) for a function row: the blurred edge
    /// n-grams plus the feature tokens. NOT memoized — the TS memoizes in
    /// the owning AST's cache because the tiebreaker recomputes the same
    /// candidates O(bucket²) times; the Rust cascade port owns that
    /// decision when it lands.
    pub fn compute_shingle_set(&self, fn_idx: usize) -> BTreeSet<String> {
        let mut shingles: BTreeSet<String> = self
            .compute_edge_ngrams(fn_idx, EdgeNgramMode::Blurred)
            .into_iter()
            .collect();
        let Some(f) = self.features.get(fn_idx) else {
            return shingles;
        };
        for ext in &f.external_calls {
            shingles.insert(format!("ext:{ext}"));
        }
        for prop in &f.property_accesses {
            shingles.insert(format!("prop:{prop}"));
        }
        for s in &f.string_literals {
            shingles.insert(format!("str:{s}"));
        }
        shingles
    }
}

/// TS `computeEdgeNgrams`'s `mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeNgramMode {
    Exact,
    Blurred,
}

/// TS `SHINGLE_SIMILARITY_FLOOR` (:343) — the cascade tiebreaker and
/// close-match corroboration both gate on this constant.
pub const SHINGLE_SIMILARITY_FLOOR: f64 = 0.5;

/// TS `jaccardSimilarity` (:384): |A∩B| / |A∪B|; 1 when both empty, 0 when
/// the union is somehow empty otherwise.
pub fn jaccard_similarity(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.len() + b.len() - intersection;
    if union > 0 {
        intersection as f64 / union as f64
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// buildFingerprintIndex (fingerprint-index.ts :91)
// ---------------------------------------------------------------------------

/// TS `buildFingerprintIndex`: one entry per function row, in graph order,
/// bucketed by structural hash.
pub fn build_fingerprint_index<'g>(
    graph: &'g UnifiedGraph,
    semantic: &Semantic<'_>,
    tables: &SymbolTables,
) -> FingerprintIndex<'g> {
    let features = features::features_table(&graph.functions, semantic, tables);
    let fn_idx_by_span = function_span_index(graph);
    let member_keys = function_member_keys(graph, semantic, tables);
    // TS `fn.callers` (analyzeCallees pairs every internal-callee edge) —
    // the exact inversion of the rows' internalCallees.
    let callers = inverted_callers(graph, &fn_idx_by_span);

    let entries: Vec<IndexEntry> = (0..graph.functions.len())
        .map(|i| {
            let fingerprint = build_full_fingerprint(
                graph,
                &features,
                &fn_idx_by_span,
                &callers,
                &member_keys,
                i,
            );
            IndexEntry {
                node: IndexNode::Function(i),
                session_id: graph.functions[i].session_id.clone(),
                fingerprint: FunctionFingerprint::Function(fingerprint),
            }
        })
        .collect();

    finish_index(
        graph,
        IndexKind::Functions,
        features,
        entries,
        fn_idx_by_span,
    )
}

/// TS `buildFullFingerprint` (:196) for graph row `i`. `excludeFromShapes`
/// is not ported: the index path (:102) never passes it.
#[allow(clippy::too_many_arguments)]
fn build_full_fingerprint(
    graph: &UnifiedGraph,
    features: &[StructuralFeatures],
    fn_idx_by_span: &HashMap<(u32, u32), usize>,
    callers: &[Vec<usize>],
    member_keys: &[Option<String>],
    i: usize,
) -> FunctionSideFingerprint {
    let callees = callee_indices_of(graph, fn_idx_by_span, i);
    let mut callee_shapes: Vec<CalleeShape> = callees
        .iter()
        .map(|&c| CalleeShape::of_features(&features[c]))
        .collect();
    sort_callee_shapes(&mut callee_shapes);

    let mut caller_shapes: Vec<CalleeShape> = callers[i]
        .iter()
        .map(|&c| CalleeShape::of_features(&features[c]))
        .collect();
    sort_callee_shapes(&mut caller_shapes);

    let mut callee_hashes: Vec<String> = callees
        .iter()
        .map(|&c| graph.functions[c].structural_hash.clone())
        .collect();
    callee_hashes.sort();

    // twoHopShapes (:240-248): the serialized shapes of the callees'
    // callees, deduped via the Set.
    let mut two_hop: BTreeSet<String> = BTreeSet::new();
    for &callee in &callees {
        for span in &graph.functions[callee].internal_callees {
            if let Some(&c2) = fn_idx_by_span.get(&(span.start, span.end)) {
                two_hop.insert(CalleeShape::of_features(&features[c2]).serialized());
            }
        }
    }

    FunctionSideFingerprint {
        structural_hash: graph.functions[i].structural_hash.clone(),
        features: features[i].clone(),
        member_key: member_keys[i].clone(),
        callee_shapes,
        caller_shapes,
        callee_hashes,
        two_hop_shapes: two_hop.into_iter().collect(),
    }
}

// ---------------------------------------------------------------------------
// buildBindingFingerprintIndex (fingerprint-index.ts :121)
// ---------------------------------------------------------------------------

/// TS `buildBindingFingerprintIndex`: one entry per HASHABLE binding row
/// (unhashable inits can never match across versions — :117-120), in
/// module_bindings order, bucketed by the binding fingerprint hash.
pub fn build_binding_fingerprint_index<'g>(
    graph: &'g UnifiedGraph,
    semantic: &Semantic<'_>,
    tables: &SymbolTables,
) -> FingerprintIndex<'g> {
    let features = features::features_table(&graph.functions, semantic, tables);
    let fn_idx_by_span = function_span_index(graph);
    let binding_idx_by_span = binding_span_index(graph);
    // TS `binding.callers` — edge builder 4d (function-graph.ts :689),
    // computed at graph build (crate::graph::ModuleBindingNode::callers,
    // where the referencePaths probe notes live) and joined to function
    // rows here.
    let binding_callers = binding_caller_indices(graph, &fn_idx_by_span);

    let entries: Vec<IndexEntry> = graph
        .module_bindings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.fingerprint_hash.is_some())
        .map(|(j, b)| {
            let fingerprint = build_binding_full_fingerprint(
                graph,
                &features,
                &fn_idx_by_span,
                &binding_idx_by_span,
                &binding_callers[j],
                j,
            );
            IndexEntry {
                node: IndexNode::Binding(j),
                session_id: b.session_id.clone(),
                fingerprint: FunctionFingerprint::Binding(fingerprint),
            }
        })
        .collect();

    finish_index(
        graph,
        IndexKind::Bindings,
        features,
        entries,
        fn_idx_by_span,
    )
}

/// TS `buildBindingFullFingerprint` (:263) for binding row `j`. The row's
/// `fingerprint_hash` is Some by the caller's filter (:131 `if
/// !binding.fingerprint continue`). Binding callees that are FUNCTIONS
/// contribute shapes and hashes exactly like function callees; binding
/// callees contribute their init hash to calleeHashes only (None filtered —
/// the old name-derived fallback made the parent's fingerprint
/// rename-variant, :287-292).
#[allow(clippy::too_many_arguments)]
fn build_binding_full_fingerprint(
    graph: &UnifiedGraph,
    features: &[StructuralFeatures],
    fn_idx_by_span: &HashMap<(u32, u32), usize>,
    binding_idx_by_span: &HashMap<(u32, u32), usize>,
    callers: &[usize],
    j: usize,
) -> BindingSideFingerprint {
    let binding = &graph.module_bindings[j];
    // The callees are MIXED (bindings' identifiers + functions' spans);
    // classify by which row map holds the span.
    let fn_callees: Vec<usize> = binding
        .internal_callees
        .iter()
        .filter_map(|span| fn_idx_by_span.get(&(span.start, span.end)).copied())
        .collect();

    let mut callee_shapes: Vec<CalleeShape> = fn_callees
        .iter()
        .map(|&c| CalleeShape::of_features(&features[c]))
        .collect();
    sort_callee_shapes(&mut callee_shapes);

    let mut caller_shapes: Vec<CalleeShape> = callers
        .iter()
        .map(|&c| CalleeShape::of_features(&features[c]))
        .collect();
    sort_callee_shapes(&mut caller_shapes);

    let mut callee_hashes: Vec<String> = binding
        .internal_callees
        .iter()
        .filter_map(|span| {
            if let Some(&c) = fn_idx_by_span.get(&(span.start, span.end)) {
                return Some(graph.functions[c].structural_hash.clone());
            }
            if let Some(&b) = binding_idx_by_span.get(&(span.start, span.end)) {
                return graph.module_bindings[b].fingerprint_hash.clone();
            }
            None
        })
        .collect();
    callee_hashes.sort();

    let mut two_hop: BTreeSet<String> = BTreeSet::new();
    for &callee in &fn_callees {
        for span in &graph.functions[callee].internal_callees {
            if let Some(&c2) = fn_idx_by_span.get(&(span.start, span.end)) {
                two_hop.insert(CalleeShape::of_features(&features[c2]).serialized());
            }
        }
    }

    BindingSideFingerprint {
        structural_hash: binding.fingerprint_hash.clone().expect("hashable"),
        callee_shapes,
        caller_shapes,
        callee_hashes,
        two_hop_shapes: two_hop.into_iter().collect(),
    }
}

// ---------------------------------------------------------------------------
// Shared index assembly
// ---------------------------------------------------------------------------

fn finish_index<'g>(
    graph: &'g UnifiedGraph,
    kind: IndexKind,
    features: Vec<StructuralFeatures>,
    entries: Vec<IndexEntry>,
    fn_idx_by_span: HashMap<(u32, u32), usize>,
) -> FingerprintIndex<'g> {
    let mut by_structural_hash: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_span: HashMap<(u32, u32), usize> = HashMap::with_capacity(entries.len());
    let mut session_to_entry: HashMap<String, usize> = HashMap::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        by_structural_hash
            .entry(entry.fingerprint.structural_hash().to_string())
            .or_default()
            .push(i);
        session_to_entry.insert(entry.session_id.clone(), i);
        let span = match entry.node {
            IndexNode::Function(i) => graph.functions[i].span,
            IndexNode::Binding(j) => graph.module_bindings[j].span,
        };
        by_span.insert((span.start, span.end), i);
    }
    FingerprintIndex {
        graph,
        kind,
        features,
        entries,
        by_structural_hash,
        session_to_entry,
        by_span,
        fn_idx_by_span,
    }
}

/// Function-row span → row index.
fn function_span_index(graph: &UnifiedGraph) -> HashMap<(u32, u32), usize> {
    graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.span.start, f.span.end), i))
        .collect()
}

/// Binding-row span → row index (ALL rows — the callee classification reads
/// the node's fingerprint, not the index membership, TS :290).
fn binding_span_index(graph: &UnifiedGraph) -> HashMap<(u32, u32), usize> {
    graph
        .module_bindings
        .iter()
        .enumerate()
        .map(|(j, b)| ((b.span.start, b.span.end), j))
        .collect()
}

/// The internal-callee indices of function row `i` (spans → row indices).
fn callee_indices_of(
    graph: &UnifiedGraph,
    fn_idx_by_span: &HashMap<(u32, u32), usize>,
    i: usize,
) -> Vec<usize> {
    graph.functions[i]
        .internal_callees
        .iter()
        .filter_map(|span| fn_idx_by_span.get(&(span.start, span.end)).copied())
        .collect()
}

/// TS `fn.callers`: analyzeCallees pairs EVERY internal-callee edge, so the
/// callers are the exact inversion of the rows' internalCallees (deduped —
/// the TS holds a Set).
fn inverted_callers(
    graph: &UnifiedGraph,
    fn_idx_by_span: &HashMap<(u32, u32), usize>,
) -> Vec<Vec<usize>> {
    let mut callers: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); graph.functions.len()];
    for (i, f) in graph.functions.iter().enumerate() {
        for span in &f.internal_callees {
            if let Some(&c) = fn_idx_by_span.get(&(span.start, span.end)) {
                callers[c].insert(i);
            }
        }
    }
    callers
        .into_iter()
        .map(|set| set.into_iter().collect())
        .collect()
}

/// The member keys of every function row, in row order.
fn function_member_keys(
    graph: &UnifiedGraph,
    semantic: &Semantic<'_>,
    tables: &SymbolTables,
) -> Vec<Option<String>> {
    let nodes = semantic.nodes();
    let row_ids = row_node_ids(&graph.functions, nodes);
    graph
        .functions
        .iter()
        .map(|f| {
            row_ids
                .get(&(f.span.start, f.span.end))
                .and_then(|(node_id, kind)| {
                    member_key::extract_member_key(*node_id, *kind, f.span, semantic, tables)
                })
        })
        .collect()
}

/// The (NodeId, AstKind) of each function row, from one semantic sweep —
/// the parent walks (member keys) and the row serialization (features) both
/// need the arena node, which a span no longer names.
pub(crate) fn row_node_ids<'a>(
    functions: &[GraphFunction],
    nodes: &'a oxc_semantic::AstNodes<'a>,
) -> HashMap<(u32, u32), (NodeId, AstKind<'a>)> {
    let row_spans: HashSet<(u32, u32)> = functions
        .iter()
        .map(|f| (f.span.start, f.span.end))
        .collect();
    let mut out = HashMap::with_capacity(row_spans.len());
    for node in nodes.iter() {
        let span = node.span();
        let key = (span.start, span.end);
        if row_spans.contains(&key) {
            out.insert(key, (node.id(), node.kind()));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Edge builder 4d: binding callers (function-graph.ts :689)
// ---------------------------------------------------------------------------

/// TS `binding.callers` — edge builder 4d
/// (`addFunctionToBindingReferenceEdges`, function-graph.ts :689), now
/// computed at graph build (`crate::graph::ModuleBindingNode::callers`,
/// where the referencePaths probe notes live) and joined to function rows
/// here.
fn binding_caller_indices(
    graph: &UnifiedGraph,
    fn_idx_by_span: &HashMap<(u32, u32), usize>,
) -> Vec<Vec<usize>> {
    graph
        .module_bindings
        .iter()
        .map(|binding| {
            binding
                .callers
                .iter()
                .filter_map(|span| fn_idx_by_span.get(&(span.start, span.end)).copied())
                .collect()
        })
        .collect()
}
