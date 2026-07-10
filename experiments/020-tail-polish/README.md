# Experiment 020 — text-diff reconciliation: collapse the residual rename tail

**Goal of this experiment:** a deterministic, post-generation pass that
reads the _rendered-text diff_ between the new humanified output and the
prior version's humanified output, and — for the "obvious" rename-noise
hunks — snaps the new leg's binding names back to the prior names by
resolving each diff position to its Babel binding and renaming through
the existing validated-rename path.

This doc is self-contained. Pick it up cold and run. Read the campaign
context below, then the design, then the runbook.

---

## The end goal (the whole campaign)

Humanify two adjacent versions of a minified bundle (fixture: Claude
Code v119/v120, Bun single-file binary decompiled to
`.../binary-decompiled/src/entrypoints/index.js`) and make the `diff`
between the two humanified outputs contain **only genuine source
changes** — so a human can review "what changed between releases" like a
real git diff. The user's exact bar: "reasonable for a human to review
the diff between the versions like a real git diff."

We measure on the **shared-lineage diff** (see Runbook): each release is
humanified incrementally on the previous one, so both diffed legs
inherit one naming lineage — that is the production scenario, and it is
the metric that matches the goal. (The older fresh-vs-incremental A/B
overstates noise because its fresh leg re-invents names every run;
one-time collision retries and synonym picks show up there as recurring
noise. Prefer the lineage metric.)

### Campaign trajectory (lineage-diff noise hunks)

Phase-6 baseline 22,998 → exp014 6,206 → exp015 5,788 → exp016 2,960
(lineage) → exp017 2,516 → exp018 1,504 → **exp019 1,288** (current
`main`). Genuine-change hunks have held in the **1,868–1,975** band the
entire time — that is the SIGNAL and must never rise. Noise share is now
**40.8%**: noise is already below genuine. This experiment attacks the
next slice.

Each round's story is in `experiments/01{4,5,6,7,8,9}-*/RESULTS.md`.
Read exp016 and exp019 at least: exp016 introduced the lineage protocol
and the class-visibility change; exp017/018/019 are the ambiguous-bucket
/ clone / class-fingerprint matcher work whose _residue_ this experiment
mops up.

---

## Why this change now — what's left, and why text-diff is the right tool

After exp019 the residual (2,474 rename occurrences, ~1,093 bindings,
avg ~2.3 occ each — a flat tail, top family only 24 occ) is dominated by
bindings the AST matcher **cannot** pair:

- **Ambiguous-bucket residue** — e.g. 88 identical `require("fs")`
  bindings, all hashing the same; the structural hash and even the
  exp017/018 context stages can't tell member #47 from #48. Both legs
  fresh-LLM them into different synonyms (`fsModuleVal` vs
  `fileSystem33`).
- **Asymmetric residue** — one leg named a binding, the other left it
  minified (`completionState` vs `Tj_`), because the matcher missed it.

The insight: the **rendered-text diff already finds these** — a line
that differs _only_ in identifier tokens, framed by identical unchanged
context lines, is almost certainly the same binding. That is
**positional / textual locality**, a signal the AST hash throws away by
construction (it hashes each function in isolation, blind to what sits
above and below it in the file). A text-diff pass is complementary to
the matcher, not a replacement, and it is aimed squarely at the artifact
the user actually reviews.

Two facts make this viable and were verified this session:

1. **No prettier runs on the output.** Formatting is
   `generate(ast, { compact: false })` (babel-generator) only — prettier
   was removed (it OOMs on 14MB files). babel-generator formats
   _structurally_, not by prettier's greedy 80-col fill, so a longer
   identifier name does **not** reflow the line. That is exactly why the
   noisy hunks are clean single-line swaps and why 1:1 line alignment
   holds. (Do not assume it holds universally — verify each hunk is
   structurally-identical-after-blanking; that check is the entry gate
   anyway.)
2. **Renaming a binding rewrites all its references atomically.**
   `fastRenameBinding` (validated-rename.ts:123) walks
   `binding.referencePaths` + `constantViolations`, so five scattered
   `return Tj_;` hunks + the declaration + the assignment are **one**
   rename, not seven.

