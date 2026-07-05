# Why module bindings miss the cache

Run B's prior-version cache hit ~99.4 % on functions but only ~74 % on
module bindings (`var X = ...` at module scope). That gap is the dominant
source of noise in the v119↔v120 `runtime.js` diff — 20,503 of 30,745
hunks (66.7 %) are pure rename swaps on module bindings whose name was
chosen fresh by the LLM in v120 rather than reused from v119.

This document explains why, where in the code, with evidence from the
actual bundles.

## The matching pipelines, side by side

The codebase has two completely different match strategies:

### Functions — 7-stage disambiguation cascade

In `src/analysis/fingerprint-index.ts` `resolveMatch()`:

| Stage | What it filters by                                         |
| ----- | ---------------------------------------------------------- |
| 1     | `structuralHash` (unique candidate → done)                 |
| 2     | `memberKey` (object property the function is assigned to)  |
| 3     | `calleeShapes` (blurred shape of what this function calls) |
| 4     | `callerShapes` (blurred shape of what calls this function) |
| 5     | `calleeHashes` (exact structural hashes of callees)        |
| 6     | `twoHopShapes` (callees-of-callees)                        |
| 7     | `shingleSimilarity` (Jaccard tiebreaker)                   |
| +     | `propagation` (constraint propagation across the graph)    |

When the same `structuralHash` appears N times, the cascade picks the
right one using neighborhood context. Functions hit 99.4 % cache reuse
in Run B.

### Module bindings — single-stage "unique-unique" filter

In `src/cache/prior-version.ts` `matchModuleBindings()`:

```ts
for (const [hash, priorList] of priorByHash) {
  if (priorList.length !== 1) continue;        // ← bail on any v119 collision
  const newList = newByHash.get(hash);
  if (!newList || newList.length !== 1) continue;  // ← bail on any v120 collision
  …
}
```

If two or more bindings share a structural hash on either side, **none
of them get matched** by this code path. There is no fallback, no
disambiguation, no propagation from this matcher.

That's the structural reason for the gap. But the actual cache hit rate
turns out to be much better than this single-stage filter would predict,
because two other code paths help.

## What actually drives Run B's 13,302 binding cache hits

Three sources contribute, recorded as distinct debug logs:

| Source                                 | Log line prefix              | Count in Run B |
| -------------------------------------- | ---------------------------- | -------------- |
| `matchModuleBindings` (unique-unique)  | `module-binding: matched`    | **2,056**      |
| `collectFunctionVarNameTransfers`      | `fn-var-name: matched`       | **1,991**      |
| Vote propagation across the call graph | `propagated: module-binding` | **9,796**      |

Sum: 13,843. The diagnostics report 13,302 unique bindings cached
(the slight excess in the logs is propagation re-touching the same
binding through different votes).

So:

- The "weak" unique-unique matcher contributes only **~15 %** of the cache
  hits.
- About **~14 %** come from the function-matcher (the 7-stage cascade)
  riding the var name through when the function is assigned to a var.
- The bulk — **~71 %** — comes from `propagation`: after some functions
  and bindings are matched, the call-graph votes for neighboring
  unnamed bindings by majority across their references.

Without propagation, the binding cache rate would be closer to **30 %**,
not 74 %. So propagation is doing the heavy lifting today. The 26 % gap
that remains is bindings the propagator can't reach.

## Why propagation can't close the last 26 %

Propagation extends matches OUTWARD from already-matched neighbors. To
match an unnamed binding X via votes:

1. Some functions that REFERENCE X must already be matched.
2. The matched functions' v119 versions must reference a v119 binding
   that has a humanified name.
3. Enough such references must agree (the "votes" threshold) to make a
   confident inference.

The bindings that miss are the ones with weak neighborhood signal:

### The empirical patterns

