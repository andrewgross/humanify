# Matching architecture: one engine for "is this the same code?"

Status: steps 1-4 BUILT (2026-10-14); step 5 (behaviour changes) is open,
on experiment branches with evals. The settings and the library index's
life cycle are in [library-matching-config.md](./library-matching-config.md).

Progress: step 1 (shared helpers), step 2 (hash forms as named, versioned
settings: `hash/forms.rs`) and step 3 (one cutter `cut.rs`, one graph
`graph/edges.rs`, region scope plumbed; merged `b7235695`) are on main.
Step 4 (reference store + rule lists, and the library profile reading graph
scope) is built on branch `refactor/matching-reference-store` (2026-10-14,
§3.7). §2 below is the inventory as it stood BEFORE them.

## Totals first

- **One question, two uses.** humanify asks "is this piece of code the same as
  that one?" in two places: against the **prior release** (to keep its names and
  files) and, on the exp095 branches, against an **index of npm package builds**
  (to say "this is lodash 4.17"). Both are the same shape: cut the new bundle into
  pieces, fingerprint each piece, look the fingerprint up in a store, then decide.
- **Today that is done by about twenty separate pieces of code**: six ways to
  print code for hashing, four literal extractors, four shingle (overlapping
  token run) schemes, two known-globals lists, two graphs, and four version
  stamps that move independently. Some differences are deliberate and written
  down; others are copies that only stay in step by a "KEEP-IN-SYNC" comment.
- **Target: five layers** — one cutter, one serializer with each hash form as a
  named and versioned setting, one evidence layer, one reference-store
  interface with two implementations (prior release, library index), and one
  decision layer that runs an ordered rule list per profile and says which rule
  decided each match.
- **The graph rule (Andrew, 2026-10-13).** Every call/reference edge carries a
  scope: INSIDE a region, INTO it, or OUT OF it. Library identification may
  never read INTO edges (app callers are app-specific), and compares by
  containment, not equality.
- **Order of work: five steps.** Steps 1-4 change no output byte and are each
  proven by `npm run check` plus a WARM neutrality run. Step 5 (behaviour
  changes: graph scope for vendor code in cross-version, names from the index)
  goes on experiment branches with evals.
- **The three risks:** the match list's insertion order drives rename order;
  four hash versions must not be silently merged; the perf arc's numbers
  (fresh run 8.35 GB / 368 s) must not regress.

## 1. The common problem

Both cases are "a query against a reference store":

|                       | cross-version                                              | library identification                                                       |
| --------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------------- |
| query                 | the new release's bundle                                   | the same bundle                                                              |
| reference store       | the prior release: ONE program, with human names and files | the index: MANY programs (package@version builds), each with a package label |
| what a match gives    | names, file placement, module carry                        | a package label (later: names, §6)                                           |
| where it lives        | in memory, built per run                                   | on disk, built offline, reused                                               |
| cost of a wrong match | a wrong name on app code (eval-visible)                    | a wrong package label; worse if it ever freezes or names app code            |

A prior release is just an index of one program whose labels happen to be
names. That is the whole case for one engine: the cutting, fingerprinting and
evidence are the same; the store and the decision rules differ.

## 2. What exists today

Paths below are under `crates/humanify-core/src/` on main; "branch" means
`/work/exp095-discovery-core/crates/humanify-core/src/`.

### 2.1 Six ways to print code for a hash

