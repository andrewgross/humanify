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
observe the first rename?** Prime suspect remains `fastRenameBinding`
(`validated-rename.ts:160`) bypassing Babel's `scope.rename()` and refreshing
only its own scope's bindings map — but this is STILL UNVERIFIED, and the
sequence probe above did not reproduce it in isolation, so a clean two-rename
sequence is _not_ sufficient to trigger it.

Start from `src/rename/processor.ts` (the LLM application path) and ask whether
the scope objects it renames through are re-derived between applications.

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
