# Exp022 — prior-aware sweep: transfer first, LLM only the residue

Branch `exp022-prior-aware-sweep` (off `main` after exp021 merged). Goal:
make the naming-floor LLM sweep (`--naming-floor-sweep`) cross-version
stable — reuse the prior version's name for a minted binding instead of
asking the LLM for a fresh one, so the sweep clears minified leftovers
without adding lineage diff noise.

## Headline

The mechanism is a REORDER, not a new matcher: with a prior present, the
LLM sweep defers until AFTER the exp020 reconcile pass. The reconcile
pass's asymmetric tier (minified→descriptive, its easy case) transfers
the prior version's name onto every minted sweep target with a clean
positional counterpart — deterministic and cross-version stable — and
the LLM then names only what is STILL minted, which by construction has
no usable prior counterpart. Zero new matching code; the partition
between "transfer" and "LLM" is the reconcile pass's real gate outcome.

Measured offline on fresh same-artifact chains (`run-sim.sh` — every
config shares the same floored/swept legs, so comparisons carry no
run-to-run LLM jitter):

| config (119 leg vs 120 leg, reconciled, diffed)    | noise   | census/leg | genuine |
| -------------------------------------------------- | ------- | ---------- | ------- |
| deterministic floor only (exp021 baseline, re-run) | 658     | 412        | 1,900   |
| independent sweeps (exp021 protocol, re-run)       | 705     | 214        | 1,894   |
| **prior-aware sweep, first hop** (120 fresh-swept) | 685     | 216        | 1,900   |
| **prior-aware sweep, converged** (hop-2)           | **498** | **203**    | 1,871   |

- The deterministic baseline reproduced exp021's 658 EXACTLY (it is
  deterministic); the independent-sweep protocol landed 705 vs exp021's
  709 (inside the ±2.6% leg jitter). Genuine held in the 1,868–1,975
  band in every configuration — no prior name landed on a wrong binding.
- Same-pair, same-artifacts: prior-aware beats independent sweeps on
  noise (705 → 685) at equal census (216 vs 214).
- At the converged steady state (hop-2, below), **both metrics beat both
  exp021 configurations simultaneously**: 498 noise / 203 census, vs
  658/412 (deterministic) and 705/214 (independent sweeps).

## Step 0 — the sizing (measured before any pipeline code)

Of **274** sweep targets on the deterministic-floored 119 leg, with a
fully-swept 120 leg as prior:

- **177 (65%) transferred** via the reconcile asymmetric tier — 680
  corroborating occurrence votes.
- **103 (35%) residue** went to the LLM: 26 named, 77 skipped
  (collisions / declines — the same population the prior leg's sweep
  skipped, so mostly identical minted tokens on both sides).
- Only **12 asymmetric candidates were gate-refused** (5
  target-in-scope, 4 decl-not-aligned, 2 target-visible, 1
  decl-not-clean), leaving just **10 refused pairs** in the final diff.
  The gates hold at 30× the volume they had ever fired at (exp020
  measured the asymmetric tier at n=6–18).

Spot-check: **all 177 transferred declaration lines are byte-identical
to the prior leg after the transfer** — the strictest convergence check
available — and samples read exactly as "the same binding named the same
way as last release" (`H → sourceValue` in `deepCloneFunction`,
`ws → socket` in `let socket = this.ws;`, `c_ → keyEvent` in
`handleKeyEvent = keyEvent => {…}`).

## Where the first-hop 685 comes from (and why it converges)

Pair-level decomposition of the final diffs (direction: 119-value vs
120-value):

| population                           | prior-aware | independent | deterministic |
| ------------------------------------ | ----------- | ----------- | ------------- |
| transfer-gap (both descriptive)      | 463         | 462         | 485           |
| 119 named / 120 minted (fresh names) | 37          | 38          | 23            |
| 119 minted / 120 named (refused)     | 10          | 9           | 12            |
| reroll (both minted)                 | 11          | 11          | 12            |

The 685−658 gap is **transitional flooring cost, not mechanism
failure**: the 37 "fresh name" pairs are bindings the 119 leg named
whose 120 counterpart is still minted (`inputValue↔H`,
`completionState↔Tj_`) — the unavoidable one-time price of driving the
census from 412 to 216 inside a two-release measurement window. Naming a
binding for the first time WILL differ from a prior that never named it.
Each such name transfers deterministically at the next hop — exactly
what the 177 transfers this hop are: last hop's fresh names arriving.

**Hop-2 (converged steady state).** Rebuilding the 120 leg as if it
inherited from the prior-aware 119 leg (reconcile 120-deterministic
toward chainA-119, sweep only the residue — the same mechanism, one hop
later): 208 asymmetric transfers fire, the residue sweep is down to 17
named, and the pair lands **498 noise / 203 census / 1,871 genuine**.
The sweep-attributable asymmetric bucket drops to 29 bindings — BELOW
the deterministic baseline's own 35 — i.e. the sweep now REMOVES more
asymmetric noise than it creates. Honest caveat on 498: hop-2's
reconcile also re-snaps descriptive drift a second time (transfer-gap
463 → 362), which flatters the total; the sweep-specific claim rests on
the asymmetric bucket and the census, both unambiguous.

## What was built

- **`src/rename/sweep-step.ts`** — the deferred sweep as a best-effort
  pipeline step mirroring `reconcile-step.ts`: parse the shipping output
  privately, `sweepMintedNames` through the validated path, prove the
  pure-rename invariant against a local baseline, regenerate; any
  failure discards the sweep and ships the reconciled output unchanged.