| form                   | where                                                                                                                                 | piece                                     | local names                          | strings                              | property names | wrapper spelling rule                   | version stamp                                 |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------------------------------------ | ------------------------------------ | -------------- | --------------------------------------- | --------------------------------------------- |
| match key              | `hash/serialize.rs` (`canonical_serialize`, `LiteralPolicy::MatchKey`); function hash at `graph.rs` (`GraphFunction.structural_hash`) | function                                  | numbered slots                       | blurred to length / volatile classes | kept           | yes                                     | none (the serializer has no version constant) |
| identity key           | `LiteralPolicy::Verbatim`: `graph.rs` (`binding_fingerprint_hash`), `finish/vendor_inherit.rs`, `naming/driver/validate.rs`           | binding, file                             | slots                                | exact                                | kept           | no (gated on match key, `serialize.rs`) | none                                          |
| statement hash         | `hash/statement_hash.rs`                                                                                                              | top-level statement                       | masked, no numbering                 | exact                                | MASKED         | yes                                     | `STATEMENT_HASH_VERSION` 3                    |
| statement-align tokens | `matching/statement_align.rs` (own tokenizer)                                                                                         | statement inside a matched function pair  | —                                    | blurred, numbers masked              | —              | no                                      | none                                          |
| vendor content key     | `modules/vendor_content.rs` → `modules.rs` (`factory_structural_hash`)                                                                | CommonJS module                           | slots, non-global free names slotted | match key                            | —              | yes                                     | `FACTORY_HASH_VERSION` 4                      |
| unit hash              | branch `libindex/units.rs` (calls main's `canonical_serialize_privates_blinded`, match key)                                           | statement / declarator + nested functions | slots                                | blurred                              | —              | yes, `#private` names blinded           | `UNITS_VERSION` "2"                           |

### 2.2 Other duplicates

- **Cutting code into pieces (3):** top-level statements (`twins.rs`), the
  branch's declarator/lazy-init units (`graph/references.rs`
  `container_units`), and the function rows of `graph.rs`.
- **Literal extractors (4):** `matching/features.rs` (string literals),
  `modules/vendor_pairing.rs` (its own lexer, ≥5 chars, rarity weighted over
  the prior), branch `libindex/literals.rs` (from the syntax tree, rarity over
  the index), `place/anchor.rs` (rare literals ≥12 chars).
- **Shingle schemes (4):** the cascade's `compute_shingle_set` (call-edge
  n-grams plus `ext:`/`prop:`/`str:` tokens), `statement_align` (k=4),
  `vendor_pairing` (5-token text runs), `twins/role.rs` (delegates to
  `statement_align::content_shingles`).
- **Known-globals lists (2):** `matching/features.rs` keeps its own list;
  `modules/known_globals.rs` is the shared one.
- **Graphs (2):** the function call graph (cascade and its propagation rungs)
  and the branch's statement reference graph (`libdetect/discovery.rs`). The
  binding pass (`matching/alternation.rs`) reads reference identity that
  overlaps the reference graph's read edges.
- **Version stamps (4), independent:** `STATEMENT_HASH_VERSION`,
  `FACTORY_HASH_VERSION`, `UNITS_VERSION`, `LITERALS_VERSION`.

### 2.3 Decision logic

- **Cross-version** (`prior.rs` `run_match_stage` → `matching/cascade.rs`):
  bucket functions by match-key hash → singleton verdict → resolve rungs in
  order (identity resolver, member key, enclosing statement, depth gate,
  callee shapes, CALLER shapes, exact callee hashes, two-hop shapes, shingle
  Jaccard ≥ 0.5, else ambiguous) → `demote_non_injective_matches`,
  `revoke_crossed_containers` → propagation rungs (matched callee, matched
  caller, scope parent, external refs, scope ordinal) → binding cascade
  (`alternation`, 3 rounds) → `resolve_ambiguous_by_ordinal` →
  `assign_interchangeable_pools` → close tier (`matching/close.rs`: cosine
  over 12 counts ≥ 0.8, corroborated by `statement_align`) →
  `assert_prior_looks_like_same_program`. The match list (`MatchMap`) keeps
  JavaScript `Map` insertion order, and the rename stage reads it in that
  order.
- **Vendor modules:** exact factory-hash carry; content-key re-keying;
  declared identity (`modules/identity.rs`); content pairing
  (`vendor_pairing.rs`: shingles plus rarity-weighted literal Jaccard, mutual
  best ≥ 0.3, margin 0.1); whole-file inherit (`finish/vendor_inherit.rs`,
  identity key); `finish/carry.rs`; `twins.rs`; the anchor tier
  (`place/anchor.rs`).
- **Library on main:** `libdetect.rs` decides by path and banner comments
  only; library functions are frozen. No content-based identification.
- **Library on the branch:** `libindex/lookup.rs` claims units (≥30 nodes and
  ≥3 content tokens, or the largest functions' hits cover 50%; 1-3 packages;
  specificity ≤ 4× + 4096 bytes; support within 10 units below 100 nodes;
  literal agreement ≥ 6 literals, ≥ 20 bits, 90% cover, 10-bit margin) →
  families → discovery seeds (declared, banner, index, queued lookup) → graph
  fixpoint (DOWN, MODULE, GLUE) → isolation veto → report only, via
  `--diagnostics`. The index builder also writes module rows that nothing
  reads.

