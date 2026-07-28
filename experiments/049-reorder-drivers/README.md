# 049 — Reorder: what actually drives it, and the export-registrar unpin

> ## STATUS — SHIPPED. Cold-gated on four pairs; the self-hop scare was the draw.
>
> **`src/` reorder 6,148 → 2,462 (−60%) and reviewer-facing `src/` noise
> 13,638 → 10,160 (−25%), DOWN ON EVERY HOP**, with `novel` (4,188) and `realLn`
> (416,377) exact to the digit, `relocSt` 0, and the boot gate green ×4 on both
> legs. 18 cold pipeline runs, 0 cache entries, 4,939 live vLLM requests.
>
> **The self-hop went 16 → 326 cold, and it was NOT the change.** Every hunk was a
> change-in-place — zero statements moved — and the lines were naming wobble on
> minted leftovers, the same class as the control's. Re-run with draws pinned off
> the two byte-identical base bundles: **6 lines and 0 move hunks on BOTH legs**,
> the same binding (`p2sBytes → saltBytes`) in the same place, and the new-emitter
> leg wrote **zero** cache entries — so emit order did not alter a single naming
> prompt, which is what refutes the `priorNames`-cascade worry directly.
>
> **Bundle-level columns rose** (`noiseLn` +2,504, `mints` +14, `newName` +102)
> with no per-hop direction (+400 / +283 / −243 / +2,064). The change is provably
> bundle-neutral — draw-pinned, the bundles are byte-identical and `noiseLn` reads
> 4,770 both ways — so that is the ±2,800 cold band, not an effect.
>
> **Corrections this experiment forced on earlier work:**
>
> - **exp045's "exact" ceiling measured the wrong scope**, closing an axis that was
>   ~2× bigger than it reported.
> - **The noise decomposition agrees with git in aggregate by cancellation, not
>   fidelity** — per-file error 11.6%, cancelling 19.6×.
> - **The decomposition does not generalise across pairs**: 215→216 ranks the
>   aligner residual 5% and last; four pairs rank it 27% and first.
>
> **Still open:** the aligner residual (~1,029 capped lines, now the largest
> reducible bucket) and the ambiguity gate (1,240 lines).

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Read
[`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md) first — this
directory added evidence to rules 2, 4 and 11 in the course of one afternoon.

## Why reopen an axis exp045 closed

[exp045](../045-reorder/) measured reorder's ceiling as **~80% load-order
constraint, ~844 of 6,078 git lines recoverable**, built nothing, and closed the
axis. Its README's STATUS says all three noise axes are "at their floors".

That ceiling is wrong, and the reason is worth more than the number.

`bundleLoadOrderFacts` admits the bundler's lazy-init wrapper as pure by finding
its DEFINITION — the `x && (y = x(x = 0))` shape — in the code it is handed. The
emitter hands it the whole **bundle**, where the helper is defined once, so the
exemption applies to all 580 call sites. `045/barrier-exact.ts` hands it **one
split file**. The helper is defined in exactly one file
(`array-builder/resource-lifecycle.js`); everywhere else it is only CALLED, as
`(0, resourceLifecycle.lazyInitializer)(...)`. So detection returns null,
`pureCallNames` is empty, and every lazy-init registration counts as a barrier —
the block `load-order.ts` itself calls "the largest pinned block of reorder
churn".

| 215→216, 1,994 reorder lines   | CONSTRAINED | RECOVERABLE |
| ------------------------------ | ----------: | ----------: |
| exp045's setting               |       90.1% |      198 ln |
| what the emitter actually sees |       78.4% |      430 ln |

Its comment says it consults "the same facts the emitter's aligner consults". It
does not. This is the failure exp045 documented in its own Task A — reasoning
about a model instead of running it — one level up: it ran the real model, in the
wrong scope.

## What the charge is actually made of

Only **185 statements** carry all 1,994 lines on 215→216. The emitter already
aligns each file to the prior release's order (`stable-split.ts`, "Emit each
file's statements in prior order, not fresh bundle order"), so the residual is
where that alignment is BLOCKED.

Decomposed across all four pairs — and it does **not** generalise from any one of
them:

| pair      |     total | is a barrier | ambiguous hash | barrier between | aligner residual |
| --------- | --------: | -----------: | -------------: | --------------: | ---------------: |
| 85→86     |     1,922 |          122 |            536 |             500 |    **764 (40%)** |
| 118→119   |       260 |           16 |             30 |              42 |    **172 (66%)** |
| 197→198   |     1,940 |           66 |            292 |             950 |    **632 (33%)** |
| 215→216   |     1,968 |          348 |            382 |           1,136 |         102 (5%) |
| **total** | **6,090** |     552 (9%) |    1,240 (20%) |     2,628 (43%) |  **1,670 (27%)** |

**215→216 is the least representative pair for the largest reducible bucket.**
Sizing the aligner residual from it alone puts it at 5% and last; across four
pairs it is 27% and first. Rule 2, paid for again.

## The lever built: the export registrar is not a barrier

Reading the helper is what settles it. In the bundle:

```js
var defineModuleExports = (targetObject, sourceObject) => {
  for (var propKey in sourceObject)
    defineProperty(targetObject, propKey, {
      get: sourceObject[propKey], // <- LAZY. nothing is evaluated here
      enumerable: true,
      configurable: true,
      set: BoundIdentityProperty.bind(sourceObject, propKey)
    });
};
```

It installs lazy getters over a literal of arrow thunks. Nothing it is handed is
evaluated at registration; only the target object must already exist. There are
**580 such calls**, and they were **580 of the bundle's 588 barriers**.

### Why `targetWritingCallNames` and not `pureCallNames`

This distinction IS the safety property, and getting it wrong would have shipped
a boot bug that no KPI would catch.

`pureCallNames` suppresses the effect flag and then walks the arguments, which
records the target as a **READ**. Two reads carry no dependence edge — so a
load-time read of `exportsObj.foo` could legally be scheduled BEFORE the
registration that installs `foo`.

Recording a **WRITE** is the honest model and gives the edges for free:

- write-after-write against `var exportsObj = {}` ⇒ the call can never float
  above its target's declaration. That exact reordering crashed the runnable tree
  in exp037, and it is the reason the blanket barrier rule existed.
- read-after-write against any load-time reader of the target ⇒ readers stay
  after.

Both are tests. **They passed before the change too** — a barrier trivially
cannot move — so they are guards against regression, not evidence the new
behaviour works; the KPI delta is that evidence.

Detection is STRUCTURAL, following `identifyBunLazyInit`'s precedent, never by
name (every name here is LLM-chosen and differs per run). The certificate is the
`get: source[k]` property: it proves values are installed rather than evaluated,
so a helper that eagerly read them will not match.

### Measured [draw-pinned]

Two runs of 215→216 against one fixed prior, flag on and off, both replaying
every prompt — **0 cache entries written by either leg**, so the pre-pass render
is identical and the delta is the flag. (Rule 10 permits the cache for a
deterministic surface; emit order never consults the model.)

| metric             |           OFF |            ON |          Δ |
| ------------------ | ------------: | ------------: | ---------: |
| `src/` reorder     |         1,994 |           550 | **−1,444** |
| `src/` total noise |         2,692 |         1,248 | **−1,444** |
| naming / alias     |      638 / 60 |      638 / 60 |          0 |
| layout real        |        26,833 |        26,833 |      **0** |
| `novel` / `realLn` | 986 / 122,066 | 986 / 122,066 |      **0** |
| bundle bytes       |    33,954,589 |    33,954,589 |      **0** |

Boot: `--version` echoes `2.1.216 (Claude Code)` and the live `-p` round-trip
returns `boot-ok`. All 3,144 emitted files parse. Zero pure-rename violations.

**What this measurement cannot see, and why the cold gate is still required:** it
holds LLM draws fixed, so it shows the direct effect only. 580 statements became
movable, and emission order feeds the split's name votes and the ledger — the
cascade that turned exp044's clean scoping argument into +3,742 lines (rule 5).

## The decomposition agrees with git by cancellation, not fidelity

Worth recording because every share quoted in this arc rests on it.
`diff-composition`'s parts sum to **29,525** where git prints **29,701** on
215→216 — 0.6% apart. Per file:

|                              |                    lines |
| ---------------------------- | -----------------------: |
| SIGNED error (the aggregate) |                     −176 |
| ABSOLUTE error               | **3,444 (11.6% of git)** |
| cancellation factor          |                **19.6×** |

The reorder column survives the check: the over-charged files are mostly
reorder-0, and over-charge whose bulk IS reorder totals ~156 of 1,994 (8%). So
exp045's "the reorder metric is sound" holds, and the error lives in the
real/naming attribution — the token-overlap pairing in `editedLineCounts`.

Two known metric artifacts to subtract before sizing anything on this axis:
`table/skill-docs/files-api.js` is charged **332** reorder lines where git prints
**4**, and `uri-validator/diff-tool/colorizer.js` is charged **52** for a
displacement git does not print at all (its 51 printed lines are new exports, a
new require and a version bump).

## What is left, ranked

| lever                    |                        4-hop size | shape                                                                                                                                                                                                                                                                   |
| ------------------------ | --------------------------------: | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **aligner residual**     | 1,670 charged / **~1,029 capped** | unambiguous AND unblocked, yet displaced. A bug or a limit of the greedy scheduler, not a constraint. Cap is per-file at what git prints, because the raw charge carries the artifacts above.                                                                           |
| **ambiguity gate**       |                             1,240 | same-hash siblings the precision gate abstains on. Exactly what the merged family-permute pass solves for NAMES via masked usage context. The gate exists because naive pairing manufactured **+2.3%** churn on 118→119, so this needs an evidence gate, not a removal. |
| barrier-between residual |                             2,628 | shrinks as barriers are admitted; not separately addressable                                                                                                                                                                                                            |

## Tooling

Everything here reuses the scoring classifier's own `statementsOf` / `onLcs` /
`composeFile`, so a probe cannot drift from the KPI it explains.

| script                   | question                                                              |
| ------------------------ | --------------------------------------------------------------------- |
| `why-moved.ts`           | in ONE file, what is charged and why                                  |
| `reorder-census.ts`      | whole-pair charge by kind of displaced statement                      |
| `barrier-exact-fixed.ts` | the corrected ceiling, printing exp045's setting beside the emitter's |
| `ambiguity-split.ts`     | charge split by cause, plus the git-capped ceiling                    |
| `per-file-fidelity.ts`   | does the decomposition match git per file, or only in total           |
| `measure-registrar.sh`   | draw-pinned A/B of the lever                                          |
| `cold-ab.sh`             | the cold 4-pair gate                                                  |

**`why-moved.ts` carries a warning worth repeating: a comparison of INDEXES is
not a list of reorderings.** Inserting one statement shifts the index of every
statement after it, so on a growing release most statements sit at a different
index while remaining in the same relative order. The first cut of that probe
compared indexes, reported 17 "moves" and 784 lines in a file the KPI charges 184
for, and made alphabetised `Object.defineProperty` accessor blocks — which never
reorder — look like the dominant driver.
