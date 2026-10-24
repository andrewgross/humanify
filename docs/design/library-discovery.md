# Library discovery: finding library code in `src/` from bundler facts, declarations and the reference graph

STATUS: PHASE 1 (REPORT-ONLY) IN CORE, 2026-10-11 (index lookup 2026-10-12), branch
`exp095-discovery-core` (exp095; not merged). The graph (§4) is
`humanify_core::graph::references`, discovery (§5, §7) is
`humanify_core::libdetect::discovery`, and a run with `--diagnostics` writes
it as `libraryDiscovery`. Nothing is named, moved or renamed by it. The
in-run seeds are the declared readers, kept banners and, with
`--library-index`, the index's statement-unit claims; no shipping index
with statement units exists yet (the lab built three).

- **The core reproduces the lab prototype** on Claude Code 2.1.216 with the
  same seeds (declared + Step B index C, the prototype's veto): 12,255 of
  12,256 claimed units shared, 93.65% estimated library recall on both, 0
  sampled app units, the fixpoint +64.2 KB over 8 rounds (prototype +63.8
  KB), 155 seeds vetoed (prototype 155), library class 116 KB (117 KB). Every
  difference is explained in `experiments/095-library-fingerprints/discovery/README.md`.
- **One change from §7 guard 4, measured:** the isolation veto now judges a
  seed's support on the fixpoint WITHOUT the MODULE rule. On 2.1.86 the
  prototype's rule kept the ink fork: MODULE filled 4.5 KB of Claude Code's
  own keypress parser around the fork's two kept tables, and that fill then
  counted as the tables' support. The new rule drops all of it, and costs
  ~2.5 KB of real library claims per release.
- **In-run index lookup (2026-10-12).** `--library-index <dir>` looks every
  statement unit up (`libindex::lookup`: the lab's Step B S1, specificity,
  literal agreement by default) and runs a targeted lookup of the queue.
  The index gained a statement-unit table built by the builder's own code.
  On 2.1.216 with Step B's index C: 81.0% estimated library recall after
  the fixpoint with the structural rule (lab S1 + fixpoint: 85.8%), 54.5%
  with agreement; 0 app either way. Details and every gap explained:
  discovery README, Results §5.
- The design and lab numbers below are unchanged; they come from one bundle
  (Claude Code 2.1.216), one random sample (620 statements) and 37
  non-library traps. Prototype, core lab binary and re-run commands:
  `experiments/095-library-fingerprints/discovery/`.

**Why.** About 14.5% of Claude Code's `src/` bytes are library code that the
bundler hoisted out of any module boundary (`negatives/README.md`: 2.2 MB,
CI 1.7–2.8 MB). Today that code is named by the model as if it were app
code, and it adds to the cross-version diff as if it were app change. Andrew's
direction (2026-10-11):

1. find the easy cases from what the bundler and the packages left in the
   code;
2. then use the reference graph to find self-contained regions that only
   point one way relative to the app;
3. name each region from the best evidence available;
4. repeat, working from the leaves inward, the way the naming waves do.

## Totals first

- **Three kinds of candidate source, all plugin pieces.** Each runs once per
  run and hands its result down:
  - **bundler facts** (the toolchain's wrapper grammar, bundle layout and
    runtime helpers);
  - **general facts** (the package readers, stop-lists and package
    families);
  - **the reference graph**: every top-level statement, linked to the
    statements whose bindings it reads or defines.
- **A "closed" region is exact and cheap to compute, but on its own it is
  not evidence of a library.** Every statement's downward closure (the
  statement plus everything it transitively references) is closed by
  construction: nothing in it points outside it, except to globals, runtime
  helpers or code already identified. So "closed and one-directional"
  describes about half of all app code too.
  - Measured with no seeds, at a 64 KB closure cap: closed regions hold
    **82% of library bytes and 5.5 MB of app bytes**. Libraries are 25% of
    candidate bytes against a 14.5% base rate, so closure alone enriches by
    only 1.7×.
  - The criterion is **never too strict about direction**: 100% of library
    bytes lie in closed regions once the cap allows the library's size.
    Library code that takes app callbacks is still closed, because a
    callback is a value passed in at run time, not a reference in the code.
  - It is **far too loose about what it admits**. The largest part of the
    app bytes it admits is pure data (bundled docs, prompts, tables). Then
    come leaf helpers, app zod schemas and fork pieces: 22–32 of the 37
    non-library traps are in closed regions.