### 2.4 Deliberate vs accidental divergence

| divergence                                                                                                      | verdict                                           | evidence                                                                                                                                                                        |
| --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| statement hash masks property names and keeps literals; the enclosing-statement rung uses the match key instead | **deliberate**                                    | `matching/statement_context.rs` header: "a different equivalence relation for a different consumer" — the split must match code across humanify's own renames of export members |
| vendor key uses the structural hash, not raw content                                                            | **deliberate**                                    | `modules.rs` above `name_cjs_factories`: Bun re-rolls minified names each build                                                                                                 |
| identity key has no wrapper-spelling rule                                                                       | **deliberate**                                    | `serialize.rs` gates the rule on the match key; the identity key must stay byte-exact                                                                                           |
| `features.rs` keeps its own known-globals list                                                                  | **deliberate, but should become a named setting** | `features.rs` header: "THE SET IS THE LOCAL ONE", the TS original's list, part of the 12-count vector's meaning                                                                 |
| `statement_align`'s copies of `identifier_role` and block unwrap                                                | **accidental**                                    | marked KEEP-IN-SYNC with `hash::serialize` — two owners of one rule                                                                                                             |
| `close_dump.rs`'s copy of the vector build                                                                      | **accidental**                                    | KEEP-IN-SYNC with `close::build_vector_map`                                                                                                                                     |
| four literal extractors with different length floors                                                            | **mostly accidental**                             | each threshold was tuned in its own experiment; none documents why the extractor itself differs                                                                                 |
| four shingle schemes                                                                                            | **mixed**                                         | `twins/role.rs` already delegates (merged 2026-09-25); the cascade's edge shingles carry graph content the others do not                                                        |
| two graphs                                                                                                      | **accidental**                                    | built at different stages for different consumers; same reference facts                                                                                                         |
| four independent version stamps                                                                                 | **accidental**                                    | no single "what hash forms did this run use" record                                                                                                                             |

Rule for the refactor: a deliberate difference becomes a **named setting**; an
accidental one becomes **one owner**. Neither changes output.

## 3. Target structure

```
bundle ─► 1 CUTTER ─► 2 SERIALIZER (named forms) ─► 3 EVIDENCE ─► 4 REFERENCE STORE ─► 5 DECISIONS ─► matches
             pieces       fingerprints                literals,      prior | index          ordered rules
             + spans      per form                    shingles,      candidates +           per profile
             + parents                                globals,       labels
                                                      scoped graph
```

### 3.1 One cutter

One pass that yields every piece kind with its byte span and its parent:
module → top-level statement → declarator (one unit per declarator of a
multi-declarator `var`) → function → binding. Lazy-init bodies and CommonJS
factory bodies are cut the same way (the branch already splits factory bodies
into statements on the index side; the misses lane found the two sides must
cut the same way or "can never meet"). Every consumer reads pieces from here:
the cascade's function rows, `twins`, the split's statements, the library
units.

### 3.2 One serializer, forms as named versioned settings

DONE in step 2 as `crates/humanify-core/src/hash/forms.rs` (`CanonicalForm`,
the `FORMS` registry; the as-built names and the three walks that stay three
are in its module doc and `docs/responsibility.md`). The plan as written:

One walk (`hash/serialize.rs` today) with a settings record. Each existing
form becomes a named preset with its own version:

| preset             | settings                                                               |
| ------------------ | ---------------------------------------------------------------------- |
| `match-key/1`      | slots, strings blurred, props kept, wrapper rule on                    |
| `identity-key/1`   | slots, strings exact, props kept, wrapper rule off                     |
| `statement/3`      | names masked unnumbered, props masked, literals exact, wrapper rule on |
| `align-tokens/1`   | token stream, strings blurred, numbers masked                          |
| `vendor-content/4` | match key + non-global free names slotted                              |
| `units/2`          | match key + `#private` blinded                                         |

The **minifier normaliser** (misses lane: `const`/`let`, template spellings,
function declaration vs expression; +6.3 points on Claude Code) is one more
setting, OFF in every existing preset. Turning it on makes a new preset
version (e.g. `units/3`); it never changes an existing one.

