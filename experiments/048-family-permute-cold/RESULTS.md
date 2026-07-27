# 048 — RESULTS (in progress)

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Totals first, every
hop judged on its own, every number below marked **[cold]** or **[cache-pinned]**.

---

## The brief's headline number was already superseded — by a file on its own branch

**This is measurement-pitfalls rule 9, and it landed before a single measurement
was taken.** The 048 brief opens with the family-permute pass as
**"−2,103 noiseLn (−35%) on one hop, shelved for a 6-line self-hop regression"**,
sourced from `036/README.md` §8b's version table. That table's rows are v1–v4.

**The code on `exp036-8b-diff-objective` is none of them.** The branch tip
(`f0c4e30`) is a fifth version — _context-strict_ — committed together with the
handoff doc `experiments/037-noise-source-decomposition/README.md`, which states
its own measurement of the code it ships with:

| source                           | version               | 215→216 noiseLn |            self-hop |
| -------------------------------- | --------------------- | --------------: | ------------------: |
| 036 §8b table (the brief's row)  | v2 greedy             |      **−2,103** |                6 ln |
| 036 §8b table                    | v4 mutual-unique      |             −45 |               14 ln |
| **037 handoff = the branch tip** | **v5 context-strict** |        **−239** | **0** (its own run) |

037 also records 85→86 as **pass-inactive, all KPIs +0**, and its self-hop as
**44 lines on BOTH legs** — pass-independent draw flake — which contradicts the
"references hold strict-0" framing the shelving decision rested on, in the same
document that shipped the code.

So the honest pre-registration for this experiment is **−239 on one hop, ~0 on
the calm hops**, not −2,103. Everything in both tables is cache-pinned and is
re-measured cold below.

### Why that recalibration decides the measurement design

exp047 measured the cold `src/` draw band directly: control-vs-candidate `src/`
moved **−344 / −110 / +354 / −250** per hop for a change proven `src/`-neutral
offline, and its RESULTS state plainly that "a single cold pair cannot resolve a
sub-400-line `src/` effect."

**A −239-line effect is inside that band.** A four-pair cold A/B therefore cannot,
on its own, attribute a `noiseLn` move to this pass — in either direction. The
gate still runs (it is the ship criterion, and it is the only thing that can catch
damage), but it is paired with an attribution instrument that does not depend on
draw luck: **the pass records every rename it applies**, so each hop's KPI delta
can be read against whether the pass fired there at all. A hop where the pass
applied zero moves and the KPI still moved is measuring the model, not the code.

---

## Task 1 — rebase (DONE)

Branch `exp048-family-permute` = the 8 commits of `exp036-8b-diff-objective`
rebased onto `main` (4d92295), plus one rebase-artifact commit.

- **One conflict**, as the brief predicted: `src/rename/plugin.ts`, and it was a
  neighbouring-import collision only (`PriorCarry` vs `runFamilyPermute`), not a
  logic conflict. The wiring hunk applied clean.
- **`run.sh` reverted to main's.** Both versions define `EVAL_PAIRS` and they
  mean different things — main's is a pair FILTER, the branch's was a config
  PATH. Main's wins.
- **`ab-pair.sh` deleted.** It hardcoded a `humanify-lever1v2` checkout,
  `/tmp/eval-work`, and an `--llm-cache` dir: an A/B driver structurally incapable
  of producing a gate-valid number (rule 10). `048/cold-ab.sh` replaces it.
- `npm run check` green: typecheck, prettier + biome, **1,638 unit tests / 0
  fail**, 33 fingerprint tests.

### Byte-identity of the DISABLED pass — PASSED

Two 85→86 runs against the same archive prior, `main` (4d92295) then the rebased
branch with `HUMANIFY_NO_FAMILY_PERMUTE=1`, **both through the LLM cache on
purpose**: the question is whether two code paths issue the same instructions,
which is only visible with the model's own variance pinned. Rule 10 permits the
cache for probing a deterministic surface; no number here is a KPI.

| artifact                                                      | result                        |
| ------------------------------------------------------------- | ----------------------------- |
| `.humanify/humanified.js` (17,780,860 B)                      | **byte-identical**            |
| `.humanify/split-ledger.json`                                 | **byte-identical**            |
| whole emitted tree — `src/`, `vendor/`, `index.js`, `run.cjs` | **`diff -rq` empty**          |
| cache entries written by leg 2                                | **0** — every prompt replayed |

The zero is what makes the comparison mean anything: leg 1 wrote 10 new entries
(main has moved since the cache was filled), leg 2 wrote none, so both legs
answered from the same draws and the only variable left was the code.

Both legs also reproduced the **same** pre-existing `runtime.js` pure-rename
violation, which is main's known draw-dependent flake and not attributable to the
rebase. Note it is present on the control side of everything below.

Post-gate, the pass gained a **move trail** (`FamilyPermuteOutcome.moves`, logged
per shipped rename): reachable only from `finalizeWithFamilyPermute`, i.e. never
on the disabled path, so the identity above still describes the control leg.
`npm run check` green after it — 1,640 unit tests, 33 fingerprint.

---

## Task 2 — the cold A/B [cold]

18 pipeline runs (4 rebase + 4 hop + 1 self-hop per leg), control = pass OFF via
the kill switch, candidate = pass ON, **no LLM cache on either leg**.

Coldness verified all three ways: `LLM CACHE: OFF … gate-valid` on both legs,
**0** cache entries written, vLLM `request_success_total` **13,368 → 18,461
(5,093 live inferences)**, and per-run averages of **470–1,440 ms** — not the
~20 ms that means disk replay.

### TOTALS — leaderboard, control → candidate

| KPI            | control |         candidate |
| -------------- | ------: | ----------------: |
| `noise` ↓      |   3,096 |   3,145 **(+49)** |
| `noiseLn` ↓    |  58,629 | 58,174 **(−455)** |
| `novel` =      |   4,188 |     **4,188 (=)** |
| `realLn` =     | 416,377 |   **416,377 (=)** |
| `relocSt` ↓    |       0 |             0 (=) |
| `newName` ↓    |   4,278 |       4,311 (+33) |
| `mints` ↓      |      85 |          95 (+10) |
| `reorderLn` ↓  |   6,730 |      6,090 (−640) |
| `vendorLn` ↓   |   1,763 |       1,738 (−25) |
| `vendorReal` = |   2,526 |      3,330 (+804) |

### PER HOP — and the number that makes the rest unreadable

`moves` is what the pass actually shipped; `ceiling` is the total lines those
restored names occupy in the emitted `src/` tree — a generous upper bound on the
noise the pass could have removed, since it assumes every occurrence would
otherwise have been churn.

| hop        |  moves | ceiling | layout noise C → X |          Δ | `noiseLn` Δ | novel | realLn |
| ---------- | -----: | ------: | ------------------ | ---------: | ----------: | :---: | :----: |
| 85→86      |  **0** |       0 | 7,688 → 7,628      |        −60 |          +8 |   =   |   =    |
| 118→119 🐤 |     14 |     247 | 3,588 → **724**    | **−2,864** |        −900 |   =   |   =    |
| 197→198    |  **0** |       0 | 2,676 → 2,908      |   **+232** |        +836 |   =   |   =    |
| 215→216    |     34 |     220 | 2,546 → 2,454      |        −92 |        −399 |   =   |   =    |
| **TOTAL**  | **48** | **467** | 16,498 → 13,714    |     −2,784 |        −455 |   =   |   =    |

**The two hops that FAIL "noiseLn down" shipped zero moves. The hop that
"wins" biggest is 91% unattributable.** Both facts come from the same instrument
and they are the result of this experiment.

#### The canary's −2,864 is not the pass

118→119 looks like the best result anywhere in the arc. It is draw luck:

- The pass shipped **14 renames**, whose restored names occupy **247 lines** of
  the emitted tree. The observed swing is **2,864**. The mechanism is short by
  more than 10×.
- The two legs' rebased 118 priors differ by only **26 lines across 5 files**, so
  this is NOT a different-base artifact — both legs hopped from near-identical
  priors. The variance is in the hop itself.
- The two legs' 119 outputs differ by **37,592 lines**, of which **252** mention
  any name the pass touched — **0.7%**. The other 99.3% is the model naming
  things differently on two cold runs.

#### 197→198 is the same coin, other face

Zero moves shipped, and `noise` **+232**, `noiseLn` **+836**, layout-real
**+3,610**. Had the legs been labelled the other way round this would read as
"the pass regresses the large-base hop." It is the same draw band.

**exp047 estimated that band at ±350 per hop. On 118→119 it is ±2,800.** Any
`src/` effect below ~3,000 lines on a single cold hop is unresolvable by this
harness, which is 6× worse than the previously published figure.

### The `=` guards held exactly

`novel` **4,188 = 4,188** and `realLn` **416,377 = 416,377**, identical on every
hop individually (786/786, 1,154/1,154, 1,261/1,261, 986/986 — and 216's
122,066 `realLn` matches exp037's frozen value). **The pass never touched real
change**, which is the one failure it could plausibly cause. This is the
criterion that mattered and it is unambiguous.

`vendorReal` moved **+804**. The pass does not touch vendor, and exp047 already
established this KPI is not draw-stable because it charges humanify's own
vendor-filename rotation as dependency change — but +804 is **4× the ±200 band
exp047 measured**, so the band itself is wider than recorded.

### Self-hop: the shelving criterion, re-measured

| leg                 | self-hop diff |
| ------------------- | ------------: |
| cold control (OFF)  |     **24 ln** |
| cold candidate (ON) |     **34 ln** |

The historic decision was framed as "**6 lines against references holding
strict-0**". Cold, **the control violates by 24**, so that comparison never
existed — confirming exp047's finding on a second independent change.

**And the candidate's 34 lines are not the pass's doing.** Read:

- **control's 24 ln** = two bindings, `ze → sessionId` and `Qo → queryContext` —
  minted leftovers the LLM named differently on the second pass.
- **candidate's 34 ln** = ONE binding, the `require("fs")` alias flipping
  `fs2 → fileSystem57` across its usage sites. It **does not appear in the 216
  move trail**.

The structural argument settles it: **this pass only ever moves a name TOWARD the
prior.** The candidate's violation is a move AWAY from the prior (the base had
`fs2`; the re-run produced `fileSystem57`). The pass cannot have caused it — at
most it failed to prevent it. Both legs' violations are the same class of
chronically draw-unstable binding, and which one flips is luck.

### Gate verdict, criterion by criterion

| #   | criterion                    | result                                                                  |
| --- | ---------------------------- | ----------------------------------------------------------------------- |
| 1   | `noiseLn` down on EVERY hop  | **FAIL** — 86 +8, 198 +836 … **both zero-move hops**                    |
| 2   | `novel` + `realLn` unmoved   | **PASS**, exactly, on every hop                                         |
| 3   | zero pure-rename violations  | **FAIL on BOTH legs** — `runtime.js`, control 1 / candidate 2 (86, 198) |
| 4   | boot gate green ×4           | **PASS** on both legs, `--version` + live `-p` round-trip               |
| 5   | self-hop vs the COLD control | 24 → 34; **+10, and not attributable to the pass** (see above)          |
| 6   | 118→119 canary               | "wins" −2,864, of which ≥91% is draw variance                           |

Criterion 3's violations are the pre-existing draw-dependent `runtime.js` flake —
it appeared identically on `main` and the branch in the Task 1 identity probe, and
the candidate's extra one is on 197→198, a **zero-move hop**. The pass also
**self-discards** its entire plan if it ever violates the pure-rename invariant,
so it structurally cannot ship one.

**Verdict: the gate is INCONCLUSIVE, not failed. The instrument cannot resolve a
sub-3,000-line effect on a single cold hop, and this pass's total attributable
effect across four hops is ≤467 lines.**

---

## Task 3 — reading the renames

All 48 shipped renames were read. **Every restored name exists in the prior
bundle** — the pass restores prior-release names, it does not invent them
(`isValidModel`, `colorGenerator`, `renderDisplayArtifactsPanel`, `getH3`,
`defaultSubscribe`, `getIO5` all present in the 215 prior).

The 216 trail contains exactly the signature the pass was built for:

- a genuine **3-cycle** — `getH3 → defaultSubscribe → getIO5 → getH3`
- a clean **2-swap** — `getCachedEntity ↔ getSkillCacheEntry`

These are family rotations: no name entered or left, and the diff was charging
every usage site of all three.

Names that look alarming in isolation — `skillDiscoveryPrefetcher → audioRecorder`,
`emptyFunction9 → dataProcessor`, `showRemoteEnvironmentSelector →
renderDisplayArtifactsPanel` — are **restorations, not corruptions**.
`audioRecorder` is instructive: it occupies 6 lines of the candidate's tree and
appears **zero** times in the control-vs-candidate diff, i.e. the control has the
same name on the same lines. The candidate's fresh draw had wandered to
`skillDiscoveryPrefetcher` and the pass pulled it back to what the prior release
called it. **The pass's successes are invisible in a leg-to-leg diff precisely
because they make the legs agree.**

### The interchangeability certificate is sound

Worth recording because the neighbouring hash is not: **`statementHash` is
literal-preserving** — it serializes actual `StringLiteral`/`NumericLiteral`/
`RegExpLiteral` values (`src/split/statement-hash.ts:28-42`). It is NOT
`structuralHash`, which keeps only a string's LENGTH and a number's magnitude
bucket and is therefore blind to six of twelve semantic differences. Same
`statementHash` really does mean "differs only in bound names", so a permutation
inside a bucket is a bijection over indistinguishable members. The residual risk
in a swap-correction is bounded by that certificate, not by the naming evidence.

---

## Task 4 — the placement-provenance gate: NOT BUILT, and why

The brief gates Task 3 on "**only if** Task 2 shows the recall/determinism
coupling survives cold". It does not. The determinism cost measured **+10 lines,
attributable to a binding the pass never touched and moving in a direction the
pass cannot produce**. There is no determinism blocker left to disambiguate, so
building a tiebreaker worth "tens of lines" against a ±2,800-line measurement
floor would be unmeasurable by construction. Not built.
