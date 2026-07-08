# Experiment 015 — megafunction truncation: naming coverage past the 500-line cap

> **Outcome (2026-07-08, branch `exp015-megafunction-coverage`,
> `RESULTS.md`):** noise hunks 6,206 → **5,788**, reroll bucket
> 2,899 → **1,411 (−51%)**, every headline family (`v6→X6`, `$_→w_`,
> `vH→T_`, `J_→W_`, `g→F`, `s→o`, `C→E`) gone from the top
> contributors, genuine change stable, both legs cheaper than baseline.
> Two mechanisms, not one: declaration-anchored code windows fixed
> visibility/cost (574 invisible bindings, 117 context-400-failed
> batches), but the families themselves were **same-named sibling-block
> bindings** that name-keyed collection could never reach — fixed by
> mechanical uniquify-then-name (`<name>_<k>`, 7,017 bindings). The
> brief's truncation→families attribution below is therefore
> historical; see `BASELINE.md` + `RESULTS.md` for what was actually
> measured. Next lever: the same name-keying collapse in the TRANSFER
> path (close-match `nameTransfers`), which caps how fast the newly
> named population converges across runs.

**Goal:** every binding of an oversized function gets a NAME in every run —
deterministically enough that two adjacent versions agree — so the
truncation-driven noise families disappear from the cross-version diff.

This doc is self-contained: pick it up cold and run. Read
`experiments/014-rename-noise-elimination/README.md` + its three results
docs first for the wider campaign; this experiment attacks one of its
three remaining noise sources.

---

## Long-term context

The project goal (see `experiments/014.../README.md`): humanify two
adjacent versions of a minified bundle (fixture: Claude Code v119/v120
Bun `runtime.js`) and make their diff contain **only genuine source
changes**. Exp014 drove rename-noise 22,998 → 6,206 hunks (−73%) via
slot-keyed transfer, ambiguous-bucket cracking, deferred rename retry,
and content-derived factory identifiers. Post-round-3 state
(`ROUND3-RESULTS.md`, baseline for THIS experiment):

- noise hunks **6,206** (76.2% of hunks), occurrences 11,596
- buckets: transfer-gap 7,283 / asymmetric 1,414 / minifier-reroll 2,899
- genuine change ~1,936 hunks (must stay stable — it is the signal)

The three remaining noise sources are: **megafunction truncation (this
experiment)**, wrapper-scope class invisibility, and close-match LLM
naming instability. Don't scope-creep into the other two (notes at the
bottom).

---

## The problem, precisely

`truncateFunctionCode` (`src/rename/processor.ts`, `MAX_CODE_LINES = 500`)
caps the code shown to the LLM; anything past line 500 becomes
`// ... [truncated] ...`. But the **identifier list is NOT capped** —
`collectOwnedBindingInfos` (`src/rename/function-bindings.ts`) collects
every binding the function owns, and `buildRequest`
(`processor.ts:~306`) sends them all. So for a binding declared past the
cap:

1. The LLM is asked to name an identifier whose declaration and usages it
   cannot see. It usually **omits** it → after retries it lands in
   `unrenamed.missing` (477 identifiers in the round-3 diag) and stays
   minified in BOTH legs → Bun re-rolls the token between builds → pure
   reroll noise. Sometimes it **names it blind** → nondeterministic →
   the same binding gets a name in one leg and not the other
   (`aggregateStats → q`, asymmetric) or different names (transfer-gap).
2. Retries can't recover: `extractRetrySnippet` (`processor.ts:~1558`)
   pulls referencing lines from the ALREADY-TRUNCATED code — a past-cap
   identifier has no referencing lines to pull.
3. Transfer can't recover either: these functions are close-matched
   (their bodies drift between versions), statement alignment
   (`src/prior-version/statement-align.ts`) transfers only locals whose
   declaration statement content-aligns, and the exp014 deferred retry
   needs transfer pairs that never exist here.

**Known magnitude (round-3 diff, `/tmp/exp014-round3/runtime-diff.txt`):**
top families `v6→X6` (114 occ), `$_→w_` (111), `vH→T_` (104), `J_→W_`
(96), plus residual same-region churn `g→F` (62), `s→o` (57), `C→E` (56)
— ≈600 occurrences visible in the top list plus a long tail; the round-1
autopsy attributed **642 pure reroll line-pairs to five megafunctions**.
Expect roughly 700–1,000 occurrences of the remaining 11,596 to be
addressable here (reroll + the blind-naming share of asymmetric and
transfer-gap).

