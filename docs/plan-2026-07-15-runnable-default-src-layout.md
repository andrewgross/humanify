# Plan — runnable-by-default `--split` + `src/`/`vendor/`/`.humanify/` layout

Status: ready to implement (fresh agent). Branch: `experiment/graph-clustering-split`
(continue on it, or branch from it). Repo: `/Users/andrewgross/Development/humanify`.

## Background — what already landed (read before starting)

exp029 replaced the split's fresh-grouping with reference-graph **clustering**,
now the SOLE `--split` approach (the old budget-grid path is deleted). Current
behavior on a Bun wrapper bundle:

- `src/split/cluster-assign.ts` — `assignClustered(body, {namer?, config?})`
  returns a per-statement file-path assignment. It: sets whole vendored Bun CJS
  factories (`var X = d((exports,module)=>…)`) aside as `libraries/<name>.js`
  (untouched), seam-cuts the app statements into a size-balanced **2-level**
  nested tree (`<folder>/<subfolder>/<file>.js`), names every level from its
  dominant binding, LLM-polished via the namer. Paths are unique
  **case-insensitively** (macOS/Windows safe).
- `src/split/stable-split.ts` — `stableSplitFromCode(code, {namer?, prior?,
clusterConfig?})`. Fresh → `assignClustered`; prior-carried → `assignWithPrior`
  (unchanged name-vote inheritance = cross-version stability). Emits byte-exact
  statement slices; `assertConcatEquivalence` guarantees the tree reconstructs
  the source byte-identically. Writes `_split-ledger.json`.
