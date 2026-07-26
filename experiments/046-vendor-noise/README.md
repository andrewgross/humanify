# 046 — Vendor noise: the 36,201 lines nobody has ever measured

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

**Read [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md)
first — all eight rules.** Rule 3 (a sizing predicate that does not test what its
name implies) bit twice while writing this brief; see "How this brief was
measured" at the bottom.

## Why this axis, now

exp041–045 took `src/` to its floor: ~4,070 reducible lines in 15,284 of measured
noise. All of that work scored **`src/` only** — `run.sh` line 121 passes
`("$OUT/src" "$PRIOR_SRC")` to the scorer and nothing else. The emitted tree also
contains `vendor/`, which a reviewer diffs like any other directory.

| tree      | changed lines, 4 hops | scored by the eval? |
| --------- | --------------------: | ------------------- |
| `src/`    |              ~156,816 | yes                 |
| `vendor/` |            **36,201** | **no — never**      |

**Vendor churn is 2.4× the entire measured `src` noise.** Every experiment in
this series has been fighting over a smaller number than the one nobody looked
at. Relocation, the axis that consumed exp041–043, started at 15,699.

### Measured, per hop

Commands are in "Reproducing the baseline" below; these are GNU `diff` line
counts (`grep -cE "^[<>]"`), not a real/noise decomposition.

| hop       | vendor changed ln | of which `_bun-modules.json` | files differing | measured `src` noise |
| --------- | ----------------: | ---------------------------: | --------------: | -------------------: |
| 85→86     |            11,540 |                        7,748 |           1,593 |                8,055 |
| 118→119   |             6,651 |                        3,004 |               — |                  780 |
| 197→198   |             9,169 |                        4,744 |               — |                3,011 |
| 215→216   |             8,841 |                        3,580 |           1,650 |                3,438 |
| **TOTAL** |        **36,201** |                   **19,076** |                 |           **15,284** |

**This table is a size, not a verdict.** It says how much territory is
unexamined. It does NOT say 36,201 lines are noise — some fraction is genuine
dependency updates, and nobody has decomposed it. Task A exists to fix that
before anything is built.

## Two sub-problems, very different in confidence

### 1. `_bun-modules.json` `factoryVar` — 19,076 ln, near-certain

53% of all vendor churn is one metadata file, and 91% of that file's diff is one
field. On 215→216: **3,265 of 3,580 changed lines contain `factoryVar`**. The
diff looks like this the whole way down:

```diff
-      "factoryVar": "_el",
+      "factoryVar": "mal",
```

Those are raw minifier tokens. The minifier reassigns them every release, so the
field churns whether or not any code changed.

The field's own docstring, `src/unpack/adapters/bun.ts:103`:

```ts
/** Original obfuscated factoryVar in the bundle (debug only). */
factoryVar: string;
```

Written at `src/unpack/adapters/bun.ts:205-213` (`manifestEntries.push({...})`),
serialized at line 235.

**Every consumer of the PERSISTED manifest was checked and none reads it:**

| consumer                                | reads `factoryVar`? |
| --------------------------------------- | ------------------- |
| `src/library-detection/adapters/bun.ts` | no                  |
| `src/commands/unified.ts`               | no                  |
| `src/split/bun-relink.ts`               | no                  |

Its live uses (`bun.ts:363`, `:420`, `:498`) all operate on the **in-memory**
record during unpacking, before serialization. `src/analysis/bun-module-classification.ts`
produces it; tests construct it in fixtures.

**Re-verify this yourself before deleting anything** — that is rule 1, and this
brief's own premise is a hypothesis (rule 8, and exp042's brief was wrong about
its central case).

Secondary candidates in the same file, unexamined: `runtimeIdentifier`,
`nameSource`, `structuralHash`, `fileName`, `name` account for ~45–47 changed
lines each on 215→216. `runtimeIdentifier` is the one to check — it may also be
minifier-derived.

### 2. Vendor library bodies — ~17,125 ln, a policy question first

~1,650 files differ per hop. These are the **1,647 library files humanify
deliberately skips** (see the "Skipping N library files" line in any run log), so
they stay minified and the minifier renames every local each release:

```diff
-exports.f = __commonJS(function(wPv,dGr){dGr.exports=function(e){function t(M,$,re,he){return new cT(M,$,re,he)}...
+exports.f = __commonJS(function(wyC,J7r){J7r.exports=function(e){function t(N,$,re,_e){return new VF(N,$,re,_e)}...
```

Each file is one or two very long lines, so a whole-file token reshuffle shows as
~2–4 changed lines × 1,650 files.

**Decide the policy before writing code.** The options, unranked until Task A:

