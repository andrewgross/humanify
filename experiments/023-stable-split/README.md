# Experiment 023 — stable split: logical files/folders that don't churn between versions

**Goal of this stage (user directive 2026-07-10):** take the humanified
output and split it into logical, stable folders and files, the way a
human would structure a repo — so changes are easier to follow and
scope. Four hard requirements:

1. **Cross-version stability** — consistent file/folder assignment
   between releases, so the version-to-version `git diff` shows no
   artificial churn from code moving between files.
2. **No dumping grounds** — no oversized single files of unrelated
   concepts (today: `runtime.js` is 524,604 lines / 21 MB — 72% of the
   output — in ONE file).
3. **Runnable post-split** — proper file naming and generated
   imports/exports so the split tree parses and can execute.
4. **Good grouping** — code grouped by subsystem/concern, human-like.

This is the same campaign as exp013–022 (a clean reviewable `git diff`
between adjacent releases), moving from the NAME axis to the FILE axis.
This doc is self-contained: campaign context, prior art and why it
stalled, what changed, the design, metrics, and runbook. Code anchors
verified 2026-07-10 on `main` (bf5e94f).

---

## Where the output stands today (measured 2026-07-10)

A pipeline run on the Claude Code fixture emits **1,495 files**:

- **Third-party layer (~8 MB, stable-ish):** the Bun UNPACK stage
  (`src/unpack/adapters/bun.ts`, manifest `_bun-modules.json`) extracts
  CJS factory modules into per-library files before renaming. Cross
  version this layer is already nearly list-stable: 1,494 of 1,495
  names shared between v119/v120 legs. Two defects worth fixing in
  passing: (a) unclassified libs are named `lib_<contenthash>.js`, so a
  lib that CHANGED between versions gets a NEW filename — churn by
  construction (observed: `lib_3b8af129.js` ↔ `lib_59f3ff19.js`, the
  same library); (b) collision/minted names leak into filenames
  (`H.js`, `H-2.js`, `DepType-2.js`).
- **App layer (21 MB, the problem):** everything else lands in
  `runtime.js` — the dumping ground. All exp013–022 diff metrics were
  measured on this file. Splitting it is the core of this stage.

The app-code splitter EXISTS (`--split`, `src/split/`, ~5,900 lines,
built in experiments 002–010) but has never run in the production
chain and is **not prior-version-aware at all**.

## Prior art — what was built, and why it was parked

(Full survey: experiments/EXPERIMENT-{PLAN,RESULTS}.md and 002–010.)

- Pipeline: `buildSplitPlan` (`src/split/index.ts:315`) — function graph
  - completeness ledger → `detectModules` → adapter →
    `groupFunctions` → per-statement assignment cascade → import-cycle
    resolution → emit. Adapters (`src/split/adapters/index.ts:14`):
    esbuild-ESM path comments (ARI 1.000 ceiling) → esbuild-CJS wrappers
    → Bun-CJS wrappers → CallGraph fallback for hoisted code.
- The CallGraph fallback clusters by BFS reachability, and for sparse
  graphs (>70%) by **co-reference clustering**
  (`src/split/reference-cluster.ts`): IDF-weighted Jaccard over
  positional reference sets, with **gap-based** splitting (byte-offset
  gaps between functions — bundlers emit files sequentially) when the
  similarity graph is too dense.
- Emission (`src/split/emitter.ts`): source-range slicing, real
  `import`/`export` statements with relative paths, barrel `index.js`,
  `shared.js` kept a leaf, 2-file cycles broken. `validate-split.ts`
  parses every emitted file and can `import()` the barrel — the
  runnability machinery exists and passed on Preact.
- File naming (`src/split/naming.ts`) is mechanical: root-function
  name, common prefix, or `mod_<fingerprint>.js`; exp004 flagged names
  like `hydrate_cloneElement_createContext.js` as inadequate.
- **Why parked:** the campaign target was ARI ≥ 0.40 vs source-map
  ground truth. Minified fixtures broke the co-reference signal —
  terser leaves same-file/cross-file Jaccard separation at 1.15×
  (near-random), so hono-minified collapsed to ONE cluster, ARI 0.000.
  Ten vocabulary/graph ideas (A–J) failed; gap-first clustering
  rescued it to ~0.5 but zod-minified (all code on 2 lines) regressed,
  and no universal cluster-count heuristic existed. Max achievable
  (~0.5) sat far below the comment-marker ceiling (1.0), and Bun
  binaries have no path comments — so effort moved to renaming
  (exp013+).
- The split-era docs contain **zero cross-version-stability design**;
  determinism-within-a-run only. The ledger
  (`src/split/ledger.ts`) guarantees completeness (no statement
  dropped), not stability.

## What changed — why this is worth reopening now