- **What the graph is good for is extending and checking claims made by real
  evidence.** Starting from the declared readers plus Step B's exact-version
  claims (85.7% of library bytes, an estimate):
  - **The fixpoint added 63.8 KB of `src/` code** over **8 rounds**: 43 KB
    in round 1, then 0–9 KB per round. Every sampled unit it touched
    (10/10) is library code of the right family.
  - **Recall rose from 85.7% to 93.7%.** That is an estimate from the
    sample and it is inflated, because 4 of the 10 gained units sit in a
    heavily weighted stratum. The exact gain is 63.8 KB, about 3% of the
    2.2 MB library estimate.
  - **One extra rule, the library-class closure,** marks a further 117 KB of
    code that identified libraries depend on. All 9 sampled units in it are
    library code, with 0 app units. The package is left unnamed.
- **Two guards come from the graph, and both measured clean on this bundle.**
  1. **Isolation veto.** An index claim with no graph neighbour of the same
     package family is dropped. This removed every claim on Claude Code's
     ink fork, and it costs 15.5 KB of claims, ≈0 recall.
  2. **Content floor before any closure.** A content-free claim such as
     `var mLl={}` (Step B called an app module's namespace object
     `@smithy/node-http-handler`) owns a whole app module through the
     statement that fills it. With the floor, the library-class closure
     claims no app code; without it, 119 KB (an estimate) of app code.
- **Recommendation:**
  1. Build the statement reference graph as one owner, next to the
     function graph.
  2. Run discovery **report-only**: seeds, then the fixpoint, then the
     guards, then a candidate queue for the index.
  3. Only then let it feed naming (as hints, then as library-freeze
     regions), and move code to `vendor/` last.

## 1. What exists today

| Piece                                              | What it does now                                                                                                                                     | Where                                                         |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| CommonJS factories                                 | Exact boundaries. Moved to `vendor/` and named (vendor ladder, P5/P17).                                                                              | `modules::classify_bun_modules`, unpack                       |
| Lazy ES modules (`var I=b(()=>{…})`)               | The end marker is known, and module naming reads it (`naming::module_names`). Library lazy modules stay in `src/`.                                   | `place::method::markers_describe_bundle`, P9                  |
| Declared readers on app code                       | `identity::scan_app_declarations`. Report-only: `--diagnostics appDeclaredLibraries` (package.json, scoped error prefix, VERSION, AWS client pairs). | P17                                                           |
| Step B (index claims on `src/` statements)         | Lab only: 81.5% recall with exact-version builds and 0 false claims on sampled app bytes. Outside the sample it claimed 383 B of the ink fork.       | `negatives/scripts/stepb_cc.py`                               |
| Banner regions, library freeze                     | Functions inside a `/*!`-delimited region are library code and are not renamed.                                                                      | `libdetect::function_carry` (`library_at_offset`, the freeze) |
| Function graph (naming order)                      | Calls only, function granularity, built on the unpacked app file. Wave N is every node whose dependencies settled in earlier waves.                  | `graph.rs` (`internal_callees`), `naming::waves`              |
| Load-time reads and writes per top-level statement | Name-based, load-time only (function bodies excluded). Identifies the export registrar by SHAPE.                                                     | `emit::load_order` (`identify_export_registrar`)              |

The gap is that nothing names or groups library code that sits in `src/`.
There are claims (Step B) and declarations, but no region around them and
no way to grow from them.

## 2. Words used here

- **Unit**: one top-level statement of the app code, or one statement inside
  a lazy-module init body. These are the units of the exp095 lab
  (`raw.jsonl`). A CommonJS factory is one unit.
- **Edge u → v**: unit `u` references a binding that unit `v` declares or
  defines (§4).
- **External**: the bundler runtime helpers, code already in `vendor/`, and
  every unit already identified as library code. The graph never walks
  past an external.
- **Closure D(v)**: `v` plus everything reachable from it without passing an
  external.