### What this does NOT do (be honest in the writeup)

It reconciles _two specific versions_ — it shrinks the v119↔v120 diff
but does not improve the naming lineage going forward. It is
cosmetic-on-the-diff. That is on-target for the goal ("make this diff
reviewable") but say so plainly in RESULTS.

---

## The design (worked out this session — implement this)

### Pipeline placement

Runs inside `createRenamePlugin` (src/rename/plugin.ts) when
`priorVersionCode` is present, as a **flag-gated** step so the default
path and its invariant story stay clean. Sequence:

1. `generate(ast, genOpts)` → new output text (exists, plugin.ts:569).
2. **Re-parse** the new output → `newAst`. Its `loc`s are in _output_
   coordinates, matching the diff's line:col. (The pipeline already
   re-parses the output for validation — reuse or mirror that.)
3. Diff new output vs `priorVersionCode`. Shell out to system `diff`
   (already used in the harness) and parse `LcL` change hunks — an
   in-process Myers diff on 370k lines will choke; the two files are
   ~95% identical so `diff` is fast.
4. Build the candidate renames (below).
5. Apply survivors via `attemptValidatedRename` on `newAst`'s scopes.
6. Re-generate from `newAst`; re-run `checkStructuralInvariant` against
   the SAME `semanticBaseline` (plugin.ts:562) — a text-diff-driven
   rename that corrupted structure must hard-fail.

### Detecting "obvious" hunks