- `src/split/cjs-emit.ts` — `emitRunnableCjs(code, ledger, wrapper?)` transforms
  the byte-exact tree into a runnable CommonJS graph: per-file `require(...)`
  headers, `foo`→`__ns.foo` reference rewrites, `Object.defineProperty(module.
exports,…)` getter/setter footers, plus a root `index.js` entry and (when the
  wrapper used exports/require/module/**filename/**dirname/this) a `_bundle.js`
  runtime. `assertLoadTimeAcyclic` THROWS on a load-time reference cycle.
- `src/commands/unified.ts` — `tryStableSplit()` orchestrates: split → optional
  runnable emit (`--split-runnable`, `tryEmitRunnableCjs` falls back to the
  review tree on any failure) → `writeSplitTree` → `relinkBunFactoriesIfPresent`
  → `emitRunnableScaffold` (writes `run.cjs` + `package.json`). LLM naming is
  always-on for fresh releases (no flag).

**Measured on real 2.1.89** (see `experiments/029-graph-clustering-split/`):
1,893 app files / 297 folders / depth 2 / median 82 lines; 1,523 libraries
aside; concat-equivalence holds; **runnable emit is load-time ACYCLIC (0
merges needed)**; 0 case collisions.

## Goal

1. Make **`--split` produce the RUNNABLE tree by default**; add `--split-pure`
   for the byte-exact (non-runnable) tree; remove `--split-runnable`.
2. Reorganize output so **generated artifacts are obviously separate from the
   humanified code**: harness + metadata at the root / in `.humanify/`, the code
   under `src/`, vendored libraries under `vendor/`.

## Confirmed decisions

- Code folder: **`src/`**. Libraries folder: **`vendor/`**. Metadata folder:
  **`.humanify/`**.
- **`_bundle.js` goes in `.humanify/`** (pure runtime you never read); the root
  stays the "front door": `index.js`, `package.json`, `run.cjs`.
- `--split` = runnable by default, falling back to the pure tree on the rare
  load-time cycle (graceful — no hard fail). `--split-pure` forces the pure tree.

## Target layout

**`--split` (runnable, default):**

```
<outputDir>/
├── index.js            entry — requires src/ + vendor/ in load order   GENERATED
├── package.json        external deps to `npm install`                  GENERATED
├── run.cjs             `node run.cjs` runner                           GENERATED
├── .humanify/                                                          GENERATED
│   ├── _bundle.js          shared wrapper-context runtime
│   ├── split-ledger.json   layout memory (next version inherits it)
│   └── …                   (rename ledger / profiling / debug — see step 6)
├── src/                the humanified app code (2-level nested tree)
│   └── <folder>/<sub>/<file>.js …
└── vendor/             vendored libraries, untouched
    └── <name>.js …
```

**`--split-pure` (byte-exact):** identical minus the runnable harness
(`index.js`, `package.json`, `run.cjs`, `.humanify/_bundle.js`) — just `src/` +
`vendor/` + `.humanify/split-ledger.json`.

## Implementation steps

### 1. `assignClustered` emits `src/` + `vendor/` paths (`src/split/cluster-assign.ts`)

- App file paths: prefix with `src/` → `src/<folder>/<sub>/<file>.js`.
- Library paths: `libraries/<name>.js` → `vendor/<name>.js`.
- Keep it simple: add `codeDir` (default `"src"`) and `vendorDir` (default
  `"vendor"`) to `ClusterConfig`/`DEFAULT_CLUSTER_CONFIG`, or hardcode — your
  call, but the names must be the ONLY place they're defined.
- This is a one-time re-baseline: the ledger now stores prefixed paths;
  `assignWithPrior` inherits them unchanged going forward. (An existing prior
  ledger with old flat paths keeps old paths until a fresh re-baseline — fine,
  no backwards compat needed.)
- Update `src/split/cluster-assign.test.ts` expectations (paths now start
  `src/…` / `vendor/…`).

### 2. Verify the runnable emit follows the code into `src/` (`src/split/cjs-emit.ts`)

- `assembleTree` already computes RELATIVE requires via
  `computeRelativeImportPath(entryName, file)`. With ledger files under `src/`,
  the entry becomes `require("./src/…")` and cross-file requires stay relative
  within `src/` — likely **no change needed**, but confirm by running
  `verify-runnable.ts` (step 7) and eyeballing a few requires.
- **Move `_bundle.js` into `.humanify/`**: in `assembleTree`, change
  `pickFreeFile("_bundle.js", taken)` to produce `.humanify/_bundle.js` (or the
  equivalent). Every file that requires the bundle context resolves it via
  `computeRelativeImportPath`, so deeper relative paths (`../../.humanify/
_bundle.js`) are handled automatically — just verify.
- **Keep `index.js` at the root** (the entry; `runnableEntryFile` matches
  `/^_*index\.js$/`).

### 3. Flip the flags (`src/commands/unified.ts`)

- `CommandOptions`: remove `splitRunnable`, add `splitPure?: boolean`.
- `checkFlagInvariants`: replace the `--split-runnable` rule with a
  `--split-pure` requires `--split` rule.
- `tryStableSplit`: attempt the runnable emit UNLESS `opts.splitPure`
  (i.e. `const wantRunnable = !opts.splitPure`). Keep the graceful fallback to
  the pure tree when `tryEmitRunnableCjs` returns null.
- CLI `.option(...)`: remove `--split-runnable`; add
  `--split-pure` ("Emit the byte-exact clean review tree instead of the runnable
  CommonJS graph. Requires --split").
- Update `src/commands/unified.test.ts` (`splitDependents` + the "every
  violation" test) to swap `splitRunnable` → `splitPure`.

### 4. Metadata into `.humanify/` (`src/commands/unified.ts`)

- `_split-ledger.json` is currently written to `path.join(outputDir,
SPLIT_LEDGER_FILENAME)`. Write it to `path.join(outputDir, ".humanify",
"split-ledger.json")` instead (mkdir -p the folder).
- Update `loadPriorSplitLedger` discovery to look in `.humanify/split-ledger.json`
  next to `--prior-version` (keep a fallback to the old path if trivial).
- Scaffold (`run.cjs`, `package.json`) stays at the root — confirm
  `emitRunnableScaffold` still points `run.cjs` at `runnableEntryFile(...)`.

### 5. `vendor/` + bun-relink (`src/split/bun-relink.ts`, `unified.ts`)

- `relinkBunFactoriesIfPresent(outputDir, files, renderer)` re-links Bun
  factory references. With libraries now under `vendor/`, verify it still finds
  / rewrites them (it may be a no-op for single-file `index.js` input — READ
  `bun-relink.ts` to confirm what it operates on before changing anything).

### 6. (Secondary — do only if quick) route rename-ledger / profiling into `.humanify/`

- `--rename-ledger <dir>` and any profiling/debug output: default them under
  `.humanify/` when `--split`. If it's more than a couple lines, note it as a
  follow-up and skip — the core value is `src/` vs harness separation.

## Invariants to preserve (do NOT break these)

- **Concat-equivalence**: `assertConcatEquivalence` must still pass — the tree
  reconstructs the source byte-identically. Prefixing paths doesn't touch byte
  slices, so this should hold; verify anyway.
- **Cross-version stability**: `assignWithPrior` is path-agnostic and stays
  untouched. The prefix change is a one-time re-baseline.
- **Load-time acyclicity**: `assertLoadTimeAcyclic` still guards the runnable
  emit. (2.1.89 is acyclic; a future cyclic input falls back to the pure tree.)
- **Determinism** and **case-insensitive uniqueness** of paths.
- **Pre-commit biome is STRICTER than `npm run check`** on cognitive complexity
  (≤15). Run `npx biome check <changed files>` before committing; extract
  helpers if a function exceeds 15.

## Verification checklist

Fast iteration needs NO LLM — the experiment harnesses run on a cached,
beautified 2.1.89 and use mechanical names. Big heap required:
`NODE_OPTIONS=--max-old-space-size=8192`.

1. `npm run typecheck` and `npx biome check src/split/*.ts src/commands/unified.ts`.
2. `npm run test:unit` (rewrite cluster-assign/stable-split/unified tests for
   the new paths + flags first).
3. `tsx experiments/029-graph-clustering-split/verify-prod.ts 2.1.89` — expect
   concat-equivalence PASSED, 0 case collisions, app files under `src/`,
   libraries under `vendor/`.
4. `tsx experiments/029-graph-clustering-split/verify-runnable.ts 2.1.89` —
   expect "load-time ACYCLIC", entry requires `./src/…`, `_bundle.js` under
   `.humanify/`.
5. `tsx experiments/029-graph-clustering-split/layout.ts 2.1.89 <outDir>` —
   eyeball the tree: root = harness + `.humanify/`, code under `src/`,
   libraries under `vendor/`.
6. `npm run check` (full: typecheck + lint + unit + e2e fingerprint).
7. Commit as a re-baseline (all files move once — expected). Message should note
   the layout change + flag flip.

## Deferred (not this task)

- **Load-time cycle merge gate**: only needed if a future input has a load-time
  cycle AND we want runnable to succeed rather than fall back to pure. 2.1.89 is
  acyclic, and the fallback is graceful, so this is a robustness follow-up:
  extend the Tarjan pass to enumerate load-time SCCs (over `cjs-emit`'s
  `loadTimeEdges`) and merge each >1 SCC's files into one before emit.
- Naming quality review (doubled `folder/folder/` names when top & sub share a
  dominant binding; proper library naming). Tracked separately.

## Key references

- Split: `src/split/{cluster-assign,stable-split,cjs-emit,runnable-scaffold,
bun-relink}.ts`. Orchestration: `src/commands/unified.ts` (`tryStableSplit`
  ~L334, `runSplit`, `checkFlagInvariants` ~L89, CLI options ~L840).
- Experiment harnesses + cached fixture loader:
  `experiments/029-graph-clustering-split/{verify-prod,verify-runnable,layout,
libaware,measure}.ts` and `lib/io.ts` (`loadBeautified`). Results/context:
  `experiments/029-graph-clustering-split/RESULTS.md`.
- Fixtures: input = `~/Development/claude-code-versions/inputs/claude-code-2.1.89/
binary-decompiled/src/entrypoints/index.js`; real-src target =
  `~/Development/claude-code-src-2.1.88/`. Consecutive versions for a stability
  check: 2.1.87 & 2.1.89.
