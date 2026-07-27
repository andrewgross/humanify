# 047 — The vendor residual: manifest order, and whether filenames rotate

> ## STATUS — this is the BRIEF. Read [`RESULTS.md`](./RESULTS.md) for what happened.
>
> **Outcome: Task 2 SHIPPED, Task 1 answered, Task 3 CLOSED as not worth
> building.** Gated **COLD** — 16 pipeline runs, no LLM cache, live model traffic.
> Manifest churn **6,393 → 1,563 (−75.6%)**, down on every hop including the
> 118→119 canary; `vendorLn` 6,556 → 1,744. The manifest is now written in the
> PRIOR RELEASE's order rather than bundle order, with the naming tie-break moved
> out of array position into a `hashOrdinal` field.
>
> **This experiment's first gate run was accidentally cache-pinned** — every
> prompt was a replayed answer and not one reached the model. Its `src/` numbers
> were not gate-valid. `run.sh` now defaults to NO cache. Three things only the
> cold run could show: `vendorReal` is **not draw-stable** (±200 for identical
> code, because it counts humanify's own filename rotation as dependency change);
> the **self-hop invariant is unreachable cold even for the control** (78 lines,
> candidate 34, all `nameSource` labels); and the committed `src/` baselines are
> themselves cache-pinned artifacts.
>
> **Claims of this brief that did NOT survive:**
>
> - **Task 2's central proposal, built as written, is WORSE than doing nothing.**
>   The brief said to add the bundle index as a field and sort by
>   `(structuralHash, index)`. Measured: `bundleIndex` on every entry comes to
>   **7,056 manifest lines against a 6,407 baseline**, because a bundle index
>   records the very churn it is meant to make recoverable — the `factoryVar`
>   pathology exp046 deleted. And every content-derived SORT key regresses
>   197→198 (+128 to +496) and the canary (+8), because sorting relocates an
>   entry whose key changed, converting an in-place edit into a delete plus an
>   add. What shipped is prior-release ORDER (not a sort) plus `hashOrdinal`
>   (not a bundle index), which costs zero.
> - **"If the answer is a handful, the risk is small."** It is not a handful.
>   129–145 same-hash groups per hop have members that disagree about `name`.
>   exp046's "do not sort the manifest" caution was correct on its merits; what
>   was a hypothesis is that the constraint forces bundle order. It forces
>   _recoverability_, which is weaker and satisfiable.
> - **Task 3's premise — "worth doing only if task 1 says rotation is happening
>   at scale" — is satisfied and the task is still not worth doing.** Rotation is
>   real and at scale by file count (119 of 133 removed files declare the same
>   library identity as an added one; `highlight.js-php.js` holds the SQL
>   grammar, `ruby.js` holds Scilab, `fsharp.js` holds the Python console). But
>   stabilising the filename recovers **138 lines, not ~1,285**, and rotation
>   occurs on ONE hop of four (removed files: 0 / 2 / 3 / 133) because a
>   `lib_<structuralHash8>` name can only rotate when content changed. Rotation
>   is a shadow cast by real change, not an independent noise source.
> - **The 3,225-vs-3,364 unit caveat dissolves.** Measured on 197→198: 133
>   removed files total 271 lines, 133 added total 784 — 1,055 against a
>   bodies+files column of 1,157, the ~102 difference being in-place body edits.
>   No double count.
> - **"No third unscored surface exists" is WRONG.** `index.js` sits at the TREE
>   ROOT, is handed to no scorer (`run.sh` passes only `$OUT/src` and
>   `$OUT/vendor`), and churns **2,067 lines — 85→86 rewrites 85.5% of its 1,533
>   lines** as pure `require()` reordering. The brief enumerated it but filed it
>   under "small, task 3". Do NOT apply the manifest fix to it: its order is
>   SEMANTIC (module init order, which the boot gate depends on), unlike the inert
>   manifest.
> - **Task 1's suspicion was right about rotation and wrong about the
>   consequence.** 113 of the 119 renamed pairs are a DIFFERENT program: 197→198
>   carries a genuine highlight.js 10.x → 11.11.1 bump (`"11.11.1"` present only
>   in 198; `scope:` 23 → 254; the v11 release notes' removal of the
>   `php3…php8` aliases matches the unmatched-removed list file-for-file).
>   exp046 reached the right classification by the wrong reasoning.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

**Read [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md)
first — all nine rules.** This brief is a HYPOTHESIS, including its cautions.
[046](../046-vendor-noise/)'s brief was wrong about its central proposal (it
would have shipped wrong code), and three predicates written during 046
confirmed hypotheses that reading the same data refuted. Assume the same rate
here.

