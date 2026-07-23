# 036 — Interchangeable assignment (stable names for indistinguishable siblings)

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
research-log entries read _Idea → Evidence (table + linked page) →
Conclusion_; outcomes are **landed** or **failed** with numbers;
totals-first tables; ceilings measured before builds.

## Why this experiment exists (2026-07-23 findings)

After exp035 (fossils swept, guard exemption merged, census fully
accounted), the 215→216 noise decomposes to essentially ONE phenomenon.
Of ~1,180 diff-visible noise lines:

| noise shape                | visible ln | st      | share of visible diff |
| -------------------------- | ---------- | ------- | --------------------- |
| **family-bucket rotation** | **1,017**  | **361** | **5.45%**             |
| outer-echo                 | 73         | 45      | 0.39%                 |
| property-drift             | 71         | 1       | 0.38%                 |
| internal-rename            | 19         | 6       | 0.10%                 |

The same phenomenon is the LAST structural self-hop channel: the duo
re-judgment (exp035 task E follow-up) showed ~26 ln/hop of LLM re-draws
for [family](../034-eval-harness/VOCABULARY.md#family-bucket) members
whose prompts depend on mutable context (`callbackRef` ×5 bindings, the
noop family) — main's byte-identical self-hop currently rests on CACHE
PINS for exactly this class, not on determinism.

Concrete faces from the 215→216 diff (top flips):

| mechanism                         | examples                                                                                                                                          | lines |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| close-match synonym re-draws      | `importSummary→importResult` ×17, `errorMessage→messageText` ×15, `orphanedWorktreePath→worktreeDir` ×13, `runningConfig→activeDaemonConfig` ×11  | ~150+ |
| decoration-ladder shifts/stacking | `upstreamConfigVal→upstreamConfigValVal` ×15, `upstreamConfig→upstreamConfigVal` ×10, `sessionPayload→sessionPayloadVal` ×9, `prInfo→prInfo_2` ×8 | ~60+  |
| vendor-family numbering rotation  | `React124→ReactJSX4` ×11, `React219→reactLib26` ×8                                                                                                | ~20+  |
| tiny-local families               | `i→idx` ×24, `A→w` ×8 (census-invisible no-digit mints)                                                                                           | ~30+  |

**The key reframe.** For PROVABLY-interchangeable members, any
consistent assignment is equally correct — the members are semantically
indistinguishable, so "wrong pairing" is unobservable. The goal is
**stability, not identification**. [Precision over recall](../034-eval-harness/VOCABULARY.md#ceiling)
still gates WHO enters the pool (only proven-interchangeable members;
distinguishable members must keep going through evidence tiers), but
INSIDE a certified pool the assignment just has to be deterministic and
prior-anchored.

## Constraints already measured — do NOT rebuild these

| idea                                                 | verdict                                                                                                                                    | evidence                                                                         |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| Source-order ordinal pairing (leftover-ordinal tier) | **failed** — self-hop violated 36 ln; pairing not stable across re-parse                                                                   | 034 RESULTS sub-exp; parked on `feat/below-floor-guard`                          |
| Identity-grade second pass over ambiguous pools      | **failed** — 10/1,420 recoverable; evidence neighborhoods are ISOMORPHIC (leaves without matched callees; callers reference whole buckets) | 0f30987; probe kept (`HUMANIFY_AMBIGUITY_PROBE`, `ceiling-identity-recovery.ts`) |
| Family-rotation head-flip repair                     | **failed** — 0 safe statements                                                                                                             | 034 results/family-rotation-ceiling sub-exp 1-5                                  |
| Echo-web fixpoint                                    | **failed** — 1 rename                                                                                                                      | same                                                                             |
| Hash-twin consumer pass                              | **failed** — 18 pairs / 371 ln on 86, 0 elsewhere; coupled webs need most roots at once                                                    | same                                                                             |
| Duo/catch-and-swaps as the cause of self-hop flips   | **exonerated twice** — first-leg settles identical to main; the flips are prompt-perturbed family draws                                    | exp035 E follow-up                                                               |

Net constraint set: the assignment key must be **reparse-stable**
(ordinal position is not), **content-external** (content evidence is
isomorphic inside the pool), and **prior-anchored** (stability comes
from the prior leg, nowhere else).

## The ideas, ranked

### 1. Prior-anchored pool assignment (the main lever)

For a certified interchangeable pool with EQUAL counts on both sides
(same statement hash, same evidence key, roles agree), assign fresh ↔
prior members to MAXIMIZE name preservation instead of leaving them to
close-match/LLM draws: anchor on surrounding-context agreement
(enclosing-statement neighbors, ledger file assignment), and break the
final ties with a deterministic reparse-stable key derived from the
PRIOR side (e.g. prior name sort order — stable because the prior is a
frozen artifact), never from fresh source order. Inside a certified
pool this is safe by construction; the certificate is the precision
gate.

### 2. Content-unchanged family members skip the LLM (self-hop closer)

The 26 ln/hop channel: a family member whose OWN statement is a
[twin](../034-eval-harness/VOCABULARY.md#twin) and whose slot has a
prior name should settle deterministically with that name instead of
re-reaching the LLM with a context-dependent prompt. This is fork
option (b) resurrected, but scoped to content-unchanged members —
precision-safe because nothing about the binding changed. Converts the
self-hop invariant from cache-pinned to genuinely deterministic.

### 3. Prior-name-first conflict allocation (kills ladder stacking)

`upstreamConfigVal→ValVal` and `prInfo→prInfo_2` happen because the
conflict ladder allocates by CLAIM ORDER each run. Change: when a
suggestion collides, consult the binding's prior-slot name FIRST — if
the prior name is free, take it before minting a ladder variant. The
276-strong `fsPromises` family then keeps its numbering identity
across versions instead of re-earning it.

### 4. Vendor-family numbering anchor

`React124→ReactJSX4`: vendor names carry over by path, but the
NUMBERING of same-vendor copies rotates. Extend the vendor carryover to
pin instance numbers by content hash of the vendor copy. Smallest
lever; measured ~20 visible lines on 216 — build only if A prices it
above trivial.

### 5. Hard-mode self-hop gate (certification, not a fix)

`SELF_HOP_NOCACHE=1`: run the self-hop invariant with the LLM cache
disabled. Passes only when NO binding's name depends on a draw — the
honest definition of "the channel is closed". Today main would fail it
(the ~5 pinned residue); after ideas 1–3 it should pass. Add as an
opt-in leg in `run.sh`, never blocking the normal sweep.

## The work, in order

### A. Ceiling: decompose family-bucket noise — DONE (2026-07-23)

**Idea.** The family row is an upper bound; price each mechanism before
building. Tool: `ceiling-family-assignment.ts` (canonical occurrence
signatures — property tokens verbatim, everything else first-occurrence
numbered — so "zeroable" means an assignment reproduces the prior
byte-exactly; conservative on string contents).

**Evidence** (both sample pairs, current reference generation):

| class                                 | 215→216            | 197→198           | read                                                                             |
| ------------------------------------- | ------------------ | ----------------- | -------------------------------------------------------------------------------- |
| TOTAL family-noise                    | 361 st / 1,017 ln  | 773 st / 1,858 ln | 100%                                                                             |
| **zeroable (idea 1)**                 | **297 ln (29%)**   | **839 ln (45%)**  | assignment reproduces prior byte-exactly                                         |
| name-churn (idea 2)                   | 161 ln (16%)       | 413 ln (22%)      | pool's name inventory changed — needs prior pinning, not assignment              |
| membership churn                      | 559 ln (55%)       | 602 ln (32%)      | bucket counts differ = largely REAL same-shaped additions/removals; NOT a target |
| decoration / vendor flips (ideas 3/4) | 44 / 94 flip pairs | 59 / 139          | token-level, minor WITHIN family rows                                            |

**Conclusion.** Ideas 1+2 together own ~40–65% of the family row
(~460 ln on 216, ~1,250 ln on 198) — GO on both, in that order. The
membership class confirms the diff-ledger family row overstates
fixable noise (55% on 216 is real same-shaped change); follow-up:
split the ledger's family row into rotation vs membership using this
signature machinery so the human-facing report stops overstating.
Ideas 3/4 within family noise are minor — idea 3 survives for the
stacking pathology (`ValVal`) and allocation determinism, not line
count; idea 4 is deprioritized (build only if it blocks the hard-mode
self-hop).

### B. The interchangeability certificate — LANDED (merged with the eval boot gate)

`certifyInterchangeablePools` in `fingerprint-index.ts`: the sound
half of the failed leftover-ordinal tier — pool extraction (ambiguous
priors grouped by exact candidate set) plus the gates (equal counts,
every candidate unmatched, ONE non-null
[evidence key](../034-eval-harness/VOCABULARY.md#evidence-key) across
BOTH sides) — as a READ-ONLY certificate; it assigns nothing. Sized by
the original tier's probe: **207 pools / 1,025 of 1,420 ambiguous
priors on 216 (72%)**. Tests include the reparse-stability proof the
ordinal pairing died on (same sources parsed twice → identical pools)
and a churn-refusal case (3:2 never certifies). Landed alongside: the
**eval boot gate** — `run.sh` now runs `--version` + a live `-p`
round-trip on every pair's runnable tree (`EVAL_BOOT_PROMPT=0` skips;
verdict in `<TO>-boot.json`, loud on failure) — an output that does
not run is invalid regardless of its noise KPIs.

### C. Prior-anchored assignment — BUILT, probe gates ALL GREEN, eval in flight

`assignInterchangeablePools` (branch `exp036-c-assignment`): inside a
certified pool, pair prior↔fresh to maximize agreement with
ALREADY-MATCHED surroundings — matched callers/callees (weight 2),
matched bundle-order neighbors (weight 1) — greedy with a
deterministic paired-session-order tie-break; the un-anchored residue
falls back to paired order. On a self-hop every anchor matches its
identity, so identity pairing wins by construction — exactly where
fresh-source-order pairing died. Unit fixtures prove the three
properties: crosswise anchoring on swapped bundle positions (source
order would cross them up), identity on self-hop, determinism.

Probe gates (216 vs the same-prior eval-leg control):

| gate                  | result                                                                               |
| --------------------- | ------------------------------------------------------------------------------------ |
| noiseLn               | 7,541 → **6,776 (−10.1%)**                                                           |
| novel / realLn        | 986 / 122,066 — byte-frozen                                                          |
| terminal-state ledger | `mintedUnaccounted` 0; census in band                                                |
| tier mechanics        | +481 exact-match settles; fn-name pins 99→5, votes 97→45 — pools settle mechanically |
| self-hop              | **BYTE-IDENTICAL** (bundle + ledger)                                                 |
| boots                 | `--version` + live `-p` ✓                                                            |

Full `REBASE_PRIOR` eval (`c36-anchored-pools-rebased`, first run with
the boot gate live on all pairs): in flight; merge decision on its
verdict.

### D. Build idea 2 (content-unchanged members skip the LLM)

Separately gated (it changes who reaches the LLM): probe + control +
self-hop; then idea 5's hard-mode self-hop as the certification that
the draw channel is actually closed. Success also unblocks the duo
(`feat/catch-and-swaps` — parked ONLY on this channel).

### E. Build idea 3 (prior-name-first allocation), then 4 if priced in

Each small, each separately probed; 3 touches `resolveConflict`
callers so it rides the same determinism gates.

## How to run everything (copy-paste)

Probe one pair (same-session A/B; ~12 min cached), current reference
prior:

    cd /Users/andrewgross/Development/humanify-lever1v2   # branch worktree
    NODE_OPTIONS="--max-old-space-size=14336" npx tsx src/index.ts \
      /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js \
      --split --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
      --api-key local --reasoning-effort low -c 32 -o /tmp/probe-X \
      --llm-cache /tmp/eval-work/llm-cache \
      --prior-version /tmp/eval-work/e-decorated-exemption-rebased/2.1.215-rebased/.humanify/humanified.js \
      --diagnostics /tmp/X-diag.json
    # ALWAYS pair with a same-session CONTROL from main; judge deltas only.

Self-hop (byte-identical required for ANY naming-determinism change):
re-run with `--prior-version <own output>`; `cmp` both bundle and
split ledger. Full eval: `REBASE_PRIOR=1
experiments/034-eval-harness/run.sh <label>`; compare
`leaderboard.ts archive-shipped baseline-main e-decorated-exemption-rebased <label>`.
Gates before any merge: `npm run check` (READ the verdict; never chain
push), `npx biome check` per touched file, novel/realLn frozen, boots
(`bun run.cjs --version` + live `-p`), `mintedUnaccounted` stays 0.
Work on branches in `../humanify-lever1v2` (NOTE: currently holds
parked `feat/catch-and-swaps` — branch from main, do not build on the
duo); never edit main mid-eval/walk; fixture symlinks are
worktree-local (real copies: `../humanify-percache`; trailing-slash
gitignore patterns do NOT match symlinks — use the worktree-local
`info/exclude`, see the consolidation notes in 035).

## Current state pointers (2026-07-23)

- main green @ 8a4e73e (exp035 complete + consolidation); committed
  reference = `e-decorated-exemption-rebased` (noise 3,541 / noiseLn
  64,544 / mints 87 / self-hop 0 via cache pins).
- Eval-leg artifacts for measurement A (may not survive /tmp):
  `/tmp/eval-work/e-decorated-exemption-rebased/2.1.215-rebased` +
  `2.1.216` (+ diag JSONs alongside).
- Duo `feat/catch-and-swaps`: rebased onto exemption main, parked
  SOLELY on this experiment's channel — idea 2 success unblocks it.
- Nothing built yet; task A is ready to run (offline, no LLM).