Measured on `/tmp/unpack-test-120/runtime.js` (v120, post-unpack, the
file Run B actually matched against v119's prior runtime):

- **6,552** top-level non-function-expression module bindings inside the
  wrapper IIFE.
- **3,053** unique structural hashes among them.
- **3,882 (59.2 %)** sit on a hash shared by ≥2 v120 bindings.
- Cross-version, only **2,521 (38.5 %)** bindings have a structural hash
  that is unique-unique in BOTH v119 and v120 — the ceiling for the
  unique-unique matcher.

The top ambiguous-hash patterns in v120 (counts = how many distinct v120
bindings collapse to the same structural hash):

| Count | Sample init expressions                                   | What it is                          |
| ----- | --------------------------------------------------------- | ----------------------------------- |
| 373   | `{}`                                                      | Empty object                        |
| 330   | `Z(()=>{wp8();})`, `Z(()=>{VkH();})`, `Z(()=>{b0();})`    | Bun ESM lazy init with 1 inner call |
| 208   | `null`                                                    | Literal `null`                      |
| 128   | `Z(()=>{mM6();AX_();})`, `Z(()=>{kK();m0();})`            | Lazy init with 2 inner calls        |
| 111   | `Z(()=>{g2();aR_();mu();})`, `Z(()=>{cR_();qR6();mu();})` | Lazy init with 3 inner calls        |
| 92    | `!1`                                                      | Literal `false`                     |
| 62    | `Z(()=>{R9q=u(f$(),1);})`, `Z(()=>{n7q=u(Kk(),1);})`      | Lazy init with single assignment    |
| 58    | `3`                                                       | Numeric literal                     |
| 57    | `Z(()=>{qvH(); g2(); rR_(); … })`                         | Lazy init with 5 inner calls        |

Two classes of failure:

**Class A: information-poor literals** (`{}`, `null`, `false`, `3`, …)

The init expression has no structure to disambiguate from. Even a perfect
matcher can't tell "the empty object at line 1234" from "the empty object
at line 5678" without external context (declaration order, usage site,
surrounding scope contents). This affects ~700 bindings.

**Class B: Bun ESM lazy-init wrappers** (`Z(()=>{…})`)

These are the much bigger problem. `Z(()=>{wp8();})` and
`Z(()=>{VkH();})` have **DIFFERENT callees** (`wp8` vs `VkH`) but
**IDENTICAL structural hashes** because the structural hash normalizes
identifier names to positional placeholders. The information needed to
distinguish them is RIGHT THERE in the init expression — it's just being
thrown away by the hash function.

In v119 (after rename), those wrappers contain matched names like
`initializeArrayHelpers`, `getEntry`, etc. In v120 (before rename),
they contain `wp8`, `VkH`, etc. After v120's matching pass, many of
those inner names ARE matched (because functions are matched at 99.4 %).
A lazy-init binding can be uniquely identified by which already-matched
function it wraps — but the matcher doesn't look at that.

This is the single biggest win available: ~1,000–1,500 lazy-init
bindings that have unique callee identity but get bucketed together by
the structural hash.

## Proposed fixes, ranked by leverage

### 1. Apply the function cascade to module bindings (biggest win)

Functions get `calleeShapes`, `callerShapes`, `calleeHashes`, etc. as
disambiguation. Module bindings currently get none of that. The same
fingerprint fields could be computed for binding inits and the same
`resolveMatch()` cascade applied.

Concretely: when `matchModuleBindings()` finds N v120 candidates with
the same hash that also has M v119 candidates with the same hash,
disambiguate them using calleeShapes/calleeHashes of the inner
expressions.

Expected impact: most of the Class B lazy-init patterns become matchable.
Should reduce the 4,672 unmatched bindings to under 1,000.

### 2. Propagation by binding-to-binding edges, not just function-to-binding

Today propagation extends from matched FUNCTIONS to neighboring bindings.
Extending it to also propagate from MATCHED BINDINGS to other bindings
would catch cases like `var X = ARR_ENUM` where `ARR_ENUM` is matched
but `X` isn't.

Expected impact: smaller — maybe 300-500 additional matches.

### 3. Accept "ambiguous within version but cross-version paired" matches

If v119 has 5 bindings with hash H, and v120 has 5 bindings with hash H,
we can use source-order or surrounding-context heuristics to pair them
up 1-to-1 instead of throwing all 10 out. The matching wouldn't be
fingerprint-quality but for boilerplate (`var X = {}`) it'd be better
than fresh-LLM names that drift between versions.

Expected impact: covers the Class A literals — ~700 bindings.

### 4. Skip-list bindings whose init is too information-poor to humanify well

For `var X = {}` and similar, even fresh LLM names are arbitrary. Just
preserve the original minified name (or apply a deterministic suffix
based on usage shape) and let the diff show the minified-vs-minified
comparison cleanly. Less work for the LLM, smaller diff noise.

Expected impact: ~700 bindings. Avoids the cost of fresh LLM calls on
these and produces stable names cross-version.

## What this means for the diff goal

Today: 167K diff lines on runtime.js, ~67 % of which is module-binding
rename noise.

If fixes 1 + 2 + 3 land, the binding cache rate should approach the
function cache rate (~99 %), and the runtime.js diff should shrink
roughly **10x** — toward 15–20K lines, dominated by genuine source
changes between v119 and v120.

The largest single lever is **fix 1** (extending the cascade to
bindings), and it reuses code that already exists for functions — it's
mostly plumbing rather than new algorithm work.