- **Closed region**: a set R where every edge leaving R lands on an external.
  D(v) is the smallest closed region that contains v.
- **Claim**: "unit u is package P", with the evidence kind and the rule that
  made it.

## 3. Candidate sources: plugin pieces

Every source is chosen once by `toolchain::resolve_toolchain` and passed
down as a value. No stage compares bundler names (pipeline-stages.md).

### (a) Bundler facts (bundler-specific; one implementation per bundler)

| Fact                      | Bun                                                                                                                                                                                                                                                                    | esbuild                                              | Slot                                                      |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------- |
| CommonJS boundary         | exact                                                                                                                                                                                                                                                                  | exact                                                | `ModuleWrapperGrammar` (P3)                               |
| Lazy ES module boundary   | **End** exact (`b(()=>{…})`). **Start** exact only when there is a namespace block (`var N={};rt(N,{…})`); otherwise it is a guess. The module owns its hoisted functions and its `var a,b;`. A module with nothing to run gets **no marker** (10.8% of lazy modules). | Every wrapped ES module has a marker.                | `BundleLayout` (P9)                                       |
| Eager ES module           | no boundary                                                                                                                                                                                                                                                            | no boundary                                          | —                                                         |
| Module-to-module edges    | `A(X(),1)`, `(I(),In(N))`, `Promise.resolve().then(()=>(I(),N))`, `Oi(…)`                                                                                                                                                                                              | same shapes                                          | `InteropHelpers` (P8)                                     |
| Runtime helpers           | Recognised by shape (`__commonJS`, `__esm`, `__toESM`, `__toCommonJS`, `__export`, `__reExport`, React Compiler sentinels). **Externals, and vetoed as candidates**: Step B labelled 35–79% of runtime bytes as library code.                                          | same helpers, without Bun's setter or WeakMap caches | P7 sibling: the never-rename lists; P12 load-order shapes |
| Namespace-filling helpers | `__export` (`rt`) and `__reExport` (`Oi`). They define their first argument (§4).                                                                                                                                                                                      | same                                                 | `emit::load_order::identify_export_registrar` (by shape)  |
| Kept identity             | `/*!` banners in place; inlined `__dirname` and paths                                                                                                                                                                                                                  | wrapper keys (unminified), end-of-file licence block | `ModuleIdentitySource` (P17)                              |

**Not used: esbuild's per-module path comments** (Andrew, 2026-10-12).
Unminified esbuild output writes `// node_modules/<pkg>/<file>` before
each module's code, which would name every hoisted library module. They
vanish under `--minify-whitespace`, so on every fully minified bundle, and
the pipeline will not rely on them: no reader is built for them. **For
minified bundles the design is the index plus the graph only.**

What this source yields:

- **boundaries**, each tagged `exact`, `end-only` or `none`;
- **edges between modules**;
- **the runtime veto**.

A lazy module whose boundary is exact is the natural candidate region
around any claim inside it. That is what rule MODULE (§5) uses.

### (b) General facts (bundler-independent)

- **The package readers** `modules::identity::readers::IDENTITY_READERS`
  (P17), run over app units by `scan_app_declarations`:

  - package.json, scoped error prefix, the VERSION constant (version only,
    names nothing), AWS client pairs (a hint);
  - banners, read from the bundler's kept comments.

  Declarations are the only **version oracle**. They are exempt from the
  isolation veto (§7), because a declaration is its own evidence.

- **Stop-lists and package families** (`crates/humanify-index/data/`):
  compiler helpers, Stainless templates, `@smithy/*` with `@aws-sdk/*`,
  `@azure/msal-*`, …. Families decide what counts as the "same package" in
  every rule below.

### (c) The reference graph (bundler-independent, with two bundler inputs)

This is a statement-level graph over the app code: nodes are units, and
edges are resolved references (§4).

It takes two inputs from (a):

- which helpers are runtime (externals);
- which helpers fill a namespace (edges that define).

It is cheap. On the 20 MB bundle, parsing, scoping and building the graph
takes **1.5 s** (oxc semantic): 79,911 nodes and 222,172 edges. Run twice,
the edge list is byte-identical.

## 4. The graph, precisely

