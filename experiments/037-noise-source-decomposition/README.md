# exp037 — noise-source decomposition (handoff after exp036 idea 8b)

> ## STATUS — AUDITED by exp048 (cold). Read [`048/RESULTS.md`](../048-family-permute-cold/RESULTS.md).
>
> **Every number in this document was produced through a shared LLM cache**
> ("Reference measurements (all this session, shared LLM cache)"), which
> measurement-pitfalls **rule 10** now forbids for a verdict. Re-measured cold:
>
> - **−239 noiseLn on 215→216 — DIRECTIONALLY CONFIRMED, magnitude unresolvable.**
>   The pass's total attributable effect across four cold hops is **≤467 lines**,
>   and the cold per-hop draw band is **±2,800**, so this harness cannot resolve
>   it. This doc's −239 is the best estimate anyone has; it is also the right
>   order of magnitude (the cold ceiling on that hop is 220).
> - **"self-hop 0" was cache luck.** Cold, the control violates by 24 and the
>   candidate by 34 — and the candidate's violation is NOT caused by the pass.
> - **"85→86 pass inactive" — CONFIRMED cold**: 0 moves shipped.
> - The **memory fix is confirmed**: 2.1.216 split with the pass ON, cold, no OOM.
>
> This doc's mechanism, dead ends, and the positional-assignment negative result
> (+50,606) were NOT re-tested and remain the best record of them.

**Status: reset point.** This doc consolidates what the 8b / interchangeable-bucket
work proved, so a fresh session can start from conclusions instead of re-deriving
them. Read [experiments/034-eval-harness/VOCABULARY.md](../034-eval-harness/VOCABULARY.md)
for terms (noiseLn, self-hop, interchangeable bucket, etc.).

---

## TL;DR

- **Bankable:** a post-render "family-permute" pass (idea 8b, "C1") that
  context-strictly swap-corrects interchangeable-bucket names. On 215→216,
  same-prior A/B: **noiseLn −239 (5,981→5,742), noise −38, self-hop 0
  (bundle+ledger byte-identical), real change frozen** (novel 986, realLn
  122,066 unchanged). Plus a **memory fix** that unblocks 216's split at 14 GB.
- **Dead, proven twice:** stabilizing interchangeable buckets by matching each
  member to "the name/place it had last run" via **declaration position**.
  Measured **+50,606 noiseLn on 216** this session; **+401 on the 85→86 shuffle**
  historically (idea 8a). Do not retry positional/ordinal assignment.
- **Why it's a floor, not a bug:** the residual ambiguous members are genuinely
  _isomorphic_ (callers call the whole bucket; leaves have no distinguishing
  callees). A prior identity-recovery pass recovered **10 of 1,420** (commit
  0f30987). Truly-identical members have no cross-version identity, so no stable
  assignment exists — it's mathematically irreducible via naming.
- **Next lever (unverified):** the 5,742 residual noiseLn is probably **not**
  dominated by bucket rotation. The 216 run shows ~1,951 names came from the LLM
  (1,107 cold + 844 close-match) vs. a small isomorphic residue. **Decompose the
  noiseLn by source before building anything** — the mass may be LLM close-match
  _drift_, a different axis (naming stability / cache), not assignment.

---

## What idea 8b ("C1") is

A best-effort, deterministic pass that runs **post-render** (after LLM naming +
reconcile + sweep, before split), in `src/rename/family-permute-step.ts`, wired
via `finalizeWithFamilyPermute` in `src/rename/plugin.ts`. It groups top-level
bindings by declaration statement hash (leg-stable composition), and within each
bucket reassigns names using the evidence the rendered artifact carries: **name
identity + masked usage contexts** (reference lines with the member's own name
blanked).

The **only safe rule** (`src/rename/family-permute.ts` → `assignBucket`):
a fresh member `f` adopts a restorable prior name `p` **only when their masked
usage-context overlap is positive AND strictly beats `f` staying on its own
name's prior counterpart.** That strict bar is the entire safety property:

- a mint (own-weight 0) adopts a context-supported dead prior name;
- a correctly-placed name is unbeatable → left alone (guards the v1 disaster of
  renaming `getClaudeCodeOAuthToken`→`deviceActionMap`);
- a genuinely cross-placed name is beaten by its true position → swaps back
  (this is what closed the v2/v4 self-hop violations — self-hop went 6→14→**0**);
- a merely _ambiguous_ member (own name as good as any) is **never moved**.

Application (`applyPlan`) is atomic: vacate every source to a unique temp, then
fill targets, so a permutation (A→B while B→A) doesn't collide; the reconcile
step's pure-rename structural invariant is the backstop.

### The memory fix (was misdiagnosed as environmental)

C1 parses the **prior bundle into a full AST** — an AST class no other
prior-touching pass held (the matcher clears its cache via
`clearBabelCacheAfterPriorMatch`; reconcile diffs the prior as a _string_).
Left un-released, that graph survived into the split phase and OOM'd 216 at
14 GB. Fix: `collectPriorByHash` collects prior members then drops the prior AST
before the fresh side is parsed, and `finalizeWithFamilyPermute` calls
`clearBabelTraverseCache()` after the pass. **Proof it was C1, not base size:**
14 non-8b 216 runs split fine at 14 GB, and C1-**off** splits at 14 GB while
C1-**on** was the only OOM. (I initially called it environmental; the user was
right that it was retained-AST pressure.)

