# Roadmap: cross-version noise reduction

Jargon: see the [vocabulary](../experiments/034-eval-harness/VOCABULARY.md).

**End goal.** The deobfuscated Claude Code history (one commit per released
version, in `~/Development/unpacked-claude-code/claude-code-history.git`) should
diff to show only **real** source change between versions — not the naming and
file-relocation churn our pipeline introduces for code that did not change. This
document ties together what we have tested, the eval that now measures it, and the
ranked ideas to drive the remaining noise toward zero. It is the entry point for
the next agent.

Companion docs (detail): `docs/plan-eval-driven-noise-levers.md` (the three
forward levers in full), `docs/plan-split-assignment-stability.md` (the split
diagnosis), `docs/issue-naming-instability-reconcile.md` (original framing),
`experiments/034-eval-harness/README.md` (the eval).

> **READ THIS FIRST (2026-07-28, latest) — the arc is CLOSED.** After 049
> (reorder 6,148 → 54) and 050, the residual over the four gate hops is **7,714
> git lines, 96.4% of it naming**. [exp051](../experiments/051-naming-residual/RESULTS.md)
> audited that bucket per GIT LINE and found **84.7% of it is not ours to fix**:
> 45.3% is upstream's own permutation of the bundle's dependency-init prologues
> (same dependency multiset, different order — this pipeline never reorders
> inside a function body), and 39.2% is the LLM naming floor. The reducible
> remainder is **1,138 lines across FOUR hops in three unrelated mechanisms**,
> each concentrated on one hop and each an order of magnitude below the harness's
> **±2,800 git lines per hop** resolution floor (measurement-pitfalls rule 11).
>
> **Every ranked idea below §4 is now either shipped or beneath the instrument.**
> Do not open a noise lever off this document without first re-reading exp051's
> RESULTS: the two levers it declined (tree-wide require-alias stability, ≤256 ln
> on one hop of four; private-field renaming, 468 ln) are the biggest ones left,
> and neither can be judged by a cold A/B.

> **Read this first (2026-07-24).** Everything below §1 measures noise as the eval
> sees it — statement-level, **order-blind**. exp037 showed that misses the largest
> avoidable slice of the diff a human actually reviews: statements that are
> byte-identical but emitted in a different position. Measured in git lines on the
> real split trees, REORDER was the biggest noise bucket on every hop (33%/27%/14%/3%).
> exp038 closed it with a load-time dependency model
> (`src/split/load-order.ts`) — reorder is now 4.4%/6.2%/3.2%/0.7% and **no longer
> the largest noise bucket on any hop**. Details and the per-hop tables:
> `experiments/038-dependency-aware-reorder/RESULTS.md`; the decomposition that
> found it: `experiments/037-noise-source-decomposition/FINDINGS.md`. Naming churn
> is now the leading noise bucket on the shuffle hop (13.8% of 85→86), so the
> ranked ideas below are again the front line. The eval still cannot see emit
> order — a within-file-order KPI in 034 remains an open follow-up.

> **Read this FIRST (2026-07-26, latest) — the arc was measuring the wrong
> tree.** Everything below, including the block that calls itself "the current
> state", scores `src/` ONLY. `run.sh` passed `("$OUT/src" "$PRIOR_SRC")` to the
> scorer and nothing else for the whole 033-045 arc, so `vendor/` — **36,201
> changed lines across the four gate hops, 2.4x all measured `src` noise** — was
> never counted while five experiments drove `src/` to a "floor" of ~3,700
> reducible lines.
>
> [exp046](../experiments/046-vendor-noise/) made vendor a scored surface (its own
> `vendorLn` ↓ / `vendorReal` = columns, never folded into `src`) and shipped two
> levers: dropping the rerolled `factoryVar` from the manifest, and inheriting an
> unchanged library body from the prior release. **Vendor churn 36,201 → 9,632
> (−73.4%), down on every hop, `src/` byte-identical, `vendorReal` unmoved, boot
> gate green ×4, self-hop 0.**
>
> Two things from that experiment outrank anything below:
>
> 1. **`structuralHash` is NOT literal-preserving.** It keeps a string's LENGTH
>    and a number's order-of-magnitude bucket only — six of twelve semantic
>    differences are invisible to it. It is a match-candidate key and must never
>    gate correctness. Use `computeStructuralSignature`. exp046's own brief
>    proposed keying body reuse on it, which would have shipped vendored
>    libraries carrying the prior release's endpoints and timeouts.
> 2. **Check what the harness actually measures before believing a floor.** That
>    check has now been run to exhaustion; there is no third unscored surface
>    ([exp047](../experiments/047-vendor-residual/) records the negative result).
>
> **Current front line: [exp047](../experiments/047-vendor-residual/), brief
> only.** The reducible remainder is 6,407 lines of manifest entry-block
> reordering, plus one number exp046 published as real and did not verify.