**Nodes.**

- Every top-level statement of the app code.
- Every statement of a lazy-module init body.
- Each lazy-module wrapper, as its own node. It owns only the wrapper
  overhead, and has an edge to each of its body statements, because calling
  the init runs them.
- A CommonJS factory is one node, and it is external (it is already
  vendor).

**What makes an edge u → v.** For every binding resolved by scope analysis,
where `v` is the unit that declares it:

| Reference in `u`                                                                                                                                                                                                                    | Edge                                                                                                                                                                                                                                                                           |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Read: a call, a property read, passing it as an argument, `typeof`, …                                                                                                                                                               | u → the declaring unit **and** u → every unit that WRITES the binding. In a Bun lazy module the real definer of a hoisted `var x` is the body statement `x=…`.                                                                                                                 |
| Write (`x=…`, `x++`, destructuring target)                                                                                                                                                                                          | u ↔ the declaring unit (both ways): a binding and the statements that assign it are one thing.                                                                                                                                                                                |
| **Definitional mutation** that runs at module-init time (not inside a function of `u`): `x.a=…`, `x.prototype.f=…`, `Object.defineProperty / defineProperties / assign(x,…)`, or a namespace-filling helper `rt(x,{…})` / `Oi(x,…)` | Counts as a WRITE of `x`, so it links both ways. Without this rule, a namespace's readers reach only the empty `var N={}` and never the module behind it: an app zod schema's closure missed zod entirely (1.2 KB instead of 250 KB). There are 877 such mutations in 2.1.216. |
| The same mutation **inside a function** (run-time configuration: `axios.defaults.x=…`, installing a hook)                                                                                                                           | A read only (u → x). App code configuring a library does not make the library depend on the app.                                                                                                                                                                               |
| An unresolved name (a global, `require`, `process`)                                                                                                                                                                                 | No edge.                                                                                                                                                                                                                                                                       |
| `this`, a callback passed in, a value returned out                                                                                                                                                                                  | No edge. A library that holds an app callback has a reference to a _value_, not to app _code_. This is why direction is never too strict (§9): 100% of library bytes lie in closed regions.                                                                                    |
| Re-export glue (`__reExport`, a namespace block, `export *` resolved at build time)                                                                                                                                                 | Defines its target (row 3), so the glue belongs to the module it fills, and its readers reach that module.                                                                                                                                                                     |

**Shared mutable state.**

- A binding written at init time by two regions links them both ways, so
  neither is closed without the other. That is correct: code that shares
  state cannot be separated.
- An app statement that, at init time, patches a library object
  (`Lib.prototype.x=…`) puts the library and the app into one region. A
  patched library is no longer the published package. This is the fork
  case again, and here the graph refuses it on its own.

**Direction.** For a closed region R, every edge between R and the rest goes
from the rest into R. That is what "called in one direction" means, and it
holds by construction. An edge from R out to unidentified code means R is
not closed. Both forms of the property are checked:

- **closure**: nothing reaches out;
- **isolation**: an index claim must be connected to its own package's code
  (§7).

**The theorem that sets expectations.** D(v) is closed for **every** v. So
"is there a closed region around this code?" is always yes. The useful
questions are about **which** region (its size and its boundary) and **what
evidence names it**. The graph answers the first question; it never
answers the second.

**One owner.** The naming graph (`graph.rs`) is calls only, at function
granularity, on the unpacked app file. Library discovery needs every
reference, at statement granularity. Both resolve bindings through the
same oxc `Semantic`.

- Proposal: a `graph::references` builder (statement × binding × read,
  write or definitional-mutation). `internal_callees` becomes its
  call-only, function-level projection.
- That is one owner for "what does this code reference", with two views.
  Add the row to `docs/responsibility.md` when it lands.
- The load-order analyzer stays separate. It answers a different question
  (load time only, by name) and owns the export-registrar shape that this
  graph reuses.

## 5. The leaf-inward fixpoint

**The state:**

- X, the externals;
- the claims, `unit → package`;
- for each claim, its rule and round.

**Round 0, seeds:**

1. Declared readers (b): package.json, scoped error prefix, AWS pairs as a
   family-level claim, and banners.