---

## The decisive negative result

Trying to pin _ambiguous/interchangeable_ members to the prior by declaration
slot ("the name that sat here last run") is a **net noise amplifier**:

| approach                                             |             216 noiseLn Δ | note                                                       |
| ---------------------------------------------------- | ------------------------: | ---------------------------------------------------------- |
| idea 8a (positional, historical)                     | **+401** on shuffle 85→86 | adjacency anchors are themselves shuffled                  |
| diff-objective + positional tie-break (this session) |        **+50,606** on 216 | big buckets tie on context → position dominates → mispairs |
| **context-strict (stayWeight) — kept**               |      **−239**, self-hop 0 | strict bar never moves an ambiguous member                 |

Root cause is fundamental: **declaration position does not correspond across
versions**, and the members that reach this pass are ambiguous precisely because
they have no distinguishing structural evidence. You cannot recover an identity
that isn't there. See `identity-recovery` (0f30987): 10/1,420 recoverable.

---

## Reference measurements (all this session, shared LLM cache)

Same-prior A/B = rebase FROM with the current tree, humanify TO twice against
that one prior (pass ON vs `HUMANIFY_NO_FAMILY_PERMUTE=1`), analyze both.

| pair            | KPI                   |           OFF | ON (C1 context-strict) |                                     Δ |
| --------------- | --------------------- | ------------: | ---------------------: | ------------------------------------: |
| 215→216         | noiseLn               |         5,981 |                  5,742 |                              **−239** |
| 215→216         | noise (churned stmts) |           364 |                    326 |                                   −38 |
| 215→216         | reloc                 |           214 |                    214 |                                     0 |
| 215→216         | novel / realLn        | 986 / 122,066 |          986 / 122,066 |                                frozen |
| 215→216         | self-hop              |             — |  **0** (bundle+ledger) |                                     — |
| 85→86 (shuffle) | all KPIs              |             — |          pass inactive |                                **+0** |
| 85→86           | self-hop              |      44 (off) |                44 (on) | baseline draw-flake, pass-independent |

`novel` and `realLn` frozen across every ON/OFF is the precision gate: C1 reduces
noise without touching real change.

---

## Code state (uncommitted on branch `exp036-8b-diff-objective`)

Last commit `49fc5dc` is the **old v4** (mutual-unique-best) — everything below is
uncommitted working tree, in the safe/reverted state (14 unit+e2e tests green,
biome clean, `npm run check` expected green):

- `src/rename/family-permute.ts` — `assignBucket`, **context-strict** (the
  diff-objective/positional experiment was reverted; the doc comment records why).
- `src/rename/family-permute-step.ts` — post-render pass; `collectPriorByHash`
  (memory fix), `applyPlan` (atomic swaps), structural-invariant backstop.
- `src/rename/plugin.ts` — `finalizeWithFamilyPermute` (wiring +
  `clearBabelTraverseCache` + `HUMANIFY_NO_FAMILY_PERMUTE` A/B toggle).
- `experiments/034-eval-harness/run.sh` — `EVAL_PAIRS` override (single-pair
  probes) + boot gate (`--version` echo + live `-p` round-trip → `<TO>-boot.json`).
- `src/rename/*.test.ts` — matching tests.
- `dry-run-consumer.ts` (untracked) — **not ours**, pre-existing leftover; ignore.

**Recommendation:** commit the C1 context-strict pass + memory fix + eval-harness
changes as the exp036 conclusion (it's a clean, bankable, self-hop-0 win). It is
NOT yet validated on the full 4-pair eval gate (216 split OOM blocked that until
the memory fix; the fix now needs the other pairs run to confirm no regression).

### Reproduce

- Single-pair A/B: `scratchpad/ab-pair.sh <FROM> <TO> [heapMB]` (rebases FROM,
  runs TO on/off, self-hops, analyzes). Recreate from the pattern below if the
  scratchpad is gone.
- Full gate (needs the memory fix for 216): `experiments/034-eval-harness/run.sh
<label>` then `leaderboard.ts baseline-main c36-anchored-pools-rebased <label>`.
- Toggle the pass off in any run: `HUMANIFY_NO_FAMILY_PERMUTE=1`.

---

## Next session: decompose the noise FIRST

Do not build another assignment mechanism — that axis hit a proven floor. Instead
find where the 5,742 noiseLn actually concentrates:

1. **Bucket the noiseLn by source** on 215→216: isomorphic-bucket rotation vs.
   LLM close-match _drift_ (the 844 close-match + 1,107 cold names) vs. reconcile
   residue vs. other. The determinism stats suggest LLM drift may dominate.
2. If LLM drift dominates → the lever is **naming stability** (cache reuse, close-
   match determinism, prior-name-first allocation), not bucket assignment.
3. Only if isomorphic rotation is a large, addressable slice is it worth
   revisiting — and even then not by position (dead); the only untried angle is
   _not renaming_ provably-isomorphic members (leave a stable content-derived
   token), which trades named-ness for stability and is its own investigation.

## Dead ends — do not retry

- Positional / ordinal / declaration-slot assignment of ambiguous members (+50,606, +401).
- Post-render identity recovery of ambiguous functions (10/1,420; neighborhoods isomorphic).
- Enriching the matcher fingerprint to crack the residue (the residue has no distinguishing detail — that's what "isomorphic" means).
