# 053 — auditing the two shingle flags

> ## STATUS: COMPLETE. Instrumentation shipped, behaviour unchanged.
>
> Method and tests were fixed before any number was read. Nothing here changes
> what the pipeline emits — one permanent log line and one env-gated probe.
>
> **Flag 1 (the cascade was never instrumented) is FIXED**, and the first census
> it printed contains a surprise nobody had seen: `enclosingStatement` is the
> second-strongest tier at **21.1%** of all matches, while the shingle
> tiebreaker resolves **0.1%** and two tiers never fire at all.
>
> **Flag 2 (the self-hash prefix) is CONFIRMED and NOT SHIPPED.** For a function
> whose shingle set is entirely edge n-grams the score is **identically 0.000**
> however identical the two call graphs are — not a bias, a degenerate metric.
> But it flips **10 of 206** shingle-decided close pairs on this hop, two orders
> of magnitude under the gate's ±2,800-line resolution (rule 11).
>
> One claim of my own did not survive: I first described the penalty as scaling
> with **callee count**. It scales with distinct callee **shapes** — edge n-grams
> dedupe in the Set and the shape key is coarse — so the affected population is
> smaller than I implied.

Two gaps surfaced while explaining shingling, both of the shape rule 11 warns
about: a mechanism nobody could observe.

## Flag 1 — the function cascade was never instrumented

`ResolutionStats` has been computed on every run since the cascade was written
and **never surfaced anywhere**. The module-binding cascade got a `debug.log`
line; the function side did not. So "how many matches does each tier resolve"
— including the shingle tiebreaker, tier 8 — has never been observable, in an
arc that spent twenty experiments reasoning about matching.

**Fix:** `logCascadeStats` in `prior-version.ts`, permanent, at `-vv`,
mirroring the module-binding line. Per-tier counts plus the three guards
(injectivity demotion, singleton rejection, still-ambiguous).

## Flag 2 — the self-hash prefix in `shinglesCorroborate`

Every edge n-gram is `myStructuralHash→calleeShape`. Inside a hash bucket the
prefix is constant across candidates, so the cascade tiebreaker pays nothing for
it. But **`shinglesCorroborate` compares CLOSE pairs, which by construction did
not pair by hash** — so the two prefixes differ, no edge n-gram can intersect,
and each distinct callee shape (they dedupe in the Set) adds two tokens to the
union and none to the intersection.

With `S` distinct callee shapes and `F` fully-agreeing feature tokens the
ceiling is `F / (F + 2S)`. A pair with 5 feature tokens and 3 callee shapes
cannot reach the 0.50 floor **even if every literal, property and external call
agrees**.

### Verified before measuring

Two characterization tests in `src/analysis/function-fingerprint.test.ts` pin
the mechanism on a synthetic pair whose features agree completely and whose
hashes differ by one added statement:

- similarity is **0.750** as computed, **1.000** with the prefix made constant;
- no edge n-gram intersects at all once the hashes differ.

The first draft of that test failed for the right reason — it passed the code
fragment where the helper wanted the whole program — which is the only reason to
write the test before the measurement.

### How the population is measured

`HUMANIFY_SHINGLE_PROBE=1` logs, per close pair, the score as computed, the
score with the prefix made constant, the edge/token counts, and the verdict each
would produce. It changes nothing — `corroborated` is decided by the caller
either way.

`analyze-probe.ts` then reports the only population that matters: pairs with
**`aligned=0`**, where no statement aligned so the shingle score is the sole
corroboration and its verdict decides whether the pair transfers names or is
downgraded to LLM context. A flip on a pair that already had an aligned
statement is free.

## Results — one real 215→216 run, killed after the matching phase

Production-exact: the probe saw **844 close pairs**, which is exactly the
`closeMatch` count the committed exp050-cold run recorded for this hop. Same
inputs, same matching, no LLM call involved (matching runs before the first
prompt).

### Flag 1 — the cascade census, first time it has ever been printed