1. **We split HUMANIFIED code now.** The ARI-0.000 diagnosis was about
   minified identifiers destroying the co-reference vocabulary. After
   exp014–022, the input has descriptive names that are **stable
   across releases** (noise 22,998 → 498; the naming floor drives
   minted tokens toward zero). Name-based grouping signals are back at
   full strength — and they agree across versions.
2. **The lineage architecture exists.** exp022 validated the template:
   _prior wins; fresh decisions only for the residue._ File assignment
   is just another attribute to carry through the lineage, like a
   name: transfer the prior version's statement→file assignment, and
   run fresh placement only for genuinely-new code.
3. **Sequential emission order survives.** Bun emits modules
   sequentially and the rename pipeline is pure-rename, so top-level
   statement ORDER is preserved into the humanified output — adjacency
   and gap signals remain usable, now WITH names.

## The design (directions, not a transcript)

### Step 0 — measured (2026-07-10, on the exp022 steady pair)

**The current splitter is a structural no-op on the real bundle.**
`probe-split.ts` on `chainA-119.js` ran in 26.6s and emitted ONE file:
`orphans.js`, 524,606 lines, 100% of the code. Diagnosis: the entire
bundle is a single top-level statement — the Bun CJS wrapper IIFE
`(function (exports, require, module, __filename, __dirname) {…})` —
and both the ledger (`ast.program.body`) and the CallGraph adapter
(top-level functions only) never look inside it. First work item:
wrapper-aware statement extraction (the rename graph already locates
the wrapper — `graph.wrapperPath`).

**The real splitting population** (`census-wrapper.ts`, inside the
wrapper): **23,442 statements** — 11,938 function declarations, 10,860
variable declarations (17,254 declarators), 268 classes, 375
expression statements (side effects whose execution ORDER must
survive splitting), 1 if. **98.4% of statements declare ≥1 named
binding.**

**The transfer ceiling** (`name-overlap.ts`, both legs): **99.4% of
the 119 leg's 35,874 declared names exist in the 120 leg** (35,660
shared; 214 A-only / 441 B-only — genuinely added/removed code). So
name-carried assignment transfer covers ~98% of ALL statements, and
the residue needing fresh placement is a few hundred per release. The
rename campaign (exp014–022) built exactly the stable substrate this
mechanism keys on.

Runnability findings for the emitter design: the wrapper params are
CJS module scope — split files need their own module semantics;
cross-file WRITES to module-scope bindings are illegal as ESM imports
(the emitter must keep writers with their binding or emit accessors —
`constantViolations` marks them); the 375 side-effect statements pin a
total execution order the entry file must reproduce.

### The mechanism — a split ledger carried through the lineage

Mirror the rename lineage at the file axis:

1. **Prior-assignment transfer.** For each top-level statement in the
   new leg, find its prior counterpart and inherit that counterpart's
   file (and folder). Matching resolves in order: same declared
   binding name (names are stable now — the cheap, high-precision
   path) → structural hash (`structural-hash.ts`, rename-invariant) →
   positional/diff alignment (the exp020 `computeNormalDiff` machinery
   on the pre-split text). Unanimity per file: a statement whose
   signals disagree goes to fresh placement, never a guess.
2. **Fresh placement for the residue only.** Genuinely-new statements
   join the file where their reference-neighbors already live
   (majority vote over resolved references — `tryAssignEntry`'s logic,
   made lineage-aware), else seed new files via the existing
   clustering (gap + co-reference, which now sees descriptive names).
3. **File/folder naming.** New files are named from their dominant
   root binding (descriptive post-rename); existing files KEEP their
   prior name even if members were renamed — the file name is part of
   the carried state. Folders group files by import-affinity /
   subsystem and are carried the same way. The split emits a
   `_split-ledger.json` manifest (statement → file, file → folder,
   name provenance) that the NEXT release consumes as its prior —
   exactly like `--prior-version` for names.
4. **Emission.** Reuse the existing emitter (imports/exports/barrel/
   cycle-breaking/ledger-completeness) — do not fork it. Extend where
   the app bundle needs it (e.g. `var` hoisting semantics, TDZ-safe
   ordering within files).

### Split-churn gates (precision over recall — project law, file axis)

Moving code between files across versions is this stage's false
positive. Gates default to KEEP the prior assignment:

- A matched statement NEVER moves files, even if fresh clustering
  would place it "better" — stability beats optimality. (Escape
  hatch for later: an explicit opt-in re-layout flag, never default.)
- New code placement must clear an affinity threshold to join a file;
  ambiguous residue goes to a deterministic overflow location, not a
  guess.