### 3.3 One evidence layer

- **one literal extractor** (from the syntax tree; the length floor and
  rarity table are parameters, not separate code);
- **one shingle scheme** (k and token classes as parameters; the cascade's
  edge shingles become a graph-evidence reader, §4);
- **one known-globals list** with the base + per-environment sets
  `modules/known_globals.rs` already has; `features.rs`'s list becomes the
  named set `ts-structural-hash-v1`;
- **one graph** over the cutter's pieces: call and reference edges, each edge
  tagged with its scope relative to a region (§4).

### 3.4 Reference store interface

```
trait ReferenceStore {
    fn candidates(&self, form: Preset, fingerprint: Digest) -> Candidates; // pieces with this print
    fn label(&self, piece: RefId) -> Label;   // prior: names + file; index: package@versions
    fn literals(&self, piece: RefId) -> Option<LiteralSet>;
    fn edges(&self, piece: RefId, scope: Scope) -> Edges;     // INSIDE / OUT-OF only for the index
    fn identity(&self) -> StoreId;            // prior: file sha; index: index id
}
```

- **Prior store:** in memory, built per run from the prior tree (today's
  `match_prior_version` prior side). Labels = names, slot mappings, files.
- **Index store:** on disk, loaded once (3.6 s with literals, 0.9 s without,
  measured on index v2). Labels = package, versions, file. It never stores
  INTO edges: the index cannot know who calls the library in any given app.
- **An index is a set of recipe slices** (Andrew, 2026-10-13). The packing
  tool's recipe — bundler, target, format, minify — changes WHAT gets
  bundled (Bun `--target bun` picks `dist-es` where `--target node` picks
  `dist-cjs`), so every slice is keyed by its recipe. The run picks the
  slice from the toolchain it resolved ONCE by detection (the
  detection-built-pipeline rule: resolve plugin pieces once and pass them
  down; the minifier verdict is untrusted, unsure means Bun). The store is
  handed that one slice; nothing below re-detects. A detected recipe with no
  slice in the index is a loud WARNING plus a diagnostics entry, never a
  silent miss (config doc §3).

### 3.5 Decision layer

An ordered list of **rules** plus **propagation**, chosen per **profile**.
Each rule declares which evidence it may read, including which graph scopes.
Output per decision: `(query piece, reference piece, evidence used, deciding
rule)` — the same trail the `--diagnostics` resolution stats already carry,
now from one place.

- **cross-version profile:** today's rung list in today's order, exactly
  (§2.3). Must be byte-identical.
- **library profile:** size floor, specificity, support, literal agreement,
  family check, isolation veto, containment (§4), then discovery's fixpoint as
  propagation.

A profile's thresholds are not a separate file: they are the `[library]`
section of humanify's ONE config file (`--config <file>`, built-in defaults
compiled in; see [config-consolidation.md](./config-consolidation.md)).

### 3.6 As built in step 3 (2026-10-13)

**The cutter** is `crates/humanify-core/src/cut.rs` (+ `cut/units.rs`):

| piece      | owner                                                                                             | consumers                                                                     |
| ---------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| module     | `container_statements` (AST), `container_statement_values` (ESTree JSON), same rule               | factory classification, discovery, index units; twins/split inventory, naming |
| statement  | `cut::units::container_units` (+ `UnitLocator`, the one position → unit lookup)                   | reference graph, library units (index and bundle side)                        |
| declarator | the same (one unit per declarator of a multi-declarator `var`)                                    | the same                                                                      |
| function   | `function_pieces`, `is_function_piece`, `enclosing_function_piece`, `enclosing_function_node_ids` | function graph rows + scope parents, eval/with taint                          |
| binding    | `container_scope`                                                                                 | function graph's module bindings                                              |

`Cut::new` lays all five out as one `Piece` list (kind, span, parent). It is
the node set of the one graph; pipeline consumers do NOT build it — each
reads its own granularity from the owner functions, computed once as before
(§8's lazy-per-kind rule). `enclosing_statement` (twins' row → statement
assignment) also lives here. Two substrates stay two, as a named view: the
twins' statement hash, the split and the naming plumbing walk the ESTree
JSON, the graphs walk the AST; both select the container by the same rule.