```
37966 unique-hash, 0 identity, 6000 memberKey, 13053 enclosingStmt,
  336 calleeShapes, 36 callerShapes, 96 calleeHashes, 0 twoHopShapes,
   62 shingle, 3499 propagation, 421 ordinal, 562 pools;
   16 injectivity-demoted, 0 singleton-rejected, 858 ambiguous, 834 unmatched
```

| tier                    |    matches | share of 61,931 |
| ----------------------- | ---------: | --------------: |
| unique structural hash  |     37,966 |           61.3% |
| **enclosing statement** | **13,053** |       **21.1%** |
| memberKey               |      6,000 |            9.7% |
| propagation             |      3,499 |            5.6% |
| interchangeable pools   |        562 |            0.9% |
| ordinal                 |        421 |            0.7% |
| calleeShapes            |        336 |            0.5% |
| calleeHashes            |         96 |            0.2% |
| **shingle similarity**  |     **62** |        **0.1%** |
| callerShapes            |         36 |            0.1% |
| identity / twoHopShapes |          0 |              0% |

Three things nobody could see before:

- **The shingle tiebreaker resolves 0.1% of matches.** As a matcher it is
  almost inert. Its load-bearing use is elsewhere — corroborating close pairs
  and binding roles.
- **`enclosingStatement` is the second-strongest tier, at 21%.** A fifth of all
  matches come from the statement a function sits in, not from the function.
  That is a much larger contributor than its position in the cascade suggests.
- **Two tiers never fire on this hop** (`identity` for functions, `twoHopShapes`)
  and the **singleton-rejection guard never triggers** (0 of 37,966 singleton
  accepts contradicted).

### Flag 2 — what the self-hash prefix costs, on real pairs

|                                                |   pairs |                             |
| ---------------------------------------------- | ------: | --------------------------- |
| close pairs probed                             |     844 |                             |
| — already corroborated by an aligned statement |     616 | shingles moot               |
| — empty shingle set                            |      22 | refused; prefix irrelevant  |
| — **decided by the shingle score alone**       | **206** |                             |
| of those: pass either way                      |      45 | 21.8%                       |
| **FAIL with prefix, PASS without**             |  **10** | **4.9%**                    |
| pass with prefix, fail without                 |       0 | 0% — the change is monotone |
| fail either way                                |     151 | 73.3%                       |

Mean score on the 206: **0.216 as computed → 0.266** with a constant prefix.

The flipped pairs show the defect is worse than a bias for one class:

```
input.js:300629:21   asis=0.000 -> 1.000   edges=2/2  tokens=2/2
input.js:283874:14   asis=0.000 -> 1.000   edges=1/1  tokens=1/1
input.js:448952:2    asis=0.333 -> 1.000   edges=1/1  tokens=2/2
input.js:132440:2    asis=0.000 -> 0.667   edges=2/3  tokens=2/3
input.js:87021:2     asis=0.000 -> 0.667   edges=6/4  tokens=6/4
```

When a function's shingle set is **entirely edge n-grams** — no string literals,
no external calls, no property accesses, which is every small pure-dispatch
function — the intersection is empty by construction and the score is
**identically 0.000 no matter how identical the two call graphs are**. For that
class `shinglesCorroborate` cannot return anything but `false`. Three of the ten
flips go 0.000 → 1.000.

### Verdict: correctly diagnosed, too small to ship

The fix is one line and provably local — `tryShingleResolve` compares an old
function against candidates in **its own hash bucket**, so the prefix is constant
there and dropping it cannot change any cascade verdict. Only
`shinglesCorroborate` is affected.

But the population is **10 pairs on one hop**, each worth a signature transfer of
a few lines instead of an LLM guess. That is two orders of magnitude below the
harness's ±2,800-line per-hop resolution (rule 11), and any change inside name
allocation has a blast radius larger than the population it touches (rule 5 —
exp044 paid +3,742 lines learning that). Same verdict as every exp051 lever:
real, understood, and beneath the instrument.

**What ships from this experiment is the instrumentation, not a behaviour
change.** The cascade census is permanent; it should have existed twenty
experiments ago.