2. Index claims: Step B structural matches, and later the index's
   structural AND literal agreement with the specificity rule (scale lane).
   Each must pass the stop-lists and the content floor.

**Each round** applies these rules to every unidentified unit. A unit
claimed in round k becomes external for round k+1.

| Rule                                | Condition                                                                                                                                                                                                                                                       | Names                                                     |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| **GLUE**                            | A lazy-module wrapper whose body statements are all claimed as package P.                                                                                                                                                                                       | P (bundler glue)                                          |
| **DOWN** (private code)             | Every caller (in-edge) of the unit is claimed as ONE package P.                                                                                                                                                                                                 | P                                                         |
| **MODULE** (closure-guarded fill)   | The unit is in a lazy module whose claims are all P, and its closure stays inside that module or X, **and** no unidentified code outside the module calls it. App code may call into the region only through entry points that already have their own evidence. | P                                                         |
| **DOWN-CLASS** (after the fixpoint) | Everything reachable from a content-bearing claim (≥ 30 bytes) over unidentified code: code that library code depends on.                                                                                                                                       | library class, package unknown ("dependency of P")        |
| **UP queue** (no name)              | Closed regions of ≤ 256 KB that reference an identified library: the next layer out ("code that calls only zod and itself").                                                                                                                                    | nothing. They are queued for an index lookup with context |

These are iterated until no rule fires.

**Relation to the naming waves.**

- The naming waves are the same shape: wave N is every node whose
  dependencies settled earlier, and naming goes leaf first.
- The fixpoint differs in what a round settles. A wave settles a _name_ for
  each node whose callees are settled. A discovery round settles an
  _identity_ for each unit whose surroundings (callers for DOWN, module for
  MODULE) are already identified.
- Both are monotone and deterministic, and both are rounds over the SCC
  condensation of the same reference resolution. The reusable machinery is
  the graph builder (§4), not the wave scheduler: discovery has no model
  calls to batch.

**The order in which evidence names a region, best first:**

1. **Declared identity** in its own text (P17 readers). This gives package
   and version.
2. **Index match** (assume a big index). This needs structural AND literal
   agreement, plus specificity: the matching index file at most ~3–4× the
   query. This combination stays at ~0.2% wrong from 753 packages up. The
   index matches _units_; the graph supplies the _region_.
3. **Graph extension of an existing claim** (GLUE, DOWN, MODULE). This never
   names a region that has no claim inside or above it.
4. **Library class without a package** (DOWN-CLASS). This keeps the code out
   of app _naming_, if that is ever wanted, and never moves it.
5. **Otherwise: nothing.** An UP candidate the index cannot name is **not**
   called an "unknown library". Closure is not evidence (§3c). Marking
   those 6 MB would hide app code.

**Region refinement against an index match** (the region boundaries are
fuzzy):

- **Grow** only along the rules above: down to private code, within an
  exact module boundary, to glue. Never upward: callers of a library are
  the app, until a claim of their own says otherwise.
- **Shrink**:
  - drop claims on content-free units (the content floor);
  - drop index claims that end the fixpoint isolated from their own family
    (the isolation veto, §7);
  - refuse a claim whose closure reaches unidentified code that has its
    own unidentified callers (the library would be calling the app);
  - when the index names a package's units, the region is cut to those
    units plus their private closure. Hoisted neighbours that are not in
    the closure stay unclaimed.
- **Refuse**:
  - two packages claimed in one module (outside a family);
  - a claim inside the bundler runtime;
  - any rule whose seed was vetoed (repeat the fixpoint after a veto: a
    dropped seed can strand what it supported).

## 6. What it measured (CC 2.1.216, lab)

Scored against `negatives/labels.jsonl`: 620 statements in strata S0–S3,
byte-weighted with the stratified ratio estimator, where library = 2,221 KB
and app = 13,036 KB of `src/`. Also scored against the 36 non-library
`traps.jsonl` units (the 38 traps minus the one labelled library).

### 6.1 Candidates from closure alone (no seeds)