> **Read this SECOND (2026-07-26) — five experiments later, the ranking has
> changed and two axes are now measured out.** Everything from §1 down predates
> exp040 and its ranked ideas are stale; this block is the current state.
>
> **RELOCATION: solved, 15,699 -> 1,390 git lines (-91.1%) over three
> experiments.** exp041 (content anchor + all-same vote, `-50.5%`), exp042
> (anchor PREEMPTS a name vote when every outer name is a recycled minted
> counter, `-41.8%`), exp043 (preempt on CORROBORATED CONTENT — twin differs
> <=10% of lines — rather than on name shape, `-69.2%`). Real change byte-
> identical throughout; the 118->119 canary ended byte-identical tree-wide on the
> last two. Residue is 6 statements / 294 lines of genuinely rewritten code plus
> ~200 lines carrying no rare literal: **no evidence left to bring.** See
> `experiments/04{1,2,3}-*/RESULTS.md`.
>
> **NAMING: now the largest bucket at 7,616 lines, but only ~a third is
> tractable.** exp044 decomposed it and both of its hypotheses died:
>
> - ~3,300 lines are interchangeable-member ROTATION — the same names
>   redistributed among bindings the matcher cannot tell apart (a 3-cycle:
>   `commandExtractor -> commandRunner -> commandValidator -> commandExtractor`).
>   87.4% of the largest slice by line mass. exp036 already proved this
>   irreducible; conservation rises monotonically with substitution count, so it
>   is not confined to big statements.
> - 566 lines are import-alias churn. **Attempted and FAILED the gate**
>   (naming +80, total diff +3,742, displacements 6 -> 6): refusing a name does
>   not remove the collision, it moves it, and module-binding names feed the
>   split's name votes and emission order, so every deflection cascades.
> - ~2,600 lines of genuine renames are what remains. Whether that is worth
>   attacking is a diminishing-returns judgement, not a measurement question.
>
> **REORDER is now the second bucket at 6,078 lines** and has had no dedicated
> experiment since exp038.
>
> **Two methodological rules earned the hard way in exp040-044**, both from
> hypotheses that a metric confirmed and reading the same data refuted:
>
> 1. Never trust a match you have not eyeballed. Sizing predicates that look
>    right fail silently — one classifier over-reported alias churn 5x by never
>    checking two aliases named the same module; another reported "7 of 8
>    followed another statement" when the answer was permutation, because
>    permuted names ARE the same names.
> 2. A damage ceiling scoped to the directly-affected population under-predicts
>    any change inside the name allocator, whose output feeds placement AND
>    emission order. Measure the cascade, not just the target.

---

## 1. What "noise" is, and how much there is

Measured by the eval harness (see §3) across four version transitions. Per
top-level statement, the cross-version diff splits into:

- **clean** (~92.7%) — structurally unchanged **and** byte-identical: the pipeline
  reproduced the prior name. Good.
- **noise** (~3.5%) — structurally unchanged but text differs → a **naming flip**.
  Reducible.
- **novel** (~3.8%) — new/changed structure → **real** code change. Must stay put.

Separately, on a binding→file axis: **reloc** (~0.4% of names) — a binding that
kept its name but moved home file, dragging every importer's `require`-alias.