- **C1 — inherit the prior body when the module is unchanged.** The machinery
  already exists: every factory carries a `structuralHash` in the manifest, and
  `loadPriorVendorNames` (`bun.ts:67`) already loads the prior manifest and keys
  by that hash to carry NAMES across releases. Extending it from names to bodies
  is a small addition at a proven seam.
  **The correctness question that gates this entirely:** does `structuralHash`
  cover string literals and numbers, or only structure? If two structurally
  identical modules can differ in a literal, reusing a prior body **ships wrong
  code**. Answer this before writing a line — precision over recall, and a
  vendored library is the last place to guess.
- **C2 — canonicalize minified locals deterministically** (stable ordinals from
  structure), so the same library minified twice yields the same text. More
  general than C1, more invasive, and must not change semantics.
- **C3 — keep vendor out of the review diff** while leaving it in the runnable
  tree. Cheapest, but it is a reporting change, not a noise fix, and the boot
  gate needs those files at runtime — you cannot simply not emit them.
- **C4 — name vendor files with the LLM.** 1,647 files; cost is the reason they
  are skipped today. Almost certainly not worth it, recorded so nobody
  re-derives it.

## The work, in order

### Task 0 — extend the eval to score `vendor/` (PREREQUISITE)

Without this there is **no gate**: the harness cannot see either win, and a
regression in `src/` traded for a vendor improvement would look like a pure win.

- `run.sh:118-121` builds `LAYOUT_ARGS` from `$OUT/src` and `$PRIOR_SRC`. Add
  vendor as its own scored surface — a separate column, NOT folded into the
  existing `src` numbers, so every committed reference stays comparable.
- Add the KPI to `experiments/034-eval-harness/kpis.ts` — one entry, with its
  direction. That file is the single definition; `analyze.ts`, `summarize.ts`
  and `leaderboard.ts` all read it.
- Older committed references have no vendor field and will print `-`. That is
  correct and must stay distinguishable from `0`; see the README section
  "Committed references are DATED".

### Task A — decompose vendor churn BEFORE building

Reproduce the table above, then answer the question it cannot:

1. **How much of 36,201 is real dependency change?** Decompose vendor the way
   `diff-composition.ts` decomposes `src`: per common file, statements that are
   byte-identical, identical-modulo-identifiers, or genuinely added/removed. A
   minified one-line file needs a different unit than a statement — decide it and
   say so.
2. **How many vendor files are unchanged modulo minified names?** That is C1's
   exact ceiling. If it is most of them, C1 is the lever; if libraries genuinely
   change every release, there is nothing to inherit.
3. **Confirm the `factoryVar` consumer audit** in the table above.
4. **Read actual vendor diffs before believing any of it** (rule 1). Two sizing
   predicates in exp044, and two in this brief, confirmed hypotheses that reading
   the same data refuted.

### Task B — drop `factoryVar` from the serialized manifest

Small, near-certain, and independent of Task C — land it as its own commit.

- Remove it at the write site (`bun.ts:205-213`); keep it on the in-memory type
  if the unpacker still wants it. Prefer deleting the field outright if nothing
  needs it — [no backwards compatibility](../../CLAUDE.md), delete old code and
  write fresh tests.
- A prior manifest that still HAS the field must keep parsing (extra JSON keys
  are ignored on read) — verify with a real prior tree, do not assume.
- TDD red-first, per CLAUDE.md.
- **This deliberately changes output**, so it is not a byte-identical refactor.
  `src/` must be byte-identical; `vendor/_bun-modules.json` must change by
  exactly the removed field and nothing else.

### Task C — the vendor body lever

Only after Task A gives it a ceiling. Behind a kill switch
(`HUMANIFY_NO_VENDOR_INHERIT=1` or equivalent), TDD red-first.

### Task D — gate and write up

`experiments/041-content-anchor/gate-verdict.sh exp043-nearident <label>`, plus
the new vendor KPI. Then `RESULTS.md`, totals-first, every claim with its number,
and a **STATUS block at the top of this README** saying what happened and which
of its claims did not survive (see `experiments/README.md`).

## Gate — all non-negotiable, judge EVERY hop on its own

1. **Vendor churn DOWN on every hop.** Baselines: 11,540 / 6,651 / 9,169 / 8,841.
2. **`src/` must not regress.** naming 7,616, reorder 6,078, relocation 1,390 —
   relocation especially, it took three experiments to get there.
3. **`novel` and `realLn` UNMOVED.** Reducing noise by dropping real change is a
   regression wearing a win's clothing. On the committed references these read
   `4188 (=)` / `416377 (=)` across every experiment.
