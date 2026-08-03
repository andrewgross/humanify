# 059 — RESULTS: the capture is REAL, fully characterised, and NOT YET FIXED

> ## STATUS: **root cause characterised down to the two bindings and the strategy that named them. The remaining question is narrow and named. NO FIX SHIPPED.**
>
> Read this before the [brief](./README.md) — the brief is what was believed
> beforehand, and two of its framings were wrong.

## What the bug actually is

`runtime.js`, in the Bun-unpacked tree for `2.1.197→2.1.198`:

```js
function getFileWriter() {
  if (!logFileWriter) {
    let dirPath = null;                                    // OUTER  (was `e`)
    logFileWriter = createBufferedWriter({
      writeFn: (task) => {
        let debugFilePath = getDebugFilePathVal();
        let dirPath = pathModuleVal.dirname(debugFilePath); // INNER  (was `r`)
        let isDirChanged = dirPath !== dirPath;             // ALWAYS FALSE
        dirPath = dirPath;                                  // NO-OP
```

Two bindings in **nested scopes** were both renamed to `dirPath`. The inner
shadows the outer, so the "has the directory changed since last time?" check is
permanently `false` and the remembered value never updates.

The original is unambiguous:

```
13668:  let e = null;              <- outer, if-block scope
13677:  let r = jae.dirname(n);    <- inner, arrow-function scope
13678:  let o = e !== r;
13679:  e = r;
```

## Who did it

From the `--diagnostics` strategy trail of the reproduction:

```
oldName=e   loc=13668:10   via=['llm']
oldName=r   loc=13677:14   via=['llm']
```

**Both renames came from the LLM naming path, in the same run.** 99 bindings in
this file received the name `dirPath` (mostly via `exact-match` prior transfer);
that is normal and harmless for bindings in unrelated scopes. Exactly one pair
is nested, and that pair is the bug.

## The narrow remaining question

`getRenameRejection` should reject whichever of the two is applied second:

- outer→`dirPath` after inner→`dirPath` must trip `shadows-child`
- inner→`dirPath` after outer→`dirPath` must trip `target-visible`

**Both orderings were tested against the real guard on this exact code shape and
both were correctly rejected.** So the guard logic is right; something about how
the LLM path applies a batch of renames means the second application does not
see the first.

That is the whole remaining question: **why does the second rename's guard not
observe the first rename?**

### The sharpest hypothesis: BOTH guards fail open on empty `referencePaths`

Both guards depend on the same thing, and neither has a fallback:

```ts
// wouldRenameShadowInChildScope
const allPaths = [...binding.referencePaths, ...binding.constantViolations];
for (const refPath of allPaths) { ... }      // empty  =>  loop body never runs
return false;                                //        =>  NO REJECTION

// wouldCaptureOuterReference
return outer.referencePaths.some(inside)     // empty  =>  false
    || outer.constantViolations.some(inside);//        =>  NO REJECTION
```

If the OUTER binding's `referencePaths` is incomplete or empty at rename time,
**both guards pass in either order**, and no race is required. That fits every
observation:

- both renames applied despite the guard being logically correct
- a clean fixture cannot reproduce it (a fresh crawl has complete references)
- draw-dependence is fully explained by "the model has to propose the same name
  twice"; the guard failure is then deterministic, which matches the
  bit-identical token position across independent runs

`applyLlmRename` DOES route through `attemptValidatedRename` (verified), and the
Babel cache is cleared BEFORE naming, not during — so neither "the LLM path
skips the guard" nor "the cache was reset mid-pass" explains it.

### REFUTED 2026-08-03 — the guards had complete data and passed anyway

The experiment below was built (`refCount` on every recorded attempt, captured
BEFORE the rename so it is the number the GUARDS saw) and run. It fired on run 1:

```
loc=13668:10  oldName=e   {"strategy":"llm","outcome":"applied","newName":"dirPath","refCount":2}
loc=13677:14  oldName=r   {"strategy":"llm","outcome":"applied","newName":"dirPath","refCount":3}
```

The outer binding had **2 references** — the read at 13678 and the write at
13679, exactly what the source shows. Nothing was empty. **The fail-open
hypothesis is dead**: both guards had complete reference data and still applied.

