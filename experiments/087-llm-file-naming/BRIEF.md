# 087 — BRIEF (hypothesis): LLM-named fresh mints instead of first-function stems

> Origin: Andrew, 2026-08-20 — "our naming should not just be the first
> function in the file, but instead a name from an LLM based on the contents
> of the file itself; it would reduce overlap in naming, and we could re-use
> our existing 'don't use these names' and retry logic. However, we must be
> able to reliably identify a file for placing the same functions back into
> it for this to be effective."

## Scope: MINTS ONLY

A module's path is minted ONCE — the first release it appears — and the
ledger carries it verbatim afterwards. So the LLM ask is one-time per new
module, and cross-release stability never depends on LLM consistency; it
depends on the MATCHER re-identifying the module (Andrew's stated
precondition). exp082/085 hardened exactly that: misassigned filenames on
the busy hop are now zero. Inherited paths are untouched by this proposal.

## Why (measured motivation)

- `moduleStem` = first hoisted declaration. The stripAnsi case: a new
  4-statement module whose first function is `stripAnsi` but whose content
  is feature-flag plumbing minted `strip-ansi`, collided with the retained
  `strip-ansi.js`, and became `strip-ansi-2.js` — 83 line-pairs of alias
  churn plus a misleading name. A content-level name would collide with
  nothing.
- 5 `-2` files remain on the busy hop, all legitimate collisions — the
  suffix is the mechanical namer failing to find a distinct name.
- Reuse: the rename pipeline's avoid-list + retry + name-legality gate is
  the exact contention machinery this needs; deterministic stem stays as
  the fallback when the model yields nothing legal and free.

## Task 0 (sizing, no code) — quote only after sampling

1. Fresh mints per hop on the walk (213 cold is all mints; 214–216 are the
   real rate).
2. Collision rate: what fraction of mints hit `claimPath`'s `-2+` branch.
3. **Stem-tier dependency: how often is `stem-corroborated` the ONLY tier
   that matched a module** (walk ledgers, offline replay — deterministic).
   LLM names break the prior-basename ↔ fresh-stem comparison for LLM-named
   files, so this number is the safety budget. If ~0, the design is clean;
   if not, the stem tier needs a ledger-carried mechanical stem alongside
   the display name before this ships.

## Design sketch (after task 0)

- At mint time: prompt with the module's statements (or a bounded summary),
  the taken-names set, and naming guidelines; validate with the existing
  legality gate; retry on collision; fall back to `moduleStem`.
- Determinism note: mints become draw-dependent within a run. Acceptable —
  the ledger freezes the name at first mint, which is the property the
  cross-version goal needs; run-to-run naming is already a fresh draw
  everywhere else (exp083).
- Validation: cold walk; `novel`/`realLines` exact; `-2` count and
  matcher counters compared directly; boot gates.

## Task-0 RESULT (2026-08-20, walk ledgers, all offline/deterministic)

1. **Fresh mints per hop:** 0 (calm 213→214), 1 (calm 214→215), **30 (busy
   215→216)** — steady-state LLM ask volume is ~30 per busy release. The
   cold start mints all 4,819 at once; whether cold-start naming also goes
   LLM is a separate scope call (one-time event, judged by steady state).
2. **Collision rate:** 2 of 30 busy mints hit `-N` (6.7%); 3 of 4,819 at
   cold (0.06%).
3. **Stem-tier dependency: ZERO.** Replaying all three hops with
   `tierStemCorroborated` deleted changes **0 matches** — every module it
   claims is also caught by a later tier on these hops. The safety budget
   for LLM names breaking stem comparison is empty on measured data.
   (Caveat: tier C earned its keep on the 85→86 era; before DELETING it,
   re-check there. For exp087 it only needs to be non-load-bearing, which
   it is.)

**Verdict: build is green-lit by the numbers.** ~30 asks per busy release,
existing contention/retry machinery, deterministic stem fallback.

## Shipped increment + pre-registered walk predictions (2026-08-20)

`mintNamer` in `FossilPlacementOptions`, wired warm-hops-only in unified.ts
(the existing `SplitNamer` + `acceptProposedName`, previously dormant on the
fossil path). Fallback ladder: invalid proposal → mechanical stem; taken
path → mechanical stem (never a `-2` off a creative name). `assignFossil`
is async now; `stats.llmNamedMints` counts adoptions.

Walk predictions (baseline = /work/exp086-walk):

- matcher counters: FINAL MATCH SETS identical on every hop (stem tier is
  zero-load-bearing; only tier attribution may shift on hops whose prior
  ledger carries LLM-named files).
- busy hop: ~30 mints named by the model (minus fallbacks); suffixed `-2`
  files 5 → ~3 (the two collision mints avoided); headline churn within the
  35-line floor (a new module's importer lines are new either way).
- `novel`/`realLines` EXACT (986/122,066 busy, 146/33,135 calm).
- calm hops: 0-1 mints — inside the 32-line spread.
- boot gates pass on all four trees.

## WALK VERDICTS

- Walk 3 (2026-08-20): **harmless no-op** — the namer batch died on model
  context (744,359 > 32,768 input) because siblings shipped every claimed
  stem tree-wide; every mint silently fell back to its mechanical stem and
  all exactness gates held. Fix: siblings = target folder's stems, cap 20.
- Walk 4 (2026-08-21): **CONFIRMED — 30/30 mints LLM-named, 0 `-N`
  collisions (was 2)**, names read as content summaries (`html-escape.js`,
  `env-processor.js`, `jwt-token.js`). `novel`/`realLines` exact; calm hop
  inside spread.
