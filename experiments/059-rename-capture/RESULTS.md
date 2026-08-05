# 059 — RESULTS: the capture is REAL, fully characterised, and FIXED

> ## STATUS: **FIXED — claim ledger keyed by the block node. Mechanism confirmed firing in production and caught 4 of 4 times across 10 cold runs.**
>
> Read this before the [brief](./README.md) — the brief is what was believed
> beforehand, and two of its framings were wrong. The mechanism section below
> was ALSO corrected twice, each time by a measurement that refuted a claim I
> had already written down. Read top to bottom; later sections retract earlier
> ones (rule 9).
>
> Jump to [the fix](#fixed--the-claim-ledger) and
> [how it was verified](#the-evidence-standard-and-why-the-first-clean-run-was-not-it).

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

### MEASURED 2026-08-03 — the two renames came from DIFFERENT SCOPE-TREE ERAS

Recorded `scopeUid` (Babel's per-Scope-object id, assigned in construction
order) and `scopeBlock` for every applied LLM rename. Reproduced on run 1:

```
outer `e`   scopeUid 577865   block 341066:342264
inner `r`   scopeUid 278209   block 341123:341437
```

Two facts, both measured:

1. **They are genuinely lexically nested.** 341123–341437 is strictly inside
   341066–342264. The guards SHOULD have seen each other.
2. **The inner scope object was constructed ~300,000 scopes BEFORE its own
   enclosing scope.** In a single crawl a parent is always constructed before
   its child, so these objects cannot come from one crawl.

Across all 6,332 renames carrying a uid, the values cluster into exactly two
groups with a **151,687 gap** between them:

```
era A   274,542 .. 426,128     <- inner `r` (278209)
era B   577,815 .. 597,814     <- outer `e` (577865)
```

So the naming pass renames through **two distinct scope-tree eras**, and this
capture is a pair that straddles them. That is exactly the condition the
identity hypothesis needs: `fastRenameBinding` patches the `bindings` map of
the object it is handed, and a guard consulting an object from the other era
sees a tree in which that rename never happened — with every reference list
intact, which is what `refCount` 2 and 3 already showed.

**Still not the complete mechanism.** What is measured is that the two renames
used different-era scope objects and that the scopes are nested. What is NOT
yet shown is which object the FAILING guard actually walked to, or the order of
the two renames.

### MECHANISM FOUND — the cache clear's safety argument covers hashing, not the guards

The two eras have a source, and it is in the code with a comment that names the
hazard and then reasons past it. `src/rename/plugin.ts`:

```
 885   buildUnifiedGraph(...)              <- graph captures scope objects   ERA A
 906   prior-version matching              <- fills Babel-cache tombstones
 965   clearBabelCacheAfterPriorMatch(...) <- "induces scope re-crawls"
 970   runRenamePass(...)                  <- fresh path.scope accesses      ERA B
```

The clear exists for a real reason (exp031/exp032: prior-AST tombstones make
V8 re-hash the ephemeron table on nearly every insert — an O(n²) naming hang).
Its own doc comment states the consequence exactly:

> Clearing Babel's cache is safe for later hashing (the pre-generate structural
> invariant) because slot placeholders key by declaration node, **which survives
> the scope re-crawls this clear induces**.

That argument is correct **for hashing** — slot placeholders key by declaration
NODE, and nodes survive a re-crawl. It does not cover the rename guards, which
read `bindings` maps off Scope **OBJECTS**, and a re-crawl produces NEW objects
for the same lexical scopes. Patching `scope.bindings` on one object has no
effect on the other.

So a rename applied through a graph-held (era A) scope is invisible to a guard
walking a freshly-crawled (era B) tree, and vice versa — with every reference
list intact, which is exactly what `refCount` 2 and 3 showed.

**It is gated on a prior version** (`if (!priorVersionCode) return;`), which is
why every observed occurrence is a `--prior-version` run.

#### Status of each claim

- MEASURED: two uid clusters 151,687 apart; the capture pair straddles them;
  both guards had complete reference lists.
- FROM CODE: the clear runs between graph build and naming, is prior-gated, and
  its own comment says it induces scope re-crawls.
- INFERRED, coherent but not yet directly demonstrated: graph-held scopes are
  era A and post-clear accesses are era B, so patching one leaves the other
  stale.

#### REPRODUCED IN A UNIT TEST — and it corrected the mechanism again

`src/rename/scope-era.test.ts` reproduces the capture deterministically in
~30ms, emitting `let changed = dirPath !== dirPath`. It is `it.skip`-ped
because it asserts the CORRECT behaviour and therefore fails on current main;
remove `.skip` when the bug is fixed, and do not weaken the assertions.

Writing it corrected the mechanism a second time. My inference was:

> a re-crawl produces a STALE tree, so the guard misses the other rename

**That is wrong.** A fresh crawl reads the MUTATED ast, so a tree built AFTER a
rename sees it perfectly. The first version of this test proved that by
passing when it should have failed.

The real hazard is the reverse, and ORDER is the whole thing:

1. an old scope object is RETAINED across the clear — which is exactly what the
   unified graph does, since it captured scopes at build time (line 885);
2. the naming pass crawls fresh (era B) and holds those scopes;
3. a rename lands through the RETAINED era-A object — mutating the AST and era
   A's bindings map, but not era B's;
4. a guard walking era B was crawled BEFORE that rename and is never told.

So it is not that either tree is inherently stale. It is that **two live trees
exist over one AST and a write through one does not propagate to the other.**

#### The falsifiable prediction that would close it

**Run WITHOUT `--prior-version`: no clear, one era, and this capture class
cannot occur.** If a no-prior run ever produces it, this mechanism is wrong.

#### Candidate fixes — NOT evaluated, do not pick one from this list alone

- **Do not** simply remove the clear: it prevents the exp031/exp032 O(n²)
  naming hang. That is why it is there.
- Re-derive the graph's scope references after the clear, so naming has one
  tree.
- Have `applyLlmRename` resolve the scope from the CURRENT program scope rather
  than using a graph-held one.
- Note the clear cannot simply move earlier: the tombstones it removes are
  created by prior matching (906), which runs after the graph build (885).

### The two probes that remain, if the prediction does not settle it

1. **Record rename sequence.** The trail has no ordering. Add a monotonic
   counter to each attempt; then it is knowable which of the two was second and
   therefore which guard was the one that should have rejected.
2. **Instrument the walk itself.** In `wouldRenameShadowInChildScope`, record
   the uid of each scope walked and whether it held the target name. If the
   walk visits an era-B arrow scope while the rename landed on era A's, the
   mechanism is proven and the fix is about scope-tree consistency across eras
   — NOT about the guard predicates, which are correct.

Find where the second era comes from: `clearBabelCacheAfterPriorMatch` runs
BEFORE naming (checked), so something inside the naming pass is re-crawling.

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

## FIXED — the claim ledger

> Supersedes "What is NOT done" as it stood on 2026-08-03, which read "**no
> fix**, no failing unit test, the bug is still live on `main`". All three are
> now false. The claim that an isolated two-rename sequence "does NOT reproduce
> it" was also wrong — it does, once the ORDER is right (see
> [the unit reproduction](#reproduced-in-a-unit-test--and-it-corrected-the-mechanism-again)).

### CORRECTED 2026-08-04 — the cause is wider than "the cache clear"

Everything above this line describes the trigger as
`clearBabelCacheAfterPriorMatch`. A fresh-context review of Babel's own source
corrected that, and I verified both points directly:

**1. The clear does not rebuild anything.** `clear()` is `clearPath()` +
`clearScope()`, each of which assigns a **new empty WeakMap**
(`@babel/traverse/lib/cache.js:14-22`). Retained `Scope`/`NodePath` objects keep
working, untouched. The hazard is not that objects are rebuilt — it is that
**nothing is**, so a retained handle silently becomes a second tree.

**2. A second tree can open with NO clear at all.** Babel's `Scope` constructor
returns the cached scope only when the cached entry's PATH is the same object:

```js
const cached = _cache.scope.get(node);
if (cached?.path === path) return cached; // else: brand new Scope
```

(`@babel/traverse/lib/scope/index.js:320-323`.) So any fresh `NodePath` for an
already-scoped node mints a fresh `Scope`. The blast radius is **any path-cache
eviction**, not the two `clearBabelTraverseCache()` calls.

**3. The split is INTRA-FUNCTION, not phase-level.** The framing above — "era A
= the graph, era B = the naming pass" — is a phase-level story for a
scope-level phenomenon. A single `collectOwnedBindingInfos(fnPath)` returns
`BindingInfo`s whose `.scope` fields straddle both epochs, because
`collectScopeOwnBindings` reads the retained `fnPath.scope` while
`collectBodyScopeBindings` and `collectNestedBlockBindings` mint fresh paths.

**Does this invalidate the fix?** No — it generalises it. The ledger keys on the
BLOCK NODE, which is shared however the second tree arose, so it is indifferent
to the trigger. But the _reasoning_ published for it was narrower than the
truth, and a reader who fixed only the clear would not have fixed the bug.

**Two further staleness sites this uncovered, both still live:**

- `strategy-trail.ts:85` keys entries by Babel `Binding` OBJECT, so one lexical
  binding gets TWO trail entries across an epoch boundary — meaning
  `postSettleAttempts`, the clobber detector built for exactly this class of
  bug, **cannot fire across epochs.** The diagnostic under-reports the thing it
  exists to catch.
- `getUsedIdentifiers` (`context-builder.ts`) walks the retained `fnPath.scope`,
  so the LLM prompt's "names already in use" is read from one epoch while
  renames land in the other.

### What the fix is

A `WeakMap<blockNode, Map<name, Binding>>` in `validated-rename.ts`, recording
every name an APPLIED rename bound, keyed by the scope's **block node** rather
than the Scope object.

Block nodes are the era-independent part: both scope trees wrap the same nodes,
so one ledger serves both. This is the same property the hash tier already
relies on across this clear — "slot placeholders key by declaration node, which
survives the scope re-crawls this clear induces". That comment was right about
hashing and silent about the guards.

### Why it is additive-only, on purpose

A claim exists only where a rename really bound that name, so consultation can
turn an approval into a rejection and never the reverse.

It deliberately does **not** correct the opposite staleness — the other era's
map still keyed under a now-dead old name. Correcting that would LOOSEN a
guard, and a loose guard is the entire bug.

Applying a rename also DELETES the old name's claim. That name is genuinely
free again, and refusing a free name only moves the collision — rule 5/6, which
exp044 paid +3,742 lines to learn.

### The three consultation points, and why two nearly shipped dead

| point                   | guard             | direction it fixes         | fired in production? |
| ----------------------- | ----------------- | -------------------------- | -------------------- |
| `getRenameRejection` q1 | `target-in-scope` | two bindings in ONE scope  | never (0 of 10 runs) |
| `resolveOuterBinding`   | `target-visible`  | an ANCESTOR renamed first  | 3 of 10 runs         |
| child-scope walk        | `shadows-child`   | a DESCENDANT renamed first | 1 of 10 runs         |

The headline reproduction exercises only the third. Planting a break in the
other two left the suite **green** — which is exactly how `singletonContradicts`
stayed structurally dead for 11,094 accepts while reporting `singletonRejected:
0`, a number that reads like perfect precision.

So the other two directions got their own tests, and all three were then
confirmed load-bearing by disabling each in turn and watching a test go red.

**Do not delete a consultation point because no test covers it. Write the test
that proves it is reachable, or delete the guard.** Note `target-in-scope` has
never been observed firing on a real pair — it is tested and reachable, but so
far only theoretical.

### Coverage of the choke point — checked, not assumed

Every rename application in `src/` funnels through `validated-rename.ts`:

- direct `scope.rename(` callers outside it: **none** (the one grep hit is
  `debug.rename({`, a logger)
- raw `identifier.name = ` / `node.name = ` writes outside it: **none**

So the ledger sees 100% of applied renames, and a bypass would be a new code
path, not an existing gap.

## The evidence standard, and why the first clean run was not it

`197->198` exited **0** with the fix, where `baseline-2026-08-03` exited 1.

**That was not proof, and I did not report it as such.** The capture fires on 8
of 40 committed cold runs, so `P(one clean run | no fix) = 0.8`. This
experiment's own README lists "trusting a single clean run" among its pitfalls,
and this bug had already produced two wrong claims from small samples.

So the fix carries an instrument, `renameClaims` in the stats JSON:

- `ledgerOnlyRejections` — verdicts the ledger flipped ALONE: the scope's own
  bindings map called the name free, and the ledger knew a rename had bound it.
  Each one is a cross-era capture that would otherwise have shipped.
- `byGuard` — which guard flipped, so one hot site cannot hide inside a total.
- `claimsRecorded` — the denominator.

Counted at the DECISION, never at resolution. The ancestor lookup finds a
claimed binding on many renames in a hot scope; counting those would inflate
the number with every safe shadow, and an inflated counter would "prove" the fix
worked on inputs where it did nothing. A test pins it: a ledger-sourced resolve
ending in no capture counts zero AND the rename stays ALLOWED.

Written **always, including all-zero** — unlike `vendorNaming` beside it, which
is omitted when the namer was never asked. The ledger is on the guard path of
every rename, so it always ran.

### Ten cold runs of `2.1.197→2.1.198`

| runs | result                                                |
| ---- | ----------------------------------------------------- |
| 10   | exit 0 — every one (baseline exited 1 on this pair)   |
| 4    | ledger flipped a verdict (n1, n6, n8, n9)             |
| 4    | of those 4 occurrences, caught — all of them          |
| —    | cost: 4 refusals across ~1.82M claims, ~1 per 455,000 |

**This is what changed the verification, and it is the part worth carrying
forward.** The original plan was to stack clean runs until `0.8^n` looked small.
The counter shows that reasoning was wrong: six of the ten runs never
encountered the condition at all, so their clean exits carry no information
about the fix. Counting them toward confidence would have inflated a number
measuring mostly luck — the same shape of error as rule 11.

The informative sample is **occurrences**, not runs: 4 arose, 4 were caught.
`P(10 clean | no fix) = 0.8^10 = 0.107` is suggestive on its own and no more;
the weight is carried by the counter showing the mechanism firing and being
caught every time.

### CONFIRMED on the full 4-pair gate (2026-08-05)

A cold 4-pair eval of main — `session-2026-08-05`, `REBASE_PRIOR=1`:

| pair            | exit  | boot gate | cache |
| --------------- | ----- | --------- | ----- |
| 2.1.85→86       | 0     | OK        | +0    |
| 2.1.118→119     | 0     | OK        | +0    |
| **2.1.197→198** | **0** | OK        | +0    |
| 2.1.215→216     | 0     | OK        | +0    |

`exitCode 0, errors: []` on all four run-status files, no manifest warnings, and
`cache +0` on every pair — so every prompt was live and rule 10 is satisfied.
`baseline-2026-08-03` had **2.1.198 exit 1** on this same pair, from this bug.

That pair has now completed **11 clean cold runs** since the fix (10 targeted +
this gate) against a measured ~20% failure rate beforehand. Taken with the
counter evidence — the condition AROSE 4 times and was caught 4 of 4 — this is
the strongest form the case gets without waiting years for the tail.

**Still not claimed:** that the capture is impossible. The fix is additive and
guards a specific staleness; a different route to two bindings sharing a name
would not be caught by it. What is claimed is that this mechanism, on these
inputs, is closed.

### An open question, labelled as one

The ledger flips a verdict on **40%** of runs while the capture failed on
**~20%**. These do not contradict — a cross-era collision only becomes an
invariant FAILURE when it changes a resolved signature detectably, so roughly
half surfacing is plausible. **That is a hypothesis, not a measurement.** It
predicts that the ~20% of ledger hits which never failed were captures the cold
re-parse check cannot see, which would mean the invariant check has a blind
spot worth its own experiment.

### KNOWN AND OUT OF SCOPE: the same staleness costs churn, not correctness

The mirror case is still live. After era A renames `X -> dirPath`, era B's map
still keys that binding under `X`. If era B then renames it again — believing it
is still `X` — `fastRenameBinding` walks the same reference nodes and rewrites
them consistently.

That emits **correct code with a different name**: era A's choice is lost. It is
naming churn, not a capture, so it cannot produce `dirPath !== dirPath`. It is
a candidate cause for cross-version rename noise and belongs to its own
experiment with its own sizing — not to this fix.