- File renames only when a file is NEW this release. A prior file that
  lost all members disappears (that's genuine); it never merges-and-
  renames in the same hop.
- The ledger completeness check (every statement exactly once) is this
  stage's structural invariant; emitted-tree parse + import resolution
  are the output validation.

## Metrics + success criteria

Measured on the exp022 steady pair, both legs split:

1. **File churn** (requirement 1): `git diff -M --name-status` between
   the two split trees — count added/deleted/renamed/modified files;
   plus binding-level assignment agreement: % of top-level bindings
   present in both legs (by name) whose file path matches. Target:
   agreement ≥99%; file renames/moves ≈ 0 spurious.
2. **Line-diff conservation**: total diff hunks across the split tree
   ≈ the single-file diff's (≈2,400 hunks, 498 noise). Splitting must
   not manufacture new diff lines (import blocks are the one allowed
   source — measure them separately).
3. **Dumping grounds** (requirement 2): p90/max file lines; share of
   app code in the largest file. Target: max file ≤ ~5k lines, no
   file > ~5% of app code, `shared.js`/overflow bounded.
4. **Runnability** (requirement 3): 100% emitted files parse; import
   graph resolves and is acyclic (barrel excepted);
   `node --check` clean per file. Stretch: smoke-execute.
5. **Grouping quality** (requirement 4): no ground truth exists for
   the Claude Code bundle — use proxies (cross-file import fan-out,
   intra-file reference cohesion) + human spot-checks of named
   files/folders. LLM may JUDGE groupings or NAME files (naming-only
   law); it never moves code.

Tripwires: assignment agreement dropping when names churn (the split
must not amplify rename noise); diff-hunk total rising vs the
single-file baseline; a "misc" file accreting >5% of statements.

## Guardrails (project law)

- **LLM for naming only, never code rewriting.** All statement
  movement is deterministic AST surgery through the existing
  emitter; the LLM may only propose file/folder NAMES.
- **Reuse, don't fork**: the ledger, emitter, adapters, structural
  hash, and diff-alignment machinery all exist — extend them.
- Red/green TDD; `npm run check` green before every commit; biome
  complexity ≤ 15; colocated `*.test.ts`; branch
  `exp023-stable-split` off `main`; do NOT merge — Andrew reviews.
- The LLM box (`http://192.168.1.234:8000/v1`, `openai/gpt-oss-20b`,
  key `local`) is owned hardware — wall-clock is the only budget.

## Runbook

```bash
# Step 0 probe (current splitter, one leg)
npx tsx experiments/023-stable-split/probe-split.ts /tmp/e022/chainA-119.js /tmp/e023/119-split

# churn measurement between two split trees (to build: measure-churn.ts)
#   git diff -M --name-status + binding-level assignment agreement

# artifacts: the exp022 steady pair
#   /tmp/e022/chainA-119.js   (prior-aware 119 leg)
#   /tmp/e022/120F.js         (floored+swept 120 leg)
# regenerate via experiments/022-prior-aware-sweep/run-sim.sh if gone
```

## Code anchors (verified 2026-07-10 on main bf5e94f)

- Split pipeline: `src/split/index.ts` — `buildSplitPlan` :315,
  `splitDryRun` :394, `splitAndEmit` :407, `splitFromAst` :689,
  `tryAssignEntry` :217 (the quadratic assignment pass),
  `resolveImportCycles` :671, `generateManifest` :703.
- Ledger (completeness, NOT stability): `src/split/ledger.ts`.
- Clustering: `src/split/cluster.ts`, `src/split/reference-cluster.ts`
  (positional co-reference + gap-first), adapters in
  `src/split/adapters/` (`selectSplitAdapter` order: esbuild-esm →
  esbuild-cjs → bun-cjs → call-graph).
- Emitter: `src/split/emitter.ts` (`buildFileContents`,
  `extractDeclaredNames`, `collectReferencedNames`); naming:
  `src/split/naming.ts`; quality: `src/split/quality.ts`.
- Unpack/lib layer: `src/unpack/adapters/bun.ts`
  (`BUN_MODULES_MANIFEST`), lib naming defects noted above.
- CLI wiring: `src/commands/unified.ts` — `runSplit` :79, `--split`
  :494 (split runs on `renameResult.ast` post-rename).
- Lineage machinery to mirror: `src/rename/prior-transfer.ts`,
  `src/rename/diff-reconcile.ts` (`computeNormalDiff` :160),
  `src/analysis/structural-hash.ts`.
- Validation: `experiments/validate-split.ts`, `src/split/ledger.ts`
  `verifyComplete`.

## Adjacent, explicitly OUT of scope

- Improving clustering ARI on MINIFIED library fixtures (dead end,
  fully documented in EXPERIMENT-RESULTS.md ideas A–J; we split
  humanified code now).
- The rename campaign's remaining noise buckets (swap cycles, LLM
  declines) — separate track.
- Executing the full Claude Code binary post-split (beyond smoke
  validation) — runnability here means parse + resolvable imports +
  node --check.