## Where 046 left the tree

[046](../046-vendor-noise/) made `vendor/` a scored surface for the first time
and took it from 36,201 to 9,632 changed lines (−73.4%) across the four gate
hops, with `src/` byte-identical and `vendorReal` unmoved. What is left:

| hop        |  residual |  manifest | bodies + files |
| ---------- | --------: | --------: | -------------: |
| 85→86      |     4,656 |     4,574 |             82 |
| 118→119 🐤 |       337 |        20 |            317 |
| 197→198    |     2,655 |     1,498 |          1,157 |
| 215→216    |     1,984 |       315 |          1,669 |
| **TOTAL**  | **9,632** | **6,407** |      **3,225** |

The `vendorReal` KPI reads **3,364** — genuine dependency change, which must not
move. The bodies/files column is essentially all of it. **So the reducible
remainder is the manifest's 6,407 lines**, and it is wildly uneven: 85→86 alone
carries 71% of it.

Body churn is done. `vendor-body-inherit.ts` inherits ~1,575 of 1,592 common
files per hop, and re-running `predict-inherit.ts` on the post-046 trees returns
`changed 17, inherit 0` — there is nothing left to inherit.

### A negative result, so nobody re-runs it

046's real lesson was "check what the harness actually measures." That was
checked to exhaustion here: **no third unscored surface exists.**

| surface                                            | churn, 4 hops | reviewer-facing?    |
| -------------------------------------------------- | ------------: | ------------------- |
| `.humanify/` (humanified.js 805k ln, ledgers, map) |        ~1.49M | **no** — metadata   |
| `package.json`, `run.cjs`, `RUNNABLE.md`           |         **0** | yes, but static     |
| `index.js` (require order, 1,502 ln)               |         2,067 | yes — small, task 3 |

The 1.49M is a SIZE, not a finding — `.humanify/humanified.js` is the whole
bundle as one file, kept as metadata and diffed by nobody. Recording it so the
next reader does not rediscover it and mistake it for a lever.

## Task 1 — is 197→198's 127 added / 127 removed real? (DO THIS FIRST)

**046 published this as real dependency change without proving it.** It is the
one number in that experiment's ledger flagged uncertain, and it is worth ~1,285
of the 3,364 `vendorReal` lines.

The suspicion, and why it is not idle: on the SAME hop, 046 read the actual
files and found `kotlin.js` in 198 contains **WebAssembly** keywords while
`elixir.js` holds different grammar on each side. humanify's vendor filenames
demonstrably rotate between releases. The added/removed symmetry is exactly
127/127, which is what a permutation looks like and not what a dependency bump
usually looks like.

Both readings are live:

- **Rotation** → `vendorReal` is overstated by up to 1,285 lines, the 046 ledger
  needs a correction, and task 2's ranking changes.
- **A genuine highlight.js version bump** → every grammar legitimately changed,
  `vendorReal` is right, and the vendor axis is closed apart from the manifest.

**Method — read, do not classify.** 046 lost two predicates to rule 3 on this
exact data. Open the 127 removed and 127 added files and answer one question per
file: does its content appear on the other side under any name? The content
signature (`vendor-churn.ts`'s masked variant) matched only 6 as pure moves, so
if these ARE rotations they are rotations WITH edits — which means the signature
is the wrong instrument and a near-identity comparison (exp043's ≤10%-of-lines
twin test) is the right one. Check a handful of grammars against the real
highlight.js release notes for 198's version before believing either answer.

**No build until this is answered.** It changes what tasks 2 and 3 are worth.

## Task 2 — manifest entry-block reordering (~6,407 ln, the top lever)

`_bun-modules.json` is written in bundle order. When Bun reorders factories, the
whole `{...}` entry moves and `diff` charges every line of it — this is why
85→86's manifest churn (4,574) dwarfs 215→216's (315) even though `factoryVar`
is gone from both.

**046 said "do not sort the manifest," and that caution is a hypothesis too.**
The reason it gave is real: `loadPriorVendorNames` documents position as its
tie-break for same-`structuralHash` groups, because re-export shims are
structurally identical but proxy different libraries, and collapsing them would
misname every member. Sorting the file naively destroys that.

But the constraint is "position must be recoverable," not "the file must be in
bundle order." **Make the tie-break explicit data and the file can be stably
ordered without losing anything:** add the bundle index as a field, sort entries
by a stable key (`structuralHash`, then index), and have `loadPriorVendorNames`
read the field instead of inferring order from the array.

Before building:

1. **Measure the ceiling.** How much of the 6,407 is entry MOVEMENT versus entry
   content changing? Reuse `vendor-churn.ts`'s field attribution — 046 already
   reports `byField`, and `(structural)` vs named-field lines separate these.
2. **Verify the tie-break claim by reading it.** How many same-hash groups with
   ≥2 members actually exist per hop? If the answer is "a handful," the risk is
   small and testable; if it is "hundreds," the sort key needs more care.
3. **This is a manifest format change**, so a prior manifest without the new
   field must still parse — verify with a real prior tree, as 046 did for
   `factoryVar`, do not assume.

Caution, and it is the one that killed exp044: **vendor names feed `src/`
require paths.** A change that perturbs which name a factory gets has a blast
radius well beyond the manifest. This task must change ORDER ONLY, never a
name — prove that with a byte-identical `src/` before reading any KPI.

## Task 3 — vendor filename rotation (gated by task 1)

Two known symptoms, same suspected cause:

- 16 files on 85→86 whose only change is a require path pointing at a vendor
  file humanify renamed (`S="../lodash/lib_eb5345cb.js" → "…-2.js"`).
- `index.js`, 2,067 lines across four hops, is a list of `require()` lines in
  load order and inherits both renames and reordering.

This is the same rotation class exp041–043 solved for `src/` — a name is not an
identity — and those experiments' machinery (content anchor, corroborated-content
preempt) is the obvious thing to point at it. **Worth doing only if task 1 says
rotation is happening at scale**; on the evidence so far it is ~16 files plus
whatever task 1 finds.