- **`src/rename/plugin.ts`** — `isSweepDeferred` (namingFloor +
  namingFloorSweep + reconcilePriorDiff + priorVersionCode + no
  sourceMap): the floor's pre-generate sweep is held back, and
  `maybeRunDeferredSweep` runs it after `maybeReconcilePriorDiff` on the
  reconciled code. Without a prior, the sweep runs pre-generate exactly
  as in exp021. The minted census now walks the FINAL shipping AST
  (post-reconcile, post-sweep), so `--diagnostics` stops over-counting
  bindings those passes resolved.
- **Seam test** (`plugin.test.ts`, red/green): a class-expression inner
  id (escapes every collector) whose derivation is capture-blocked, with
  a descriptive prior counterpart on a clean diff line — the test fails
  on the old order (fresh LLM name wins, reconcile rightly refuses the
  descriptive↔descriptive snap) and passes on the new (prior name
  transfers, the LLM is never asked, the genuinely-new sibling still
  gets LLM-named). Plus step-contract tests in `sweep-step.test.ts`.
- **Harness**: `run-floor.ts --sweep-only` + reasoning-effort
  passthrough; `run-sim.sh` composes the exp020/021 harnesses into the
  three same-artifact chains plus the hop-2 convergence run.

### Why the "defer" framing and not an inline transfer

The brief offered two realizations: defer-to-reconcile (minimal) and an
inline transfer inside the sweep. They apply the SAME renames through
the same gates — an inline pass could only differ by duplicating the
positional matcher (against the unify rule) or by weakening gates
(against precision-over-recall). The sim bounds what inline could add:
the 10 refused pairs. Not worth a second apply surface; defer was
implemented.

## In-pipeline confirmation

A real `--naming-floor --naming-floor-sweep --reconcile-prior-diff`
lineage leg through the actual CLI, against the floored+swept 120F prior
(the steady-state scenario). Total wall: **9m23s**, 1,213 LLM calls,
zero internal errors, output parses, genuine 1,890 (in band). The debug
log shows the exact designed sequence:

```
naming-floor          derived 3 inner id(s), undecorated 0, swept 0 (0 sweep groups)   ← inline sweep DEFERRED
reconcile-prior-diff  snapped 612 binding(s) to prior-version names (444 skipped)
naming-floor          deferred sweep named 19 residue binding(s) (77 skipped)          ← post-reconcile
```

- The residue numbers match the offline sim (19 named/77 skipped in
  vivo vs 26/77 offline — the named delta is LLM decline jitter).
- Census **217** (CLI "minted leftovers", computed on the final shipping
  AST — the census move working) vs 216 offline.
- Noise 701 on this fresh leg vs 685 offline — a fresh leg re-rolls the
  LLM dice on every unsettled function, and 701 vs 685 is inside the
  fixture's ±2.6% leg-to-leg jitter; the same-artifact comparisons above
  are the controlled numbers.
- **`derived 3`** is its own small confirmation: exp021's 328 derived
  class-expression names arrived via the ordinary lineage transfer this
  hop (settled, zero re-derivation) — the derivation is cross-version
  stable in production, exactly as designed.

## Honest caveats

- The strict "steady-state noise ≤ 658" criterion is NOT met on the
  two-release fixture window (685): that window necessarily contains the
  first hop's flooring cost. The mechanism's own noise contribution —
  refused transfers (10 pairs) plus LLM re-name disagreements — is below
  the deterministic baseline's pre-existing asymmetries, and hop-2 shows
  the trajectory (498/203). With only two decompiled versions, true
  three-release steady state cannot be measured directly.
- A prior name that itself looks minted (`ui` for userInterface) can
  never transfer — the reroll/downgrade gates refuse it by design, and
  the residue sweep then re-names it fresh each release until a
  descriptive name lands. A handful of bindings.
- The hop-2 total (498) partially reflects double-reconciled
  descriptive drift, not just the sweep fix; quoted with that caveat
  everywhere.
- 77 residue targets stayed minted (LLM declines/collisions) — same
  population and causes as exp021's 74; census floor stays ~200 until
  those are addressed (decoration retry across the lineage is exp021's
  next-candidate #2).

## Next candidates

1. **Two-phase (temp-name) swap renames** in `runReconcileRounds` — 137
   target-in-scope skips in the prior-aware chain (152 in exp020); the
   biggest single reconcile-side bucket left.
2. **Sweep-decline retry across the lineage** — the 77 stable-minted
   residue targets re-decline every release on the same code window;
   a per-binding "declined last release" memory could stop re-asking, or
   a different window could unlock a name.
3. The transfer-gap reservoir (463 pairs) remains the campaign's main
   noise mass — operator normalization / diff-quality pass territory,
   per the standing note.

## Reproduce

```bash
# LLM box up? (owned hardware — don't ration tokens)
curl -s -m 3 http://192.168.1.234:8000/v1/models

# the whole offline sim (baselines + mechanism + metrics), ~10 min
bash experiments/022-prior-aware-sweep/run-sim.sh

# hop-2 convergence
npx tsx experiments/020-tail-polish/run-reconcile.ts \
  --new /tmp/e022/120D.js --prior /tmp/e022/chainA-119.js \
  --descriptive --apply --out /tmp/e022/hop2-120R.js
npx tsx experiments/021-naming-floor/run-floor.ts \
  /tmp/e022/hop2-120R.js --sweep-only --out /tmp/e022/hop2-120.js
diff /tmp/e022/chainA-119.js /tmp/e022/hop2-120.js > /tmp/e022/hop2-diff.txt
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/e022/hop2-diff.txt 10

# in-pipeline lineage leg (real CLI, prior = floored+swept 120 leg)
EXTRA_HUMANIFY_FLAGS="--naming-floor --naming-floor-sweep --reconcile-prior-diff" \
  PRIOR_V120=/tmp/e022/120F.js CHAIN_OUT=/tmp/exp022-chain \
  bash experiments/016-diff-noise-convergence/run-chain.sh
```