**Determinism.** Of 64,493 functions, **98% are deterministic** (exact-transfer /
already-named / nothing-to-rename). Only **3.3% reach the LLM**: 876 close-match
(have a prior counterpart, re-named because the match was not exact) + 1,259 cold
(genuinely new). Module bindings are 97.7% pinned. The ~20k-line run-to-run floor
is the internal locals of those 2,135 LLM functions, and its cause is **concurrent
batch-serving** (each call's `usedNames` context varies with completion order),
not temperature (already 0).

**The key insight.** Every `noise` statement _has_ a structural twin in the prior
(that is why its rename-invariant hash matches) — so it is **not** new code. It got
re-named because the matcher works at a finer function-fingerprint granularity and
classified it as _close_/ambiguous. That means most of the noise bucket is
**deterministically recoverable** if we inherit names at the coarser statement
level (Lever 1).

---

## 2. What we have tried (and what it taught us)

**Merged and working** (the eval shows current `baseline-main` beats the shipped
`archive-shipped` by ~21% noise / ~15% noise-lines / ~11% reloc, with `novel`
identical — pure noise reduction, no dropped change):

- fix-1 single-vote pin, A1 per-id hints, A2 post-LLM snap, A3 decoration fixes.
- exp020 prior-diff reconcile, exp021 naming floor, exp022 prior-aware sweep.
- Vendor-name carryover, capture-rename fix.

**Built but low-yield** (kept, safe, dormant):

- **Split binding-identity tiers** — Lever B (fill) + Lever A (preempt) wired into
  the split. Measured on a real 215→216 run: the production `priorMatchMap` has
  only **5 useful entries** because ~all 22,802 matched module bindings _pin_
  (inherit name AND file). The real novel-name population is ~1,020, of which only
  ~280 have any recoverable identity even in a best-case oracle. Part-4's "2,162
  relocations / 18,833 lines" ceiling was a final↔final **oracle artifact**
  (2,923 of its 3,203 entries re-point a same-name binding to a _different_ prior
  binding — would misfile). Conclusion: matched bindings do not relocate; the 22%
  alias churn is the ~216 _same-name_ relocations, a different lever (Lever 2).

**Refuted** (measured dead ends, keep as record):

- Operator normalization (exp028) — 0 recovery; minifier emits canonical forms.
- `#4` export-set alias inheritance — files never move paths across versions.
- B2 distrust-generic-votes — net −1,389 (throws out legitimate keeps).

**Not a target:** the LLM naming of genuinely-new code (`cold`, no prior) is
legitimately nondeterministic and small; don't chase it.

---

## 3. How we measure now: the eval harness (034)

`experiments/034-eval-harness/` — a repeatable scorecard, now the cross-version
**validation gate** in `CLAUDE.md` (on top of `npm run check`).

- **Run:** `run.sh <model-label>` — one pipeline run per pair (~1 hr for four),
  stored under `results/<model>/` so runs stack up. `summarize.ts` prints a table
  (counts + %); `leaderboard.ts` compares models (deltas vs the first-listed).
- **Pairs:** 85→86 (quiet 80s control), 118→119 (biggest feature), 197→198
  (feature on large base), 215→216 (ongoing, largest base). Config: `pairs.json`.
- **Reference baselines (committed):** `archive-shipped` (what the git history
  shipped — computed from existing outputs, no run) and `baseline-main` (current
  main). Beat these.
- **Metrics:** determinism (from the new `--stats-json` flag) + the real/noise
  churn split + relocations.
- **Classification = the same rename-invariant hashing the levers reuse:**
  `noise` vs `novel` from the split's identifier-blind `statementHash` (hash in
  both → unchanged → noise if text differs, clean if identical; novel hash → real).
  `reloc` from a ledger `nameToFiles` diff.
- **`REBASE_PRIOR=1`**: if a change alters _formatting_ (not just names), the
  archive `v-1` is no longer a like-for-like base — re-humanify each base version
  with the current pipeline first. Expected and fine.

**Gate rule:** a change passes only if the reducible KPIs (`noise`, `reloc`,
`mints`) fall AND real change (`novel`, `realLn`) does not move.

---

## 4. The ideas (ranked) — status as of 2026-07-22

1. **Statement-level wholesale name inheritance — BUILT (branch
   `feat/lever1-statement-inherit`).** Ceiling measured first (82.7% of noise
   lines have a unique 1:1 hash-twin; 96.5% share the head line — the churn is
   internal locals; `experiments/034-eval-harness/results/lever1-ceiling/`).
   Shipped as `src/prior-version/statement-twin.ts` + an apply phase in
   `prior-transfer.ts` that runs BEFORE exact-match transfers: unique-twin
   bridging via the placeholder-slot walk; cross-pair repair (twins outrank
   ordinal exact matches crossed by bundle reorders); equal-count bucket
   pairing by symmetric matched-reference identity keys; private-name masked
   gate + positional PrivateName transfer; outer-ref votes (2-vote floor only,
   never pin-grade). Precision gates throughout: unique/equal-count only,
   statement callee veto, `bindingRolesAgree`, structural-walk equality,
   pending-or-exact owners, validated renames. What it cannot fix, measured:
   unbound free-identifier drift (UMD probes — no binding to rename),
   changed-leaf echo chains (distinguishing ref unmatched — abstain by
   design), and the LLM floor itself.

2. **Same-name relocation stability — FAILED (measured).** Of 783 relocs, the
   neighbor-hash signal recovers ~0; 60% are multi-file-name `[0]`-order flips
   the metric overcounts; 184 sit on changed statements. True target ~131
   names — below build threshold. `results/lever2-ceiling/RESULTS.md`. Fix the
   METRIC (per-instance moves) before any tier.