The five known megafunctions (positions are from the round-1 beautified
`input.js`; they drift per version — re-derive, don't hardcode):
`515903:4` (938-line SDK-query class method), `517698:2`, `518882:5`
(1,072-line function), `524829:2`, `524905:2122` (1,822-line CLI
`.action(async (commandInput, commandOptions) =>` callback).

**First diagnostic to write:** enumerate functions whose generated code
exceeds `MAX_CODE_LINES`, with counts of bindings declared inside vs past
the cap (grep the run log for `Truncated function` lines, or walk the
graph offline like `experiments/014-rename-noise-elimination/fresh-pool-overlap.ts`
does). That sizes the population beyond the known five and gives the
before/after coverage metric.

---

## Approaches (prioritized; pick with data, TDD each)

**A. Segment-aware naming.** Split an oversized body into segments and
run one naming batch per segment:

- Segment by **top-level statements of the function body** (structural
  anchoring), packing consecutive statements up to a line budget
  (~400). NEVER split by raw line count alone — a version-stable binding
  must land in a segment whose content is the same in both versions, or
  the fix itself becomes a noise source (this is the suspected mechanism
  behind `aggregateStats → q`).
- Assign each binding to the segment containing its **declaration**;
  a binding used across segments is still named once, where declared.
- Each segment's prompt: the segment code + a shared header (function
  signature, enclosing/parent context, callee signatures from
  `buildContext`) + `alreadyRenamed` accumulated from earlier segments
  (the existing prior-transfer pairs mechanism, `processor.ts:~347`,
  shows the idiom) + the shared `usedNames` set so collisions are caught
  at suggestion time.
- Compose with the existing machinery rather than around it: identifiers
  are already split into parallel "lanes" when a function has >25 of
  them (`DEFAULT_LANE_THRESHOLD`, `processor.ts`) — but every lane
  currently gets the SAME truncated code. Segmenting replaces that
  code-selection step; lanes/batching/retries should keep working.

**B. Identifier-complete skeleton.** Keep one call per function but make
the shown code an OUTLINE that covers every identifier: keep all
declaration/signature lines, collapse deep block bodies (e.g. replace
inner statement lists beyond a depth/size threshold with `/* … */`).
Cheaper than A (one call), likely lower name quality (less usage
context). Could serve as the fallback for segment-boundary stragglers.

**C. Hybrid:** skeleton for a first pass (guarantees coverage), segments
only for the identifiers the skeleton round left missing.

Whatever the approach: the LLM naming for these regions is FRESH in both
legs every time until names exist to transfer — so run-to-run naming
stability matters. Reasoning effort is already `low` and temperature is
whatever the local server defaults to; don't rely on determinism, rely
on **coverage** (a named binding in both legs becomes an exact-transfer
candidate in every later run, which is what actually kills the noise).

---

## How to run everything

### Checks (before every commit — CLAUDE.md)

```bash
npm run check        # typecheck + lint + unit + fingerprint snapshots
```

Red/green TDD is the house rule: failing test first, then the fix.
Plugin-level tests with a mock LLM live in
`src/rename/plugin-cross-version.test.ts` (see `countingProvider` — a
provider that tags names with a suffix, so you can assert which
identifiers reached the LLM and what code they were shown). A synthetic
function exceeding 500 lines in a unit test is the cheapest red: assert
every binding is either renamed or visible in the request's code.

### The LLM box (required for A/B runs)

- Endpoint `http://192.168.1.234:8000/v1`, model `openai/gpt-oss-20b`
  (vLLM, 4 replicas, ~28K tok/s aggregate), `HUMANIFY_API_KEY=local`.
- Check it's up: `curl -s -m 3 http://192.168.1.234:8000/v1/models`.
  Ask Andrew to power it on if unreachable.
- Throughput knobs (baked into the harness as overridable env):
  `HUMANIFY_CONCURRENCY=120`, `HUMANIFY_MODULE_CONCURRENCY=40`,
  `HUMANIFY_MAX_TOKENS=2000`, `HUMANIFY_REASONING_EFFORT=low`.

### Full A/B protocol

Naming-coverage changes affect BOTH legs, so run fresh + incremental
(like exp014 round 3, unlike rounds 1–2 which seeded the prior leg):