That also rules out, for the same reason, anything that works by starving the
guards of references — a stale crawl, a binding registered without references,
a reference list cleared by an earlier pass.

WHAT SURVIVES: the guards ran with correct inputs and returned "no rejection".
On a consistent scope tree that is impossible — both orderings reject when
tested directly. So the two renames were evaluated against scope state in which
the other rename was not visible, WITHOUT the reference lists being wrong.

NEXT SUSPECT, and it is now the only one left standing: **scope-object
identity**. `wouldRenameShadowInChildScope` walks `refPath.scope` upward and
tests `refScope.bindings[newName]`; `wouldCaptureOuterReference` calls
`scope.parent?.getBinding(newName)`. Both read `bindings` maps off scope
OBJECTS. `fastRenameBinding` patches exactly one such map — the one belonging
to the scope it was handed. If the scope object handed to the second rename is
a DIFFERENT object for the same lexical scope, its map still holds the old name
and both guards see a tree in which the first rename never happened.

The experiment: log the scope's `block.start`/`block.end` and a per-object
identity marker alongside each applied rename, then check whether the arrow's
scope object seen when renaming `r` is the same object walked to when renaming
`e`. Same block positions with different object identity is the answer.

### The one-line experiment that settled the previous hypothesis

Log, for each applied LLM rename, `binding.referencePaths.length` alongside the
scope block's `start`/`end`. Then reproduce and look at the two renames at
`13668:10` and `13677:14`. If the outer binding shows **0 references** while the
source plainly references it twice inside the arrow, the hypothesis is
confirmed and the fix is about keeping `referencePaths` accurate — NOT about
adding a guard rule.

If instead both show correct reference counts, the hypothesis is dead and the
next suspect is scope-object identity: whether `binding.scope` handed to
`applyLlmRename` is the same object the other rename mutated.

**Do not add a rejection rule to `getRenameRejection`.** Twelve shapes already
reject correctly; a rule that rejects MORE costs names, and exp044 spent +3,742
lines learning that.

## Reproduction

Deterministic enough to work with: **fired on run 2 of 14, twice in a row**, and
the token position and both slot numbers were **identical** across independent
runs (`308757`, `$4431`→`$4434`). The location is fixed; only whether the model
proposes the colliding name varies.

```bash
experiments/059-rename-capture/repro.sh 14
```

## Corrections to the brief

1. **"A capture shows as `x !== x` in the emitted text" — false as a search
   strategy.** The final file has 45 self-comparisons and the pre-rename source
   has the same 45, identically distributed: `x !== x` is the standard NaN idiom
   (`value !== value`, lodash's `eq`). The capture is invisible by that method
   because the file on disk is not the checked artifact — see 2.
2. **The preserved `runtime.js` was the WRONG artifact.** Reconcile, the
   deferred sweep and the family permutation all replace the output after
   validation. The checked code had 16,384,801 tokens; the written file had
   16,120,630. Diffing the file reported a divergence at token 145 — a
   variable-declaration merge, unrelated — and cost a full investigation cycle.
   Fixed: `<file>.validated` is now preserved and is the artifact that pairs
   with `.original`.

   **Always verify `tokens(.validated) == the diagnostic's "of N tokens each"`
   before trusting any diff.** That single check caught this.

3. **The suspect list was too narrow.** The brief pointed at the prior-version
   transfer path; both renames came from the **LLM** path.

## What is now in place

Every one of these was built during this arc, and the diagnosis used all of them:

- the cold re-parse invariant catches it and marks the run failed
- `run.sh` refuses to publish the pair's KPIs as clean
- the diagnostic names the diverging token and both contexts
- the detail survives into the committed run status
- `.original` and `.validated` are preserved for post-hoc diffing
- the `--diagnostics` strategy trail names the binding, location and strategy

## What is NOT done

- **no fix**
- no failing unit test that reproduces the guard bypass (the isolated
  two-rename sequence does NOT reproduce it — that is a finding, and it means
  the bypass needs whatever additional state the real pipeline carries)
- the bug is still live on `main`
