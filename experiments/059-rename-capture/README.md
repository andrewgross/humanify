# 059 — The rename capture: `a !== b` is emitted as `b !== b`

> ## STATUS: **CHARACTERISED, NOT FIXED. Read [`RESULTS.md`](./RESULTS.md) first — it supersedes three of this brief's framings.**
>
> The two captured bindings, their source lines, and the strategy that named
> them are all identified. Both renames came from the **LLM path**, not the
> prior-version transfer this brief guessed at. The remaining question is narrow
> and stated in RESULTS: why the second rename's guard does not observe the
> first.
>
> ---
>
> ### Original brief follows
>
> This is not a lever, a sizing exercise, or a noise-reduction idea. It is a
> **correctness bug that emits wrong code**, caught by the pure-rename invariant
> on 8 of 40 committed `2.1.197→2.1.198` runs. Everything below the "Evidence"
> heading is measured, not believed.
>
> - **What it does:** a rename collapses two distinct bindings into one, so an
>   expression comparing two different values becomes one comparing a value to
>   itself. `a !== b` → `b !== b`, which is **always false**.
> - **The boot gate PASSES on the broken tree.** It starts, reports its version,
>   and answers a live prompt while computing the wrong answer. No runtime check
>   can see this class of bug — do not treat "it boots" as "it is correct".
> - **The rename guard is NOT trivially broken.** Twelve adversarial shapes were
>   probed and `getRenameRejection` rejected every one. A single rename cannot
>   produce this. Do not "fix" the guard without a reproduction.
> - **This brief names a prime suspect and explicitly forbids acting on it
>   unverified.** See "The suspect, and why you must not trust it".
>
> **This experiment may correctly produce NO CODE.** 051, 052, 053 and 057 each
> did. If the answer is "the invariant is right and the rename is right and
> something upstream mutated the tree", that is a finding — write it in
> `RESULTS.md` and stop.
>
> Whoever finishes this stamps a STATUS block here naming which of the brief's
> claims did not survive.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).

**Read first, in order. Do not skip 1 or 2.**

1. [`CLAUDE.md`](../../CLAUDE.md) — the ONE gate command, the eval, and
   `neutrality.sh`. Rules 3, 10 and 11 of
   [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md) all bite here.
2. [`docs/responsibility.md`](../../docs/responsibility.md) — who owns "may this
   rename happen" (`getRenameRejection`) and "apply a rename"
   (`attemptValidatedRename`). **Never call `scope.rename` directly in transfer
   code**; there is exactly one documented exception.
3. [`src/output-validation.ts`](../../src/output-validation.ts) — the two
   invariant checks and how they differ. Knowing WHICH one fires is half the
   diagnosis.
4. [`docs/pipeline-stages.md`](../../docs/pipeline-stages.md) — the twelve stages.
   The failure is in stage 9 (naming), on a file produced by stage 3 (unpack).

---

## The bug

From the cold `baseline-2026-08-03` run, `2.1.197→2.1.198`, file `runtime.js`:

```
ERROR: .../2.1.198/runtime.js: Rename changed how identifiers resolve
(structural signature mismatch on re-parse): a renamed binding captures
references of another binding, or structure changed beyond binding names.
  first divergence at token 308757 of 16384801 tokens each
    original: "$4431"
    output:   "$4434"
    original context: id: $4435 ; init: BinaryExpression{ left: $4431 ; operator: "!==" ; right: $4434
    output context:   id: $4435 ; init: BinaryExpression{ left: $4434 ; operator: "!==" ; right: $4434
```

`$NNNN` are placeholder slots assigned in walk order by the structural
serializer — two different numbers mean two different **bindings**. The input
compares binding 4431 with binding 4434. The output compares 4434 with itself.