| Closure cap | Library bytes in closed regions | App bytes | Library share of candidates | Traps inside |
| ----------- | ------------------------------- | --------- | --------------------------- | ------------ |
| 4 KB        | 1,024 KB (46%)                  | 2,898 KB  | 26%                         | 22           |
| 64 KB       | 1,823 KB (82%)                  | 5,499 KB  | 25%                         | 26           |
| 256 KB      | 2,157 KB (97%)                  | 6,989 KB  | 24%                         | 30           |
| 1 MB        | 2,221 KB (100%)                 | 8,154 KB  | 21%                         | 32           |

- **Requiring at least one function in the region** drops pure data
  (bundled docs, prompts and tables are the largest app units admitted). At
  64 KB that gives 1,613 KB of library against 3,271 KB of app: 33% library.
- Still, 22–32 of the 37 non-library traps are in closed regions: the edit-distance
  function, the transcript normalisers, app zod schemas, ink-fork pieces,
  the yoga port.
- **Verdict:** closure is a region shape and a precondition, not a
  detector. It is too loose, and never too strict about direction.

### 6.2 The fixpoint from evidence

These are estimated library recall figures, with exact `src/` bytes added
in brackets. The rules are DOWN + MODULE + GLUE.

| Seeds                                      | Seed recall | After fixpoint           | Rounds (bytes per round)                                   | App claimed (sample)                            | Traps claimed             |
| ------------------------------------------ | ----------- | ------------------------ | ---------------------------------------------------------- | ----------------------------------------------- | ------------------------- |
| declared readers only                      | 9.3%        | 9.4% (+36 KB)            | 4 (26 K, 8.6 K, 1.4 K, 0.2 K)                              | 0                                               | 0                         |
| Step B, lab index only (index A)           | 43.1%       | 44.2% (+37 KB)           | 3                                                          | 0                                               | ink fork ×2, tslib ×2     |
| Step B, exact versions (index C)           | 81.5%       | 89.4% (+67 KB)           | 8                                                          | 0                                               | ink fork ×2, tslib ×2     |
| declared + index C                         | 85.7%       | 93.7% (+66 KB)           | 8 (45 K, 8.5 K, 1.2 K, 2.2 K, 2.0 K, 3.5 K, 3.3 K, <0.1 K) | 0                                               | ink fork ×2, tslib ×2     |
| **declared + index C + isolation veto**    | **85.7%**   | **93.7% (+64 KB)**       | 8                                                          | **0**                                           | **tslib ×2 only**         |
| same, MODULE **without** the closure guard | 85.7%       | 92.4% (+127 KB)          | 2                                                          | **1 unit, 8.5 KB est.** (an app promise helper) | tslib ×2                  |
| + DOWN-CLASS from claims ≥ 30 B            | —           | +117 KB of library class | —                                                          | 0 (9/9 sampled units library)                   | lodash helper (a library) |
| + DOWN-CLASS from ALL claims               | —           | +236 KB                  | —                                                          | **6 units, 119 KB est.**                        | —                         |

- **Does the fixpoint grow?** Yes, but by little:
  - round 1 carries two thirds of the bytes (43 of 64 KB);
  - rounds 2–8 add 1–9 KB each;
  - the rounds continue down chains of private helpers (AWS clients,
    `@azure/identity`, mcpb).
- **The graph cannot stand in for the index.** With the weaker index A it
  adds 1.1 points; with index C it adds 8 (estimated). Most of what Step B
  misses sits in closed regions next to the app's own leaf code. After the
  fixpoint, 100% of the remaining library bytes are in candidates ≤ 64 KB,
  but so are 6 MB of app bytes. Naming them needs the index.
- **The recall estimate is noisy where it counts.** The +8 points come from
  10 sampled units, and 4 of them are in the S2 stratum, where each sampled
  byte stands for ~11. The exact figure is the bytes column: +64 KB is
  about +3% of the library estimate.
- **The UP queue after the fixpoint** holds 399–436 KB of `src/`: about
  113–146 KB of library against 232 KB of app (estimates), ≈ 33–39%
  library. That is 2.5× the base rate. It is a good queue for the index,
  but not something to name on its own.
- **The two tslib claims** (`__classPrivateFieldGet` named after the
  Stainless SDK that carries it) are the stop-list's job, not the graph's.

### 6.3 The guards on the measured failure modes