3. **Close-match determinism — DONE (2026-07-22).** `--llm-cache` (disk
   response cache, 3bef249) + `--wave-scheduling` (wave-deterministic prompt
   context, fc50a41, **ON by default** since 4343b22): prompts read frozen
   pre-wave state, renames apply at barriers in deterministic order, and
   reruns from a saturated cache are **byte-identical** (measured: 65,706 →
   0 divergent lines on 118→119). Same-session quality A/B halved noise vs
   the free loop; full eval: best noise on record, invariants frozen.
   The free loop and its toggle were DELETED 2026-08-10 — the wave scheduler
   is the only implementation.
   Original framing for the record: Discovered
   2026-07-22: cross-session LLM-serving drift reaches ±2.7k noiseLn per pair
   with byte-identical code (same-session runs agree to ±115). Until batch
   context is order-independent (frozen `usedNames` snapshots) the eval's
   noiseLn cannot grade changes across sessions — variants are graded by
   same-session A/B probes (~12 min per pair: pipeline + analyze). This lever
   now buys BOTH the ~20k floor and a trustworthy eval.

4. **Fn-head single-vote pin — DONE (2026-07-22 eve, merge 7af1218).** The
   biggest class on clean baselines: unchanged tiny functions whose family
   defeats the cascade AND statement-twin; their single exact-matched
   caller's slot vote was collected then dropped below the ≥2 floor, so the
   LLM re-invented the head every hop (get↔retrieve flips). Shared ladder
   extracted to `src/rename/single-vote-pin.ts` (exact testimony, cross-map
   claimant injectivity, role corroboration, validated rename); fn-decl
   heads get the below-floor fallback; close-matched fns excluded. Certified
   vs a byte-identical same-session control: noiseLn −24% aggregate
   (`results/pin-rebased/`, now the reference), invariants frozen, mints
   flat, boots. 198 trades +121 tiny family statements (+32 ln) — prompt-
   context healing, restabilizes on the next cached hop.

5. **Consumer tier in the reconcile — DONE (2026-07-22 late, merge 1518913).**
   Changed-leaf heads (decl hunk genuine → aligned-declaration proof
   impossible) inherit their prior name when ≥2 unchanged callers testify
   from distinct clean hunks, the prior name is dead in the new output, one
   claimant, and the fresh name is novel this hop. Audited on real pairs
   (216: 8 fire, all verified; 119: 3). **Scope discovery:** the corpus gate
   (<50% aligned prior lines) turns the WHOLE reconcile pass off on shuffle
   pairs — 85→86 has only 24.7% alignment — so this tier pays on quiet
   pairs and walk hops, not shuffle pairs.

6. **NEXT — family-bucket rotation / cascade cross-pairing.** After the pin,
   86's residual echo mass (~24k ln noise-status roots + the 27 ceiling-B
   reciprocal pairs ≈5.8k ln) is lazy-initializer module bindings settled by
   the cascade onto the WRONG same-shaped sibling — votes never tally
   (propagation only indexes PENDING nodes). Reconcile cannot reach it
   (corpus gate). The fix lives in the cascade/twin layer: identity-guarded
   re-pairing of settled family members, or vote tallies over settled-but-
   unconfirmed bindings. High line-mass, highest-risk precision work —
   measure a ceiling first (docs/plan-split-assignment-stability.md has the
   adjacent magnet analysis).

---

## 5. How to work these (discipline)

- **Measure the ceiling before building.** Each lever has a cheap, mostly-no-LLM
  measurement that bounds its win; build only if the ceiling justifies it. Reuse /
  extend `experiments/034-eval-harness/analyze.ts` and the `b-ceiling`-style
  deterministic split measurements.
- **Precision over recall.** A wrong name on the wrong binding, or a statement in
  the wrong file, is worse than a missed inherit. Gate hard (unique + corroborated);
  abstain on any ambiguity. The concat invariant validates bytes, not _choice_ — the
  gate is the real safety.
- **Red/green TDD** for the implementation (per `CLAUDE.md`), `npx biome check`
  before committing (stricter than `npm run check` on complexity).
- **Validate on the eval gate:** `run.sh <label>` then
  `leaderboard.ts archive-shipped baseline-main <label>` — reducible KPIs down,
  `novel`/`realLn` unmoved, precision spot-check clean. Work on a branch/worktree,
  not `main`; no walk may be running when you touch `main`.

---

## 6. References

- Eval: `experiments/034-eval-harness/` (README, `run.sh`, `analyze.ts`,
  `leaderboard.ts`, `pairs.json`, committed `results/{archive-shipped,baseline-main}`).
- Inputs: `~/Development/claude-code-versions/inputs/claude-code-2.1.<v>/binary-decompiled/src/entrypoints/index.js`.
- Priors: `~/Development/unpacked-claude-code/versions/claude-code-2.1.<v>/.humanify/{humanified.js,split-ledger.json}`.
- LLM endpoint (local): `http://192.168.1.234:8000/v1`, model `openai/gpt-oss-20b`, `--reasoning-effort low`.
- Levers detail: `docs/plan-eval-driven-noise-levers.md`. Split diagnosis:
  `docs/plan-split-assignment-stability.md`. Prior levers: `docs/plan-naming-noise-levers.md`.