```bash
git worktree add --detach /tmp/humanify-run-015 HEAD
ln -s "$PWD/node_modules" /tmp/humanify-run-015/node_modules
PHASE2_OUT=/tmp/exp015 bash /tmp/humanify-run-015/experiments/013-bun-cjs-classification/run-phase2.sh
# ~13m fresh leg + ~8m incremental leg. Inputs live at
# /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.{119,120}/...
```

Artifacts land in `/tmp/exp015/`: `cc-119/runtime.js`, `cc-120/runtime.js`,
`runtime-diff.txt`, `cc-{119,120}-diag.json`, logs.

### Measure

```bash
python3 experiments/013-bun-cjs-classification/classify-diff.py /tmp/exp015/runtime-diff.txt
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp015/runtime-diff.txt 20
```

Baseline (round 3, main @ the exp014 merge): noise hunks **6,206**,
share 76.2%, occurrences 11,596, buckets 7,283 / 1,414 / 2,899, genuine
~1,936. Round-3 artifacts may still exist at `/tmp/exp014-round3/`; if
gone, re-derive by running the protocol above from unmodified main.

### Success criteria

- The truncation families (`v6→X6`, `$_→w_`, `vH→T_`, `J_→W_`, `g→F`,
  `s→o`, `C→E`) leave the attribute-noise top contributors; reroll
  bucket drops toward ~2,300 (the wrapper-class share remains — that's
  experiment #13's territory, not yours).
- Diag `unrenamed.missing` drops from 477 toward ~0; `coverage.identifiers`
  shows the recovered population as llm-named.
- Genuine-change hunks stay ~1,900–2,000 (a rise means corrupted real
  diffs — investigate before celebrating noise drops).
- No parse failures, no structural-invariant failures, `internalErrors`
  clean in the run logs.
- LLM cost stays sane: round-3 incremental leg was 5.9M tokens / ~8 min.
  Naming more identifiers costs more — a moderate rise is expected and
  fine; a multiple is not.

---

## Guardrails (project law — from memory + CLAUDE.md)

- **Precision over recall.** A wrong name applied confidently is worse
  than a minified leftover.
- Every rename goes through `attemptValidatedRename`
  (`src/rename/validated-rename.ts`) — never raw `scope.rename` in
  transfer or naming code.
- The rename-only structural invariant (`src/output-validation.ts`)
  hard-fails the run if anything but binding names changes — use it as
  the safety net for aggressive changes.
- Biome cognitive complexity ≤ 15; prettier runs via lint-staged on
  commit; colocated `*.test.ts`; `npm run check` green before commit.

## Code anchors (verified 2026-07-08)

- **The cap:** `truncateFunctionCode`, `src/rename/processor.ts` (~line
  1608, `MAX_CODE_LINES = 500`); logs `Truncated function <id> from N to
500 lines` on the `processor` debug channel.
- Request assembly: `buildFunctionCallbacks`/`buildRequest`
  (`processor.ts` ~275–380) — code, identifiers, windowed usedNames,
  `alreadyRenamed`, `priorVersionCode` context.
- Retry snippet: `extractRetrySnippet` (`processor.ts` ~1558,
  `RETRY_SNIPPET_MIN_LINES`/`MAX`).
- Lanes/batching: `DEFAULT_LANE_THRESHOLD = 25`,
  `DEFAULT_BATCH_SIZE = 10` (`processor.ts` top); shared retry batching
  in `src/rename/retry-batcher.ts`.
- Binding collection: `collectOwnedBindingInfos` +
  `collectShadowedBlockBindings` (`src/rename/function-bindings.ts`);
  the shadowed second pass runs in `processFunction`
  (`processor.ts` ~220).
- Prompt shape: `src/llm/prompts.ts` (`buildBatchRename*`);
  request type `BatchRenameRequest` in `src/llm/types.ts`.
- Close-match local transfer (what already covers part of these
  functions): `src/prior-version/statement-align.ts`.
- Diagnostics JSON: `unrenamed.missing`, `coverage.identifiers`,
  `transferStats` (incl. `retry`) — written via `--diagnostics`.

## Adjacent problems — NOT in scope here

- **Wrapper-scope ClassDeclarations** (~1,100 remaining occurrences,
  `y6→C6`, `HK→qK`): classes aren't graph nodes; votes for them are
  discarded. Separate experiment.
- **Close-match LLM naming instability** (`serializeWithErrorHandling`,
  423 occ — four different names in four runs): prompt-anchoring work,
  not coverage work.
- **Close-match tie hazard**: identical-similarity candidates pair by
  iteration order (`findCloseMatches`). Known, tracked, separate.