- **App forks of libraries (the ink fork).**
  - The fork keeps the library's leaf tables, so the index matches them. It
    rewrites everything around them, so a kept table's only callers are app
    code.
  - **The isolation veto** (an index claim with no caller or callee claimed
    as the same family after the fixpoint, repeated until stable) drops all
    three fork claims (41803, 41804, 42003) and the MODULE fill that hung
    off them (41773).
  - Its cost: 155 claims and 15.5 KB, with one sampled library unit (3 B,
    execa) lost. Declarations are exempt.
  - The evidence is thin: one fork, three units.
- **Content-free claims.**
  - Step B claimed `var mLl={}` (11 B), an app module's namespace, as
    `@smithy/node-http-handler`. Through the namespace-filling statement it
    defines, its closure is a whole app module: env-name constants, the
    hostname helper, terminal detection.
  - With a 30-byte floor on where closure may start, the library-class
    closure has 0 app units.
- **App code speaking a library's protocol** (transcript normalisers,
  `tool_use`), and app zod schemas: no claims on them from any rule here.
  - Schemas _depend on_ zod, so they are callers of zod (UP), never zod's
    private code (DOWN).
  - The graph sorts them correctly as "uses a library", and the index has
    to resist naming them (the literal lane's own failure).

## 7. Safety: the guards, in the order they run

Precision comes before recall. The failures this design must not repeat are
measured ones:

- the ink fork claimed as `ink`;
- app code speaking a library's protocol;
- the bundler runtime labelled as a library;
- unbounded neighbour fill (211 KB of app negatives).

The guards, in order:

1. **The runtime veto.** Runtime helpers and React Compiler sentinels are
   externals, never candidates.
2. **The content floor** on seeds and on closure starts: no content-free
   units (`var X={}`, a bare init call, a `var a,b;`), using the index's
   ≥ 3 content tokens for statement hits.
3. **Stop-lists and families** on every index claim.
4. **The isolation veto, repeated to a fixpoint.** An index claim must end
   connected to code of its own family. Declarations are exempt. The
   support is counted after the fixpoint's DOWN and GLUE rules, not
   MODULE's fills (core, 2026-10-11): a fill grown from the seed itself is
   not independent evidence (Claude Code 2.1.86's ink fork).
   `veto_support = "shown"` (exp144, default off) relaxes that for a
   family the bundle already shows without fills: an unambiguous seed of
   such a family may count its module's fill as support. A family whose
   every seed only its own fill supports (a lone copied file) and a seed
   another family's prints also accept (a fork, a copy) are vetoed as
   before; a seed the default veto keeps is never dropped.
5. **No upward growth without evidence.** A caller of a library is app code
   until its own evidence says otherwise. UP regions are only a queue.
6. **The closure guard on fills.** A fill never crosses a unit whose
   closure leaves the module, and never a unit that unidentified outside
   code calls.
7. **A region that defines or patches app state is not a library.**
   Init-time mutations link both ways (§4), so a patched library falls out
   of closure.
8. **Agreement before anything leaves `src/`.**
   - A structural AND a literal index claim on the same package, or a
     declaration.
   - Graph rules may _extend_ a claim inside an exact module boundary. They
     never decide a move alone.
9. **Report-only first** (§8), with every claim carrying its rule, round
   and evidence, so a wrong one can be traced to the rule that made it.

Product-vocabulary strings (`claude`, `tengu_`) are **not** a guard. The
first-party libraries (`@anthropic-ai/sdk`, sandbox-runtime, mcpb) contain
them, and picking negatives by product strings is exactly what hid the hard
cases before (negatives lane).

## 8. What changes in the output, and how the eval scores it

**Phases.** Each phase waits for the previous one's audit.

1. **Report-only.** `--diagnostics libraryRegions`: one row per claimed
   unit, carrying `{start, end, package, version?, rule, round, evidence}`,
   plus the UP queue. Nothing is named, moved or renamed. This is the same
   contract as `appDeclaredLibraries`.
2. **Naming hints.** The model's prompt for a function inside a claimed
   region says "this is <package>'s code" (the hint channel P17 already
   uses for vendor modules). It is cheap and reversible. Measured as an
   ordinary naming change: `noise` and `mints` may move, `novel` and
   `realLn` must not.
3. **Library freeze or library naming.** Claimed regions join
   `libdetect`'s library classification, which is the same owner as banner
   regions: they leave the app naming waves, and their names come from the
   library side.
   - Caution: the existing freeze keeps minified names, against the "no
     minified leftovers" goal. This phase needs a library-side namer
     first.
4. **Move to `vendor/`.** Only for regions with exact boundaries and agreed
   evidence (guard 8). The vendor record would carry the region's source
   statements and its relink, like a CommonJS factory's.

**Scoring.**

- `novel` and `realLn` have a band of zero and must not move in phases 1–3.
  Phase 1 cannot move anything. In phase 4, statement-content scoring
  across `src/` and `vendor/` together must keep them byte-equal, because
  moving a statement is not real change.
- **A new column, `reclassifiedBytes`**: `src/` bytes the run attributes to
  a library, split by rule (declared, index, GLUE, DOWN, MODULE,
  DOWN-CLASS). A second column, **`reclassifiedAppBytes`**, comes from a
  fixed hand-labelled audit set: this sample's 620 statements and 36 traps,
  extended per release.
- The fear is hiding app change. So every byte in `reclassifiedAppBytes` is
  reviewed, and a phase-4 merge needs it at 0 on the audit set.

## 9. What surprised us

- **Closure is the cheapest thing to compute and the weakest evidence.**
  "Self-contained and called one way" is true of every leaf of the app. The
  graph's value is in shaping regions and in vetoing claims, not in finding
  libraries.
- **Direction is never violated by real libraries.** No sampled library
  unit reaches the app's strongly-connected core, and closure from claimed
  library code reaches no sampled app unit, through any edge kind here,
  callbacks included. The only app code reached from claimed library code came
  through a wrong claim. So "the claim's closure reaches app code" is a
  usable _alarm_ on the claim.
- **Re-export glue is essential.** Without namespace-filling edges the graph
  is wrong in the dangerous direction: an app zod schema looks
  self-contained.
- **Forks reveal themselves by isolation.** The kept tables of the ink fork
  have no ink-claimed neighbour, because the fork rewrote their callers. One
  fork is all the evidence there is.
- **The giant strongly-connected app core is small**: 1.2 MB, 2,972 units.
  Most app code (10.4 MB) lies _below_ it, in a DAG. The hoped-for picture
  of "a big app knot with libraries hanging off it" does not hold for a
  lazily-loaded Bun app.

## 10. Next steps

1. **DONE (2026-10-11, branch `exp095-discovery-core`).** The graph builder in core (`graph::references`), with red/green tests
   on synthetic bundles for each edge kind in §4: namespace fill,
   definitional vs run-time mutation, writers as definers, wrapper → body.
   Then the report-only pass with its diagnostics block, on a toolchain
   piece for helper shapes.
2. **Feed the UP queue to the index with context.**
   - Measure whether a lower threshold for units adjacent to an identified
     package (Step B's "neighbour support", now along graph edges instead
     of byte positions) recovers the 7% residue without app claims.
   - That is the one place the fixpoint could grow upward.
3. **Widen the fork evidence.** (Partly done 2026-10-11: on 2.1.198 and
   2.1.86 the veto, with MODULE fills excluded from support, removes every
   ink-fork claim; the index made no claim on the help formatter or the
   yoga port in any of the three releases, so the veto was not tested
   there. Discovery README, "Results".) Hand-label CC's commander-style help, the
   yoga port and other forks in more releases. Check that isolation holds
   on 2.1.198 and 2.1.86 (one fork is one data point).
4. **Repeat on a second release, and on an esbuild bundle.** (Partly
   done 2026-10-11: 2.1.198 and 2.1.86 with Step B claims; wrangler 3.114.0,
   an unminified esbuild bundle scored exactly against its source map, with
   declared seeds only. Discovery README, "Results".) esbuild marks
   every lazy module, so MODULE should fill more there.
5. **The audit-set scorer columns** (`reclassifiedBytes`,
   `reclassifiedAppBytes`) in the 034 harness, before phase 2.