**The graph** is `crates/humanify-core/src/graph/edges.rs`: one `Edge`
(from, to, `EdgeKind` ∈ Call / Read / Write / InitMutation / LazyWrapper,
`scope: Option<EdgeScope>`) and one `CodeGraph` (the cut's pieces + every
edge, joined by span). The two BUILDERS stay two, as named views, because
folding them changes bytes: the function graph attributes a call to EVERY
ancestor function (babel's recursive traverse) and its binding edges follow
the TS 4a/4b/4d rules over babel's `referencePaths` (assignment targets
dropped) — the cascade, propagation and alternation read those rows in
stored order; the reference graph attributes at statement granularity over
oxc's reference flags (writes kept) for discovery's fixpoint.
`UnifiedGraph::edges()` and `ReferenceGraph.edges` both speak `Edge`. The
reference graph's init-time definitional mutations are now their own kind
(`InitMutation`); a pair a plain write already links stays one `Write`
(one edge per pair and class), so the edge count and both adjacency lists
discovery reads are unchanged.

**Region scope.** `EdgeScope { from, to }` stores the two ends' region ids;
`relative_to(region)` answers INSIDE / INTO / OUT-OF (an edge between two
regions is OUT OF one and INTO the other). `Regions` maps laminar spans to
region ids (innermost wins: containment). The producer is library
discovery: `discover_in_bundle` assigns every reference-graph edge its
scope from `library_regions` (one region per claimed package, its claimed
units' spans). `CodeGraph::assign_scopes` does the same over the one graph
when regions are known. **No rule reads scope in step 3.**

**How step 4's rules read scope.** Each library-profile rule declares the
scopes it may read, and the rule list refuses any rule that declares INTO:
identity evidence for region R is the edges with `relative_to(R)` ∈
{Inside, OutOf}, the OUT-OF end abstracted (the callee's package or "a
runtime helper", never its local name). Comparison against the index is
CONTAINMENT: an INSIDE edge of the bundle region absent from the index
entry counts against the match; an index edge absent from the bundle (tree
shaken) counts for nothing. INTO edges are read only by a post-claim
consistency check (they should land on the package's exports). The
cross-version profile keeps reading every edge, as today (vendor INSIDE-only
is step 5, with an eval). Note the two graphs run on different texts in the
pipeline (discovery: the RAW bundle at unpack; the cascade: the fresh
pre-rename text), so a region known on one must be mapped by span through
the unpack before the other can use it — step 4's store owns that map.

### 3.7 As built in step 4 (2026-10-14)

Branch `refactor/matching-reference-store`, four commits on main
`372221a0`: 4a (store + rule lists), the neutrality harness's
`--pipeline-set`, 4b (the library profile reads graph scope), and a perf
fix to 4a's rung interpreter.

**The reference store** is `matching/store.rs`: one `ReferenceStore` trait
(identity, form, candidates for a print, label, literals, edges by scope)
with two implementations. The PRIOR store is `FingerprintIndex` (per run;
the cascade's buckets read it through the trait; a scoped edge read is
refused, since one program has no regions). The INDEX store is
`LibraryIndex`, loaded with ONE recipe slice chosen once in
`LibraryIndex::load`; it refuses INTO and every-edge reads and answers
INSIDE / OUT-OF with `NotRecorded` (this index format has no edge table).
The lookup reads it through the trait. The span map between the raw-bundle
graph (discovery) and the pre-rename graph (cascade) that §3.6 assigned to
the store was NOT needed: no step-4 rule reads a region across the two
texts. It arrives with step 5's vendor INSIDE-only rule.

**The rule lists** are `matching/rules.rs`: `RuleSpec` (name, stage,
`Evidence` incl. graph scope), `Profile::admit` (the library profile
REFUSES a rule that reads INTO or every edge, unless it is a post-claim
`Stage::Check`), `RuleList` (built only through `push`) and
`RuleList::decide`, the ONE constructor of a `Decision` (query piece,
reference piece, deciding rule, what it reads).

- **Cross-version profile** (`cross_version()`): today's rungs in today's
  order. `cascade::resolve_match` EXECUTES the resolve rungs off
  `rules::RESOLVE` (each rung a Match / Park / Narrow / Pass step); the park
  pools, the one insertion point and the single calleeHashes+twoHopShapes
  rung are as before, so the match list's order is unchanged.
  `MatchResult::decisions` is the trail. The interpreter borrows the
  candidate pool (`Cow`) until a rung narrows it: 4a's first version copied
  every pool up front and cost +1.9 s on match:functions (8.3 → 10.2 s);
  the fix brings it back to 8.1 s, match list digest identical.
- **Library profile** (`library()`): the index lookup rules and discovery's
  fixpoint (`libdetect::discovery::LibraryProfile`, every rule admitted by
  `Profile::admit`). Each rule reads the graph through `RoundView`, which
  serves only the scopes that rule declared. DOWN reads INSIDE edges only
  (a unit whose LIBRARY callers are all one package is that package's; the
  app's callers are not read); MODULE dropped its outside-caller (INTO)
  guard. Only the post-claim `boundary` check reads INTO
  (`packages[].intoEdges` / `intoUnits` in the report), and it decides
  nothing.
- **Two guards came with the scope rule**, because without the app's
  callers DOWN reaches further. **Containment** with the index's unit
  table (`discovery::Containment::contradicts`): a propagated unit at or
  over the index's row floor (`libindex::units::UNIT_ROW_MIN_NODES`, 20
  nodes) that no print of the claimed package's family holds is refused
  (bundled ⊂ built). **Ambiguous seeds cannot vouch for themselves**: in
  the isolation veto, a seed the structural rule would accept for more
  than one family (`LibraryIndex::structural_owners`) is not supported by a
  neighbour that was claimed only through that seed's own propagation.
- **The §4 scope fixture** (`libdetect/scope_fixture_test.rs`, written
  first and red): one library bundled into two apps that call it
  differently identifies the same, piece for piece, against an index built
  from the library alone; a probe rule declaring INTO is refused, and with
  the refusal removed the probe runs and the apps disagree.

**Gates (final head `6816607a`).** `npm run check`: all 13 stages passed.
Warm neutrality vs `372221a0` on 2.1.215→2.1.216: 0 files / 0 lines, both
legs +0 cache writes, with the default settings AND with
`library.matching = "report"` plus the step-4 index (the only differing
artifact is the diagnostics-only `library-report.json`). Pipeline output
did not move.

**Library score** (exp096, index `/work/exp097-data/ix/step4`, the same
index as exp097's step-4 labels; labels `step4f-agreement` /
`step4f-structural` vs `exp097-s4-*`; band 0, so every delta is real;
recall / wrong family, origin-aware truth):

| set (rule)                    | before       | after        | app claimed |
| ----------------------------- | ------------ | ------------ | ----------- |
| Claude Code 2.1.216 (struct.) | 67.1% / 0.0% | 70.6% / 0.0% | 0 → 0       |
| held-out, 67 apps (struct.)   | 20.7% / 5.1% | 23.9% / 3.9% | 0 → 0       |
| task-master-ai (struct.)      | 38.5% / 8.2% | 40.5% / 2.1% | 0 → 0       |
| wrangler (struct.)            | 7.6% / 5.9%  | 14.6% / 4.9% | 0 → 0       |
| Claude Code 2.1.216 (agree.)  | 41.9% / 0.0% | 61.9% / 0.0% | 0 → 0       |
| held-out, 67 apps (agree.)    | 12.9% / 7.2% | 18.5% / 3.9% | 0 → 0       |
| task-master-ai (agree.)       | 22.8% / 4.0% | 28.3% / 1.0% | 0 → 0       |
| wrangler (agree.)             | 6.6% / 0.0%  | 10.4% / 0.0% | 0 → 0       |

Read from the claims that changed (statement bytes, per unit):

- **Recall rises through the scope rule.** New DOWN / MODULE claims the
  app's callers used to block: highlight.js in codex (709 KB, MODULE),
  @modelcontextprotocol/sdk in Claude Code, node-fetch / zod / MCP SDK in
  task-master-ai, @aws-sdk/client-sts and yargs-parser in wrangler. The
  isolation veto counts a same-family neighbour as support, and DOWN now
  claims those neighbours, so seeds it used to drop survive: Claude Code's
  vetoed bytes under agreement fell 161 KB → 17 KB (zod's seeds, the main
  part of agreement's +20.0 points; its "guard" miss fell 21.4% → 5.8%).
- **Wrong family falls through containment.** The largest wrong claims
  before were propagated units the index's build of that package cannot
  hold: `web-streams-polyfill` over @google/genai's shared TypeScript
  helpers (126 KB in task-master-ai, 155 KB in held-out task-master-ai),
  zod-to-json-schema over @ai-sdk/provider-utils in tidgi (31 KB),
  content-type over the MCP core in task-master-ai (14 KB). Containment also drops some RIGHT claims (held-out ~174 KB, e.g.
  markdown-it in renovate, 36 KB; @aws-sdk/client-s3 in wrangler, 5 KB): a
  unit the bundle has and the indexed build does not print the same way.
- **New wrong family from the scope rule**, which the guards do not catch:
  uglify-js claimed over terser in html-minifier-terser (115 KB, DOWN:
  terser is uglify's fork and shares its prints), @hono/node-server over
  MCP code (19 KB in each task-master-ai), parse5 over entities (56 KB
  across held-out apps), zod-to-json-schema over @ai-sdk/provider-utils in
  the esbuild task-master-ai (7 KB, MODULE).
  The isolation veto now drops @inquirer/figures seeds over `figures`
  (~6 KB in each of three apps) and confbox over jsonc-parser in wrangler.
- Claude Code under structural loses 0.9 KB of sample bytes
  (@aws-sdk/client-bedrock, a DOWN claim no longer reached) and gains
  12.5 KB (MCP SDK DOWN claims, p-map and client-bedrock-runtime seeds).

The ~0.2% wrong-name bar is still unmet on held-out (3.8-3.9%),
task-master-ai (1.0-2.1%) and structural wrangler (4.9%); Claude Code and
agreement-wrangler stay at 0. App code claimed: 0 bytes on every set,
both rules.

## 4. Graph evidence has a scope

Andrew's constraint, 2026-10-13. A region is a set of pieces we think are one
library (or one module). Every graph edge touching it is one of:

| scope      | what it is                                          | library identification                                                                                        | cross-version                 |
| ---------- | --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ----------------------------- |
| **INSIDE** | both ends in the region                             | identity evidence                                                                                             | identity evidence             |
| **INTO**   | caller outside, callee inside                       | **never** identity evidence; only a consistency check                                                         | app code: all edges, as today |
| **OUT-OF** | region calls another package or the bundler runtime | abstract form only (e.g. "calls a runtime helper", "calls package X's export"), never the caller's local name | as today                      |

- **Why.** Two apps that use lodash call it from different places. Anything
  read from INTO edges describes the app, not lodash, and the index cannot
  contain it.
- **Containment, not equality.** Tree-shaking drops unused functions, so a
  bundled region is usually a SUBSET of the index build. Only an INSIDE edge
  the index does NOT have counts against a match; an index edge missing from
  the bundle counts for nothing.
- **The boundary as a consistency check.** INTO edges may confirm a claim
  after it is made: they should land on the package's public entry points
  (its exports). An INTO edge landing deep inside private code is a reason to
  look again (wrong region boundary, vendored copy), never a reason to accept.
- **Vendor code in cross-version.** Today the cascade's caller rungs read app
  callers for vendor functions too. Restricting vendor regions to INSIDE
  evidence is arguably more correct (an app edit should not unmatch an
  unchanged library), but it is a **behaviour change**: step 5, with an eval.
  It is not part of the byte-identical refactor.
- **Test fixture (written in step 4, red first: `libdetect/scope_fixture_test.rs`).** One small library (a few
  functions calling each other, one exported entry), bundled into two
  different apps that call it from different places and different numbers of
  times. Build a two-entry index from the library alone. Required: both apps'
  library regions identify as the same package with the same per-piece
  matches. Then add a probe rule that reads an INTO edge to the library
  profile: the test must FAIL (the profile refuses a rule that declares INTO
  scope, or the two apps' results differ). The test proves the scope tags are
  enforced, not just present.

## 5. Output-side unification (what does not change)

Cross-version outputs stay where they are: the match list (prior → fresh),
names via slot mapping, `statement_align` body-local transfers, binding
renames, twin inheritance, the placement ledger, vendor module/file carry.
Library outputs stay report-only (`--diagnostics`' `libraryDiscovery`) until
step 5. The `is_library` flag and function freezing on main keep their
path/banner detector.

## 6. Names from the index (future, step 5)

Index units are built from published packages. Where a package ships
**unminified** source (most do, in `dist/` or `lib/`), the index unit's slot
mapping carries the library author's own local names. An exact unit match
gives the same slot-to-slot correspondence a prior-release match gives, so
the names could transfer the same way a prior match transfers them — no LLM
ask, and stable across releases because the library's names do not change.
Open points: which build of a package is "unminified" (the index builds are
minified today; it needs a second, unminified print per unit), whether to use
names only on agreement-backed claims (the index-v2 README recommends
agreement for anything that names code: 4.8% vs 12.5% wrong family), and how
this interacts with a prior release's names (prior should win, for
stability). Needs an eval.

## 7. Refactor order and gates

Each of steps 1-4 is its own branch and merge. Gate for each: `npm run check`
green (run bare), then WARM neutrality on 2.1.215→2.1.216:

```bash
experiments/lib/neutrality.sh <base> 2.1.215:2.1.216 --cache /work/neutrality-cache
```

with **0 differing files, 0 differing lines, and the baseline leg +0 cache
writes**. A cold run is invalid (CLAUDE.md).

1. **Shared helpers.** One literal extractor, one shingle function, one
   known-globals module (with the `features.rs` list as a named set), the
   KEEP-IN-SYNC copies in `statement_align` and `close_dump` deleted in favour
   of their owners. Rows in `docs/responsibility.md` for each owner.
2. **Hash forms as settings.** The serializer takes a preset; every current
   caller passes the preset matching today's form. Add one version registry
   that lists all presets and their versions (replacing four loose
   constants). Gate addition: an **equality test per preset on a real
   corpus** — every function/statement/module of Claude Code 2.1.216 hashed
   by old and new code, digests identical — committed as a test that reads a
   small frozen sample plus a one-off full-corpus run recorded in the PR.
3. **One cutter + one graph.** The cutter feeds every consumer; the graph
   carries scope tags (unused by any rule yet). Perf check against
   `docs/perf-inventory.md` (fresh 2.1.182: ≤ 368 s, ≤ 8.35 GB; with prior
   2.1.213: no regression) on top of neutrality.
4. **Reference store + rule list. BUILT (§3.7).** The cross-version
   cascade moved behind the prior store and its rule list unchanged (rung
   order, `MatchMap` order kept). The exp095 library code was already on
   main (`587e211f`, OFF by default, report-only); step 4 made it the
   second profile, reading graph scope (§4) with the scope fixture written
   first. The one library setting landed in config sweeps 5-6 as
   `[library] matching = "off" | "report"` (flag `--library-matching`) plus
   `unnamed = "leave" | "model"` replacing `--skip-libraries`. Warm
   neutrality with the default AND with `matching = "report"` and an index:
   0/0, both legs +0 (the second run needs the harness's
   `--pipeline-set`, which step 4 added).
5. **Behaviour changes, on experiment branches with evals:** INSIDE-only
   evidence for vendor regions in cross-version; the minifier normaliser
   preset; names from the index; any library result that renames, moves or
   freezes code.

## 8. Risks

- **The match list's order is load-bearing.** `MatchMap` keeps JS `Map`
  order and the rename stage applies renames in that order; a rule list that
  emits matches in a different order changes names even with the same
  matches. Step 4 must preserve emission order, and the neutrality run is
  what proves it.
- **Version gates.** Merging four stamps into one registry must not change
  any stamp's value: index shards and the prior-tree ledger are keyed by
  them. A stamp change invalidates every index (and should — the loader
  refuses a mismatch, config doc §4).
- **Performance.** The perf arc took a fresh run from ~94 GB / 623 s to
  8.35 GB / 368 s, byte-identical. A shared cutter that materialises every
  piece kind for every consumer could undo that; pieces should be computed
  lazily per kind and shared, not eagerly for all.
- **Deliberate differences erased by accident.** The statement hash's masked
  property names are what let the split match across humanify's own export
  renames. Step 2's per-preset equality test is the guard.
- **Library profile reaching app code.** Report-only on main until an eval
  says otherwise; the index-v2 numbers (0 app units claimed on Claude Code
  under both rules) are lab numbers on three releases, not a guarantee.