That is the FIRST disjunct of the message ("captures references of another
binding"). It is a real capture, not a serialization artifact.

### Why this is different from the 2.1.86 failure that was already fixed

`2.1.86` tripped the same _message_ for the SECOND disjunct — class-private
names serialized verbatim, so a legitimate `#f`→`#A` rename read as structural
change. That was a **false positive** and is fixed
(`computeRenameInvariantSignature`, per-class private slots).

**Do not assume the two are related.** They trip different checks:

|           | 2.1.86 (fixed)                             | 2.1.198 (this bug)                          |
| --------- | ------------------------------------------ | ------------------------------------------- |
| check     | `checkStructuralInvariant`, pre-generation | `checkResolvedSignature`, cold **re-parse** |
| cause     | private-name serialization                 | a genuine capture                           |
| verdict   | false positive                             | **wrong emitted code**                      |
| frequency | 37 of ~37 runs, deterministic              | 8 of 40 runs, draw-dependent                |

I conflated these twice while investigating. The pre-generation check resolves
identifiers through binding caches captured _before_ any rename, so a capture is
structurally invisible to it — only the cold re-parse resolves names the way a
runtime would. That asymmetry is documented at `output-validation.ts` and is the
reason this bug survived so long.

---

## Evidence (all measured)

### Frequency: 8 of 40 committed `2.1.198` runs

```
baseline-2026-08-03 (030be60)   exp050-cold-control (c0e41fb)
baseline-main       (1d5d305)   lever1-twin-v3      (27925bb)
exp044-alias                    lever1-twin-v4      (9829a82)
exp048-cold         (eb071da)   post-consolidation  (3a7d092)
```

Reproduce that list yourself — do not trust it:

```bash
cd experiments/034-eval-harness/results
for d in */; do f="$d/2.1.198.stdout"; [ -f "$f" ] || continue; \
  c=$(grep -c '^ERROR:' "$f"); [ "$c" != "0" ] && echo "${d%/}"; done
```

**~20% per run.** It does not reproduce warm (2 attempts, both clean, with the
LLM cache on and the archive prior). It is therefore **draw-dependent**: the
model proposes a name that collides only on some runs.

### The guard is not trivially broken

Twelve adversarial shapes were run through `attemptValidatedRename` and then
through the cold re-parse check. `getRenameRejection` rejected **all twelve**:

| shape                                             | rejected by        |
| ------------------------------------------------- | ------------------ |
| target bound in a sibling/child function          | `shadows-child`    |
| target bound in a nested block                    | `shadows-child`    |
| target is an arrow parameter                      | `shadows-child`    |
| target is a `catch` parameter                     | `shadows-child`    |
| target bound two scopes deep                      | `shadows-child`    |
| target is a free global referenced inside         | `target-free-name` |
| target already in the renaming scope (6 variants) | `target-in-scope`  |

So **a single rename in a clean scope cannot produce this.** It requires a
sequence of renames, or scope state that has gone stale between them.

> **Trap I fell into, do not repeat it:** my first six probes declared both names
> in the _same_ scope, so every one was rejected by `target-in-scope` without
> ever exercising the interesting guards. If every case rejects for the same
> reason, your fixtures are wrong, not the guard.

### The boot gate passes on the broken tree

`baseline-2026-08-03/2.1.198-boot.json` is `{"ok": true}` — version **and** live
prompt. This is the argument for the structural invariant existing at all.

---

## The suspect, and why you must not trust it

`fastRenameBinding` (`src/rename/validated-rename.ts:160`) bypasses Babel's
`scope.rename()` for a ~340,000× speedup (1.7s → 0.005ms on a module scope,
because `scope.rename` re-traverses the entire bundle). It rewrites the
declaration identifier, `binding.referencePaths`, and constant violations, then
patches **this scope's** bindings map:

```ts
scope.bindings[newName] = binding;
delete scope.bindings[oldName];
```

Babel's real renamer re-traverses and repairs all scope bookkeeping. This does
not. Child scopes' state and other bindings' `referencePaths` are never
refreshed, so a later rename consulting stale information could pass its guard
and still capture.

**That is a hypothesis. It is UNVERIFIED.** It is consistent with every fact
above — draw-dependence, sequence-dependence, invisibility to the pre-generation
check — and consistent is not the same as true. `fastRenameBinding` exists for a
real reason; naively replacing it with `scope.rename()` would make a module-scope
rename ~340,000× slower and is not an acceptable fix.

**Do not change `validated-rename.ts` until Task 2 produces a reproduction.**

---

## Tasks, in order

### Task 0 — confirm the bug still exists (do not skip)

Everything below assumes current `main` still emits it. Confirm before building.

```bash
git log --oneline -1                 # record the commit you are testing
npm run check                        # all 6 stages must pass before you start
```

### Task 1 — reproduce it and CAPTURE THE EVIDENCE

The rejected file and its pre-rename source are now preserved under
`<out>/.humanify/failed/` (`src/failed-output.ts`). **Before this existed the
split deleted the file and the investigation dead-ended — that is why this
experiment is a brief and not a fix.**

Run the pair cold, repeatedly, until it fires. At ~20% per run:
10 runs ≈ 89% chance of at least one hit, 14 runs ≈ 96%. Each run is ~16 min,
so budget ~3 hours.

```bash
experiments/059-rename-capture/repro.sh 14      # see the script; writes to /work
```

Stop as soon as one fires. You need exactly one.

### Task 2 — read the two preserved files side by side

```bash
D=/work/exp059/hit-<N>/.humanify/failed
ls $D    # runtime.js.original  runtime.js.validated  runtime.js
```

**Diff `.original` against `.validated`, NOT against `runtime.js`.**

`runtime.js` is the file that reached disk, and it is a DIFFERENT ARTIFACT from
the code the invariant examined: reconcile, the deferred sweep and the family
permutation all run after validation and replace the output. Measured on the
first reproduction — the checked code had 16,384,801 tokens (matching the
diagnostic) and the written file had 16,120,630. Diffing the file reports a
divergence at token 145, a variable-declaration merge with nothing to do with
the failure, and sends you in the wrong direction. I did exactly that.

A quick self-check before you trust any diff: the token count of `.validated`
must equal the "of N tokens each" figure in the diagnostic.

Find the divergence. **Both sides are required** — see the trap below.

```bash
# every self-comparison in the OUTPUT, with its line
grep -nE "([A-Za-z_$][A-Za-z0-9_$]*) !== \1\b" $D/runtime.js
# then look at the SAME construct in the original
```

> **Trap, and it cost me three false leads:** `x !== x` is the standard NaN
> idiom and appears all over real bundles (`value !== value`, `key !== key`,
> lodash's `eq` is literally `valueA !== valueA && valueB !== valueB`). A
> self-comparison in the output is **not** evidence on its own. The bug is a
> self-comparison in the output whose ORIGINAL had two _different_ identifiers.

Name the two bindings. Then answer: which rename gave one of them the other's
name, and why did `getRenameRejection` allow it?

`--diagnostics` records a per-identifier strategy trail
(`strategyTrail`, `src/rename/strategy-trail.ts`) naming every tier's attempt
for each binding. Run the reproduction with `--diagnostics <path>` and search it
for the two names.

### Task 3 — write a FAILING unit test before touching any source

Reproduce the capture in `src/rename/validated-rename.test.ts` as a minimal
fixture. The test must fail on current `main` for the right reason. **Verify the
red step**: if it passes before you change anything, it is not testing the bug.

### Task 4 — fix, then prove it

The fix changes the rename core, so it is **not** output-neutral and
`neutrality.sh` is the wrong instrument for the fix itself.

```bash
npm run check                                    # all 6 stages, non-negotiable
experiments/034-eval-harness/run.sh exp059-fix   # cold, 4 pairs, ~1h
```

Then re-run Task 1's loop. **A single clean run proves nothing** — the bug only
appears ~20% of the time. You need enough clean cold runs that the absence is
meaningful: 14 clean runs put the upper bound on the failure rate at roughly 20%
→ under 5% (0.8^14 ≈ 0.04). State the number of runs with the claim.

Compare against `baseline-2026-08-03` (committed, cold, rebased priors):

```bash
npx tsx experiments/034-eval-harness/leaderboard.ts baseline-2026-08-03 exp059-fix
```

Confirm `realLn` and `novel` did **not** move — a "fix" that drops real code
change is a regression (rule 5).

---

## What "done" looks like

- [ ] the two captured bindings are NAMED, with the rename that collapsed them
- [ ] a failing unit test exists that reproduces it minimally, verified red
- [ ] the fix passes `npm run check`, all 6 stages
- [ ] a cold 4-pair eval shows `realLn`/`novel` unmoved vs `baseline-2026-08-03`
- [ ] ≥14 clean cold `197→198` runs, and the count is stated with the claim
- [ ] `RESULTS.md` written, including which of this brief's claims were wrong

**If the fix is not found**, that is still a result. Write down what the two
bindings were and why the guard allowed it, even without a fix. The next person
starts from a named mechanism instead of a token offset.

---

## Things that will waste your time

1. **Reading the output file alone.** A capture is invisible from one side.
2. **Trusting a single clean run.** 20% per run; four clean runs in a row is a
   38% event even with the bug fully present.
3. **Warm runs.** It has never reproduced with the LLM cache on. Cold, always —
   which is also what rule 10 requires for a verdict.
4. **Assuming this is the 2.1.86 private-name bug.** Different check, different
   cause, already fixed. I made this mistake twice.
5. **"Fixing" `getRenameRejection` by adding a rule.** Twelve shapes already
   reject correctly. A new rule that rejects MORE renames costs names —
   exp044 refused names on principle and cost **+3,742 lines**.
6. **Editing `src/` while an eval is running.** `run.sh` executes from the
   working tree; the baseline leg of `neutrality.sh` uses a detached worktree.
7. **Believing `noiseLn` says "no effect".** It scores the BUNDLE, not the split
   tree. Read `layout.churnLines`.