## Gate — unchanged, all non-negotiable, every hop on its own

`experiments/041-content-anchor/gate-verdict.sh exp046-bodyinherit <label>`

1. **Vendor churn DOWN per hop.** Baselines: **4,656 / 337 / 2,655 / 1,984**.
2. **`src/` must not regress.** Every KPI is currently byte-identical to
   `exp043-nearident`: `noise` 3125, `noiseLn` 61878, `newName` 4307, `mints` 85,
   `reorderLn` 6078, `relocSt` 0. Relocation via `relocation-churn.ts`, NOT the
   name-keyed `reloc` column: 349 / 16 / 343 / 682.
3. **`novel` 4188, `realLn` 416377 and `vendorReal` 3,364 UNMOVED** — unless
   task 1 justifies correcting `vendorReal`, in which case say so explicitly and
   re-baseline rather than letting it drift.
4. **Boot gate green all four.** Bun is at `~/.bun/bin` and NOT on PATH; without
   it `run.sh` silently prints "BOOT GATE SKIPPED".
5. **Self-hop byte-identical**, bundle AND split ledger.
6. **118→119 is the canary.** Its vendor residual is 337 — there is essentially
   nothing to win and everything to lose.

## Reproducing the baseline

```bash
W=/work/exp046-bodyinherit

# residual, split into manifest vs bodies
for p in "85-rebased:86" "118-rebased:119" "197-rebased:198" "215-rebased:216"; do
  a=${p%%:*}; b=${p##*:}
  t=$(diff -rN "$W/2.1.$a/vendor" "$W/2.1.$b/vendor" | grep -cE "^[<>]")
  m=$(diff "$W/2.1.$a/vendor/_bun-modules.json" "$W/2.1.$b/vendor/_bun-modules.json" | grep -cE "^[<>]")
  echo "${a%%-*}->$b: total $t, manifest $m"
done

# per-field manifest attribution, and the full decomposition
npx tsx experiments/046-vendor-noise/vendor-churn-decompose.ts \
  "$W/2.1.215-rebased/vendor" "$W/2.1.216/vendor" 215->216

# does the body lever still have anything to inherit? (expect: no)
npx tsx experiments/046-vendor-noise/predict-inherit.ts \
  "$W/2.1.215-rebased" "$W/2.1.216" 215->216
```

## Warnings carried forward

- **Never trust a match you have not eyeballed.** It has now refuted ten
  hypotheses in this series, three of them during 046 alone.
- **A sizing predicate that "looks right" fails silently.** State in one sentence
  what your predicate actually tests, and check that sentence is the claim you
  are making.
- **`structuralHash` is NOT literal-preserving** — it keeps a string's LENGTH and
  a number's magnitude bucket only, so six of twelve semantic differences are
  invisible to it. Never gate correctness on it; use
  `computeStructuralSignature`. Probe literal coverage with SAME-LENGTH pairs, or
  the probe reassures you for the wrong reason (046 §A2).
- **A ceiling scoped to the directly-affected population under-predicts.**
  exp044's alias reservation destabilised nothing it measured and still cost
  +3,742 lines through second-order effects.
- **Never edit `src/` while a pipeline is in flight.** `pgrep -f` matches your own
  command line — use `ps -eo pid,cmd | grep -v grep`.