4. **Boot gate green for all four.** `bun run.cjs --version`. Bun lives at
   `~/.bun/bin` and is **not on PATH** — without it `run.sh` prints
   "BOOT GATE SKIPPED" and says nothing else. **This matters more here than in
   any prior experiment: Task C rewrites the code that actually runs.**
5. **Self-hop byte-identical**, bundle AND split ledger.
6. **118→119 is the regression canary.** It has the least vendor churn (6,651)
   and the least `src` noise (780) — almost nothing to win, everything to lose.

## Reproducing the baseline

```bash
W=/work/exp043-nearident

# per-hop vendor churn and the manifest's share of it
for p in "85-rebased:86" "118-rebased:119" "197-rebased:198" "215-rebased:216"; do
  a=${p%%:*}; b=${p##*:}
  v=$(diff -rN "$W/2.1.$a/vendor" "$W/2.1.$b/vendor" | grep -cE "^[<>]")
  j=$(diff "$W/2.1.$a/vendor/_bun-modules.json" "$W/2.1.$b/vendor/_bun-modules.json" | grep -cE "^[<>]")
  echo "${a%%-*}->$b: vendor $v (manifest $j)"
done

# what kind of manifest lines change
diff $W/2.1.215-rebased/vendor/_bun-modules.json $W/2.1.216/vendor/_bun-modules.json \
  | grep -E "^[<>]" | grep -c factoryVar

# browse any of it
git diff --no-index $W/2.1.215-rebased/vendor $W/2.1.216/vendor | less -R
```

Exact counts differ between GNU `diff` and `git diff` (they pick different edit
scripts — the manifest reads 7,748 under one and 11,078 under the other on
85→86). Compare like with like; the ratios hold either way.

## `vendor-ref-churn.ts` — the app-code side, already measured

Included here because it belongs to this axis, though it is **not** part of Tasks
B or C. It answers a different question: how much of the **`src/`** diff changes
only because a name that reaches INTO vendor moved?

```
npx tsx vendor-ref-churn.ts <priorSrcDir> <freshSrcDir> <label>
```

| hop       | src changed ln | changed ONLY by a vendor-reference name |
| --------- | -------------: | --------------------------------------: |
| 85→86     |         44,824 |                                     660 |
| 118→119   |         36,767 |                                      22 |
| 197→198   |         45,231 |                                     544 |
| 215→216   |         29,994 |                                      62 |
| **TOTAL** |    **156,816** |                               **1,288** |

~8% of measured `src` noise. It is a slice of the `naming` bucket, **not**
additive with it. Two families are in scope: emitter path aliases
(`const lib_eb5345cb_2 = require(".../vendor/noop/lib_eb5345cb.js")`) and renamer
handles derived from them (`React93 = importDefault(lib_eb5345cb_2.f(), 1)` —
~120 `React*` bindings collide on the stem and get slot ordinals).

## How this brief was measured — and the two errors it survived

Recorded because both are rule 3, and both were caught only by checking against a
case whose answer was already known by hand.

1. **`vendor-ref-churn.ts` first reported 826 lines (0.5%).** It paired diff lines
   positionally within each `-U0` hunk and skipped any hunk whose `-` and `+`
   counts differed — discarding **35,490 of 44,824 changed lines, 79% of the
   diff**, having compared only 4,667 pairs. Matching by masked content across
   the whole **file** (with `-U0`, a renamed identifier makes one tiny hunk per
   use site, so a line's partner is almost never in its own hunk) took it from
   316 matches to 9,166 and the answer to 1,288. The surviving caveat: whole-file
   content matching can pair two unrelated lines that mask alike, which pushes the
   number UP, so 1,288 is a floor with an unmeasured ceiling.
2. **`React123 → React93` was presented as the exemplar of the 85→86 mass.** It
   is the top of the drift list by statement size and **not** the mass: numbered
   `React*` bindings touch 1,403 of 42,732 changed lines (3.3%, itself an upper
   bound), and exp044 puts the whole counter class at 313 of 5,760 naming lines.
   That is rule 2, and exp044 had already flagged this exact trap.

## Warnings carried forward

- **Never trust a match you have not eyeballed.** It has refuted seven
  hypotheses in this series, including two from a brief's own premise.
- **A ceiling scoped to the directly-affected population under-predicts.**
  exp044's alias reservation destabilised nothing it measured and still cost
  +3,742 lines through second-order effects.
- **Name-keyed metrics cannot see content identity.** The leaderboard's `reloc`
  column rose on all three experiments that cut relocation 91%. Use
  `experiments/040-diff-census/relocation-churn.ts`.
- **Never edit `src/` while a pipeline is in flight.** `pgrep -f` matches your own
  command line — use `ps -eo pid,cmd | grep -v grep` or you will get a false
  "still running".
