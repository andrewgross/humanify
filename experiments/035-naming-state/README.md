# 035 — Deterministic naming state (kill the classifier, close the census)

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
research-log entries read *Idea → Evidence (table + linked page) →
Conclusion*; outcomes are **landed** or **failed** with numbers;
totals-first tables (TOTAL at top, REMAINING at bottom).

## Why this experiment exists (2026-07-24 findings)

The minted-leftover census said 165 bindings across the eval pairs were
"never named". Per-binding classification (54 on pair 216, via the
strategy trail + diagnostics of the guard probe) showed that is THREE
populations, none of which is "the LLM failed":

| n | population | example | real cause |
|---|-----------|---------|------------|
| 37 | never entered any naming path | `do7Function`, `T7Class`, `sm6Factory`, `h1Regex` | some deterministic pass BUILT a name from a minted stem; the result no longer qualifies for the sweep |
| 9 | statement-twin inherited `_` names | `_` | underscore-convention bindings, faithfully inherited |
| 8 | the LLM's answer, decorated or misflagged | `fsPromises_`, `pathModule_`, `w3cTraceContextPropagator` | collision decoration appends `_` to a GOOD name; `w3c` is a domain stem |

Consequence: the guard-idempotence fork (034 RESULTS sub-experiment 8)
was posed against an inflated population. Fix the producers and the
classifier, and the genuinely-cycling set likely shrinks to single
digits per pair — possibly dissolving the fork.

## The work, in order

### A. Census classifier: decorated names are not mints — LANDED? (do first, small)
`isBunToken` flags any trailing `_` before `isDecoratedDescriptive`
gets a say. Fix: a trailing-underscore name whose STEM passes the floor
is decorated-descriptive, not minted (`fsPromises_` ok, `M2_` still
minted). Add `w3c` to `DOMAIN_STEMS`. Red tests in
`src/rename/minted-census.test.ts`; probe not required (classification
only — verify output byte-identical like the last classifier fix).

### B. Collision-decoration history: why `_` instead of an LLM retry?
User question to answer with evidence: when the LLM's name collides
(`fsPromises` taken), we append `_` — where, and why not re-ask?
Suspects to read: `attemptValidatedRename` callers on the LLM apply
path, `decoration-retry.ts` (exists! why didn't it undecorate these?),
`retry-batcher` (off in wave mode — relevant?), uniquify-then-name
(exp015). Deliverable: the exact code path + a design choice: (i)
LLM re-ask with "name taken" context (bounded retries), (ii) better
deterministic suffix from code evidence, or (iii) keep `_` but track it
as "decorated", never "minted". History tooling: strategy trail +
`renamed[]` in diagnostics has per-round trails already.

### C. The `<mintedStem><Word>` producer (the 37)
Find who manufactures `do7Function`/`T7Class`/`sm6Factory`/`h1Regex`.
Suspects: `class-id-floor.ts` `deriveExpressionInnerNames` (its
`derivationSource` claims to refuse minted sources — verify), the
split-namer mechanical stem (exp024, BAD_STEM list), grammar naming
tiers (human-layout). Then either fix the producer (derive nothing from
a minted stem) or make the coverage sweep include its output. Trail
gap: floor/derivation passes are NOT instrumented in the strategy trail
— instrument them while there (they should record like every tier).

### D. Deterministic naming-state ledger (the architecture ask)
Replace shape-classification with ground truth: we ALREADY track
lifecycle per node (pending/transferred/skipped) and per-tier attempts
(strategy trail). Extend to a complete per-binding terminal state:
`named-by-<tier> | inherited | decorated | marked-fine | untouched`,
where **marked-fine** covers deliberate keeps (SHORT_WORDS locals, `_`
convention, vendor-skipped) and **untouched** is exact, not inferred.
The census then = `untouched` (+ optionally `decorated`), computed from
bookkeeping, no `isBunToken` needed. Likely shape: the identifierLedger
gains a `terminalState` rollup fed by the trail + floor instrumentation
from C; self-hop + eval gates unchanged. This also gives B its history
for free.

### E. Re-pose the guard fork with honest numbers
After A–C, re-run the guard probe + duo-rebased eval; measure the
actually-cycling population. Then decide (options in 034 RESULTS
sub-experiment 8; standing recommendation: prior-name fallback (b) IF
the set is still material, else keep (a)). Unblocks branches
`feat/catch-and-swaps` (catch params + private swaps — features
exonerated, verdicts contaminated) and re-judges `feat/below-floor-guard`
ordinal tier separately.

## How to run everything (copy-paste)

Probe one pair (same-session A/B; ~15 min cached):
    cd /Users/andrewgross/Development/humanify-lever1v2   # branch worktree
    NODE_OPTIONS="--max-old-space-size=14336" npx tsx src/index.ts \
      /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js \
      --split --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
      --api-key local --reasoning-effort low -c 32 -o /tmp/probe-X \
      --llm-cache /tmp/eval-work/llm-cache \
      --prior-version /tmp/eval-work/floor-guard-rebased/2.1.215-rebased/.humanify/humanified.js \
      --diagnostics /tmp/X-diag.json
    npx tsx experiments/034-eval-harness/analyze.ts /tmp/probe-X/.humanify/humanified.js \
      <prior> /tmp/probe-X/.humanify/split-ledger.json <priorLedger> /tmp/stub-stats.json "2.1.215->2.1.216"
    # ALWAYS also run a same-session CONTROL from main when judging deltas.

Full eval (new reference; auto evidence pages + self-hop invariant):
    REBASE_PRIOR=1 experiments/034-eval-harness/run.sh <label> [workdir]

Reports: `npx tsx experiments/034-eval-harness/trail-report.ts <diag.json> out.html <fresh> <prior> [ledgers]`, then `open out.html`.
Mint census by name: adapt the list-minted snippet (see 034 RESULTS sub-exp 7/8 shell history) or extend trail-report.

Gates before any merge: `npm run check` (READ the verdict, never chain
push), `npx biome check <file>` per touched file, novel/realLn
byte-frozen, boots (`cd /tmp/probe-X && bun run.cjs --version` and a
live `-p`), self-hop for anything touching naming determinism. Work on
branches in ../humanify-lever1v2; never edit main mid-eval/walk;
fixture symlinks are worktree-local (real copies: ../humanify-percache).

## Current state pointers (2026-07-24)

- main green @ 909964e; reference = `floor-guard-rebased` (mints 165,
  self-hop pass was CACHE LUCK — see 034 sub-exp 8).
- Parked branches: `feat/catch-and-swaps` (blocked on E),
  `feat/below-floor-guard` (ordinal tier, failed separately).
- Evidence pages: ../034-eval-harness/results/family-rotation-ceiling/pages/
- Artifacts that informed this doc (in /tmp, may not survive):
  probe-floor-216 + floor-216-diag.json (the 54-binding classification).
