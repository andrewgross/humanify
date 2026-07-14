# Next steps — 2026-07-14

Reference doc for the follow-ups parked after the runnable-split + diagnostics
work. Everything below is optional and independent; pick by priority/appetite.

## What just landed on `main`

Two feature branches merged (both full-check green; validated on the real
Claude Code 2.1.120 bundle):

- **`feat/bun-module-relink`** — `--split-runnable` now produces a genuinely
  runnable tree:
  - `src/split/bun-relink.ts` re-binds the extracted Bun `__commonJS`
    factory modules into an executable graph (live `.f` bindings survive
    require cycles).
  - `src/split/runnable-scaffold.ts` emits `run.cjs` + `package.json` +
    `RUNNABLE.md` into the output, so `npm install && node run.cjs` boots the
    real CLI. Externals are auto-detected (excludes builtins + `scheme:`
    specifiers like `bun:jsc`).
  - `src/commands/unified.ts` `checkFlagInvariants` — unusable flag combos
    (`--split-runnable`/`--split-llm-names`/`--split-ledger` without `--split`;
    `--naming-floor-sweep` without `--naming-floor`; `--reconcile-prior-diff`
    without `--prior-version`) now crash upfront instead of being ignored.
  - fix: `src/split/cjs-emit.ts` no longer routes class-element `this` (field
    initializers / static blocks) through the bundle context — was a latent
    crash on the pre-branch `main`.
- **`feat/rename-diagnostics-trail`** — rename provenance + a replayable ledger:
  - `--diagnostics` entries gain `trail: [{round,proposed,result}]`,
    `structuralHash`, and `strategy` (`src/rename/processor.ts`,
    `src/rename/diagnostics.ts`, `src/analysis/types.ts`).
  - `--rename-ledger <dir>` emits `rename-ledger.json` (every rename keyed by
    byte offset) + `source.js` (beautified snapshot) + `apply.mjs` (standalone
    replay). `src/rename/rename-ledger.ts`. `applyRenameLedger(source,ledger)
=== generate(ast)` is self-verified on emit.

## Parked follow-ups (the actual next steps)

### 1. env-reads → a real tool

**What:** promote the scratchpad AST extractor (removed) into a committed
`humanify env-reads <path>` subcommand or a `scripts/` util, with tests, and
an optional HTML view.
**Why:** we used it to inventory 1,557 env reads / 590 vars in the CC output;
worth keeping as a first-class diagnostic (audit secrets, diff env usage across
versions, generate `.env.example`).
**Design (proven):** Babel visitor over `process.env`/`Bun.env`/
`import.meta.env` bases; per use inspect the parent — member (direct/computed
literal → name; computed dynamic → flag), `VariableDeclarator` ObjectPattern
(destructure → keys), Identifier init (alias → resolve `binding.referencePaths`),
else whole-env/enumerated. Report by var with `file:line`, plus dynamic +
enumerated sections. Report we generated: `../unpacked-claude-code-env-reads.md`.
**Effort/risk:** ~150 LOC, low risk. The 108 dynamic-key reads are inherently
unresolvable statically — surface with locations, don't pretend completeness.

### 2. Flag _value_ validation (companion to the precondition work)

**What:** `--bundler`/`--minifier` still accept unknown type strings silently.
Reject unknown values upfront, matching the "no silent no-op" principle behind
`checkFlagInvariants`.
**Why:** same invariant class the user called out — a flag that can't take
effect should crash, not be ignored.
**Where:** `src/commands/unified.ts` action handler; validate against the known
`BundlerType`/`MinifierType` sets (mirror `parseReasoningEffort`'s
`console.error` + `process.exit(1)`). Consider folding into a single upfront
`validate…` step alongside `enforceFlagInvariants`.
**Effort/risk:** small, low risk. Confirm the accepted value sets and that an
invalid value currently no-ops (vs. being handled downstream).

### 3. Rename ledger — reconcile coverage

**What:** `--rename-ledger` reproduces the LLM-rename output but NOT
`--reconcile-prior-diff` renames (they run post-generate, in the output
coordinate space, not the beautified-input space the ledger indexes).
**Options:** (a) emit a second, output-space ledger section for the reconcile
pass; or (b) leave as-is and keep the documented caveat (reconcile is off by
default). Low urgency.
**Where:** `src/rename/plugin.ts` (`buildRenameLedgerBundle` is built pre-
reconcile); `src/rename/diff-reconcile.ts` (the post-generate renames).

### 4. `using` desugaring (vs. the run.cjs shim)

**What:** the runnable scaffold's `run.cjs` STRIPS `using`/`await using`
(regex → `const`) on Node versions that can't parse it — which drops the
`Symbol.dispose`/`Symbol.asyncDispose` cleanup semantics. It's a compat shim,
not a faithful transform.
**Clean fix:** a real desugar at emit time (try/finally + dispose calls), or
gate on a Node version that supports `using` natively (feature-detect already
present in the runner). Until then the caveat stands: resources acquired via
`using` aren't disposed under the shim.
**Where:** `src/split/runnable-scaffold.ts` (`RUNNER_SCAFFOLD`).

### 5. Scaffold dependency version pinning

**What:** the auto-generated `package.json` pins externals at `"*"`. Fine for
most, but a package whose internal layout must match (e.g. `ajv`'s
`dist/runtime/*` is v8-specific) can resolve to an incompatible major.
**Idea:** detect a version hint where possible (bundler banner / lockfile if
present) or document the pin-if-needed note (already in RUNNABLE.md). Minor.
**Where:** `src/split/runnable-scaffold.ts` (`packageJsonSource`).

### 6. Runnable-emit double parse (perf)

**What:** `emitRunnableCjs` re-parses the whole ~9 MB bundle back-to-back with
`stableSplitFromCode`'s parse of the same string.
**Idea:** thread the parsed AST + wrapper result out of `StableSplitResult` and
into `emitRunnableCjs` (byte offsets align — same string), deleting the second
full parse + scope crawl.
**Where:** `src/split/stable-split.ts` (return the AST), `src/split/cjs-emit.ts`
(accept it), `src/commands/unified.ts` (thread through). Bench-level; opt-in
path only (`--split-runnable`).

## Separate: pre-existing project backlog

Not from this work stream, but on the broader list (see auto-memory
`project_future_work.md`, `project_plan_2026_07_06_executed.md`): rename-invariant
shingles (~196 residual), the ~158 hash-absent (0.4% hash instability), and
operator normalization for cross-version naming consistency.