Port `attribute-noise.py`'s test: `norm(line) = replace every identifier
with "#"`. A change hunk is **rename-noise** iff it has equal line counts
on both sides AND every `<` line's `norm` equals its paired `>` line's
`norm`. Structural identity guarantees the identifier token streams
align 1:1. Zip them; each position where prior=`A`, new=`B`, `A≠B` is a
candidate `B → A` in `newAst`.

### The core algorithm: resolve to bindings, GROUP BY BINDING

This is the load-bearing safety design. Do **not** dedupe by hunk —
dedupe by Binding:

1. Collect every `(newAst position → priorName)` candidate across ALL
   rename-noise hunks.
2. Resolve each position to its **Babel Binding** via scope. (This is
   why scoping is a non-issue: `Tj_` in function A and `Tj_` in function
   B are different Binding objects and can never be conflated — the
   pure-textual "same string, different function" mess is dissolved.)
3. Group candidates by Binding. Each binding now carries the set of
   prior names its occurrences voted for.
4. **Require unanimous agreement.** All occurrences of a binding must map
   to the SAME prior name. Agreement across N scattered references is
   strong corroboration; disagreement is a red flag (alignment slip or
   changed region) → skip that binding.
5. Apply `binding → priorName` once (rewrites all references).

### Safety ladder — stage the tiers

- **Tier 1 (ship first, read-only first): single-line binding
  declarations.** Hunk 1↔1 line, byte-identical after blanking, the
  differing identifier is a _declaration_ (`var/let/const X =`,
  `function X(`, `class X`). The `fsModuleVal`/`fileSystem33` and
  `Tj_`/`completionState`-declaration cases. Safest: the rename site IS
  the binding.
- **Tier 2: small contiguous blocks (N≤~10) of Tier-1 declarations.**
  The "block of 5 `require` assignments" case. Same safety, batched.
- **Tier 3 (separate, more scrutiny): function signatures even when the
  body changed.** `function stringify(inputObject,…)` vs
  `(dataObject,…)` — the body drifted (native `using` rewrite) so the
  hunk is NOT pure rename-noise, but the _signature line_ is
  structurally identical if only param names differ. Reconcile params
  only when the function is matched/close-matched, param counts are
  equal, and the signature line differs only in binding-identifier
  positions. Params are positional (slot 0 = "the thing being
  stringified" regardless of body drift) → low risk. Runs on the
  signature line in isolation, explicitly ignoring the changed body.

Reference-position renames (not declarations, e.g.
`() => retrieveSessionResult` → `() => sendRequestToSession`) are
Tier-1-eligible when the line is a byte-identical-after-blanking single
line; they lean on the exact-match + binding resolution rather than
declaration-site safety.

### Gates (precision over recall — a wrong name is worse than a leftover)

- Act only when the after-blanking match is EXACT (true 1:1 alignment).
- Require unanimous per-binding agreement (step 4).
- Require the prior name to be **currently absent from the new binding's
  scope** — fills a gap, never overwrites a distinct binding, and avoids
  collisions.
- Every rename goes through `attemptValidatedRename` — it already
  rejects capture / reserved words / target-in-scope / shadowing. When
  it rejects, skip (do not force).
- On any doubt, skip. Recall is cheap to leave on the table; a
  mis-mapped rename applies a confidently-wrong name a reviewer trusts.

---

## Runbook

### Checks (before every commit — CLAUDE.md)

```bash
npm run check   # typecheck + prettier + biome (complexity ≤ 15) + unit + fingerprint
```

Red/green TDD is the house rule: failing test first, then the fix.

### Develop the reconciliation OFFLINE (no LLM needed)

The pass is deterministic. Build it as a pure module
`reconcile(newText, priorText) → { renames, skipped }` and unit-test it
with small synthetic two-file pairs (colocated `*.test.ts`). No bundle,
no LLM, instant loop. Then validate at scale (below).

### The LLM box (only needed to PRODUCE humanified outputs to test on)

- Endpoint `http://192.168.1.234:8000/v1`, model `openai/gpt-oss-20b`
  (vLLM, 4 replicas, ~28K tok/s), `HUMANIFY_API_KEY=local`.
- Check it's up: `curl -s -m 3 http://192.168.1.234:8000/v1/models`.
  If unreachable, ask Andrew to power it on; do all offline dev
  meanwhile.
- Knobs (overridable env, baked into the harness):
  `HUMANIFY_CONCURRENCY=120`, `HUMANIFY_MODULE_CONCURRENCY=40`,
  `HUMANIFY_MAX_TOKENS=2000`, `HUMANIFY_REASONING_EFFORT=low`.

### Produce a shared-lineage pair + measure (the goal metric)

`experiments/016-diff-noise-convergence/run-chain.sh` re-humanifies v119
with an existing incremental v120 output as prior, then diffs the two
lineage-sharing legs. It needs a prior v120 leg (default
`/tmp/exp016-r1/cc-120/runtime.js`); if that `/tmp` dir is gone,
regenerate it first with the two-leg A/B:

```bash
git worktree add --detach /tmp/humanify-run-020 HEAD
ln -s "$PWD/node_modules" /tmp/humanify-run-020/node_modules
# full two-leg (fresh v119 + incremental v120), ~22 min, writes /tmp/exp020-ab:
PHASE2_OUT=/tmp/exp020-ab bash /tmp/humanify-run-020/experiments/013-bun-cjs-classification/run-phase2.sh
# then the lineage leg (~4 min), diffing v119-lineage vs the v120 leg:
PRIOR_V120=/tmp/exp020-ab/cc-120/runtime.js CHAIN_OUT=/tmp/exp020-chain \
  bash /tmp/humanify-run-020/experiments/016-diff-noise-convergence/run-chain.sh
```

Measure:

```bash
python3 experiments/013-bun-cjs-classification/classify-diff.py /tmp/exp020-chain/runtime-diff.txt
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp020-chain/runtime-diff.txt 15
python3 experiments/016-diff-noise-convergence/classify-decl-kinds.py \
  /tmp/exp020-chain/runtime-diff.txt /tmp/exp020-chain/cc-119-lineage/runtime.js 5
```

To validate the reconciliation itself at scale WITHOUT a re-humanify:
run it as a post-pass over the two existing lineage outputs
(`cc-119-lineage/runtime.js` and the prior `cc-120/runtime.js`), then
re-diff and re-classify. Diff the noise count before/after.

### Baseline for THIS experiment (exp019, current `main`)

lineage noise **1,288 hunks / 40.8% share**, occurrences **2,474**,
genuine **1,868**, buckets transfer-gap 2,271 / asymmetric 145 /
reroll 58. Drive noise down without moving genuine out of ~1,868–1,975.

### Success criteria

- Lineage noise hunks drop from 1,288 (target the `require`-bucket and
  asymmetric residue: `fsModuleVal→fileSystem33`, `Tj_→completionState`
  families leave the diff).
- **Genuine-change hunks stay ~1,868–1,975.** A rise means a
  reconciliation mis-mapped a real change into a rename — investigate
  before celebrating.
- Parse clean, `checkStructuralInvariant` clean after the pass, zero
  `internalErrors`.
- Spot-check the applied renames (read-only mode dumps them): every one
  should be a name a human agrees is "the same binding."

---

## Guardrails / issues to avoid (project law)

- **Precision over recall.** A wrong name applied confidently is worse
  than a minified leftover — a reviewer trusts it. This is why every
  gate defaults to skip.
- **Every rename goes through `attemptValidatedRename`**
  (validated-rename.ts:165) — never raw `scope.rename` in reconciliation
  code, never textual line replacement (it desyncs scopes and other
  references).
- **The structural invariant is the net.** `checkStructuralInvariant`
  (output-validation.ts:115) hard-fails if anything but binding names
  changed. Run it AFTER reconciliation. It is the only correctness
  guarantee that works on artifacts we cannot execute (Bun bytecode).
- **LLM is for naming only, never code rewriting.** This pass touches
  names only, mechanically — no LLM. Keep it that way.
- **Don't scope-creep into multi-identifier complex hunks.** The tail is
  long and flat; Tier 1 + the group-by-binding corroboration is where
  the confident wins are. Report (log) what you skip so "covered
  everything" is never implied.
- **Biome cognitive complexity ≤ 15**; colocated `*.test.ts`;
  `npm run check` green before commit; commit incrementally with
  descriptive messages; work on a branch (the current one is
  `exp020-tail-polish`); do NOT merge to main — Andrew reviews and
  merges (the repeated pattern: merge prior exp, branch the next).

## Code anchors (verified 2026-07-09, re-verify before editing)

- Plugin output path: `src/rename/plugin.ts` — `generate` (569),
  `checkStructuralInvariant` (562), `validateGeneratedOutput` (159/573),
  `priorVersionCode` option (110). Insert the pass between generate and
  the return, gated.
- Validated rename: `src/rename/validated-rename.ts` —
  `attemptValidatedRename` (165), `fastRenameBinding` (123, rewrites
  `referencePaths` at 144 + constant violations).
- Structural invariant: `src/output-validation.ts:115`
  (`checkStructuralInvariant`).
- Rename-noise detector to port: `norm()` in
  `experiments/014-rename-noise-elimination/attribute-noise.py`.
- Eligibility (whether a leftover token is even renameable):
  `src/rename/rename-eligibility.ts` (`createIsEligible("bun","bun")`).
- Lineage protocol: `experiments/016-diff-noise-convergence/run-chain.sh`.
- Two-leg A/B: `experiments/013-bun-cjs-classification/run-phase2.sh`.

## Adjacent, explicitly OUT of scope

- Literal-arg hashing (probed this session,
  `probe-literal-args.ts`): require/import-scoped splits only 10
  functions; all-string-args splits 309 but trades match STABILITY
  (reworded log/error strings) for discrimination. Not worth it, and it
  does NOT touch the `require("fs")` bucket (88 identical `"fs"`
  literals — a within-bucket problem). Skip.
- Improving the matcher/lineage going forward (the reconciliation is
  deliberately a diff-cleanup, not a matcher change).
- Runtime verification of the rename flow (future: fetch recent npm
  versions — `npm/package/cli.js` is plain runnable JS, unlike the
  116-120 binary decompiles — humanify, and execute `--version`/`--help`
  to prove behavior is preserved; complements the invariant with an
  executable check).
