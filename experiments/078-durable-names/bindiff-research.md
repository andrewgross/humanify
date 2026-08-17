# BinDiff, and what transfers to our matcher

> Research commissioned 2026-08-17 (Andrew: "use a sub agent to research the
> BinDiff algorithm and how we could apply it for function and file matching.
> Its main downside is that it can get slow at larger scales, but I wonder if
> it is feasible given that we have a lot of other checks and info we can use
> to clean up and minimize the search space before applying it.")
>
> **The scale worry does not apply to us. The thing worth taking is not the
> algorithm's speed profile but one specific trick we do not have.**

## Provenance — what was read, and what was not

Verified by reading the primary source:

| source                                                                       | what it gave                                                                                                       |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Flake, _Structural Comparison of Executable Objects_, DIMVA 2004             | the 3-tuple, the both-sides uniqueness rule, iteration, 2004 results                                               |
| Dullien & Rolles, _Graph-based comparison of Executable Objects_, SSTIC 2005 | Selectors/Properties, propagation pseudocode, Small Primes Product, the δ correction                               |
| `google/bindiff` `graph_util.h`                                              | the exact MD index formula, verbatim                                                                               |
| `match/call_graph.cc/.h`, `match/flow_graph.cc`, `differ.cc`, `context.h`    | uniqueness test, drill-down, propagation, confidence sigmoid, similarity weights, **absence of any un-match path** |
| `bindiff.json`                                                               | the shipped step order and per-step confidences                                                                    |

**NOT verified:** the NATO/STO 2010 paper usually credited with the MD index
(HTTP 403 — the formula was taken from source instead, which is stronger);
what "MD" abbreviates (no primary source found, not guessed); the QBinDiff
timing table (PDF extraction mangled it — its f1 table extracted cleanly and
is quoted, the timings are not).

## The architecture, and how much of it we already are

BinDiff is an **ordered cascade of keyed steps**, descending confidence, with
propagation to a fixed point _inside each step_:

```
for each step S, strongest first:
    run S globally over still-unmatched functions
    repeat until no new matches:
        for every match found so far:
            run S on (unmatched CALLERS of A) vs (unmatched CALLERS of B)
            run S on (unmatched CALLEES of A) vs (unmatched CALLEES of B)
```

A step matches only where a key is unique **on both sides** — the 2004
equation, and `match/call_graph.cc:135` still reads `if (count1 == 1 &&
count2 == 1)`. **There is no similarity matrix anywhere.** Every step is a
group-by; ambiguity is never scored, only narrowed or abandoned.

The mapping to us is close to exact:

| BinDiff                             | us                                    |
| ----------------------------------- | ------------------------------------- |
| binary                              | released bundle version               |
| function (call-graph node)          | enclosure / module                    |
| call graph                          | import graph                          |
| basic block                         | function within a module              |
| instruction                         | statement                             |
| `hash matching`, confidence 1.0     | our rename-blind structural hash      |
| the fixed-point set after that step | **our 97.6% confidently-matched set** |

So our tier A _is_ BinDiff's step 2, and our 97.6% _is_ its post-step-2 seed.
Tier D's edge agreement is a single-pass approximation of its propagation.

## Answering Andrew's question: is the scale concern real for us?

**No.** Each step is a group-by over ≤4,850 items. The propagation loop, even
in BinDiff's own admittedly wasteful "iterate every fixed point every round"
form, is thousands of set operations. The things that make BinDiff slow —
enumerating every call-graph edge for edge-keyed steps, running a full
basic-block cascade inside each of 150,000 function matches — either do not
apply at 40–115 candidates or are ours to control. BinDiff's own source
carries the fix as a TODO: iterate _new_ fixed points only.

**The scarce resource for us is not CPU, it is the risk budget.** Every tier
below exact-hash can turn a _no match_ into a _wrong match_, and a wrong
module identity carries a name onto different code and moves files.

## What to adopt

1. **AMBIGUITY DRILL-DOWN — the biggest gap, and the least obvious.** When a
   key ties with n>1 on both sides, BinDiff does not abandon it: it recurses
   into the **next** step _restricted to that tied bucket_
   (`match/call_graph.h:172-190`). This is what makes weak signals safe —
   they only ever run inside a set a strong signal already isolated. We
   abstain on ties instead, which is why 13 of our 41 leftovers are stuck.
2. **Alternating propagation to a fixed point.** We do one pass of edge
   agreement. SSTIC 2005 is explicit that alternating importers/imports and
   iterating gives the best results.
3. **String literals as their OWN high-confidence key, not blended.** BinDiff
   ranks string references 0.7, below its structural tiers — _because
   binaries have few strings, mostly in error paths._ That ranking is a fact
   about binaries and should invert for us: minifiers preserve string
   literals verbatim, so they are high-entropy and near-unique per module.
   Ours are currently mixed into the graded token soup.
4. **Record which tier made each match.** BinDiff derives confidence from the
   histogram of steps that produced a match. We have tier COUNTS but not
   per-match provenance, so we cannot tell a hash match from a last resort
   when reading a result.
5. **A `MatchUnique` last resort, fenced.** If a matched module has exactly
   one unmatched importer on each side, pair them with no content check —
   but log at confidence 0, exactly as BinDiff does, so it can never read as
   evidence.
6. **The δ correction (SSTIC 2005 §4.5.6), if we ever use a positional
   signal.** An anchor supplies not just a narrowed candidate set but the
   OFFSET by which an insertion skewed the positional signature. It is the
   difference between a positional signal being useless and usable.

## What NOT to adopt

- **The MD index as a floating-point equality key.** The insight is sound —
  irrational weights (√2…√13 over in/out degree and BFS level) make six
  features near-injectively encodable as one number. The float
  representation is a C++ compromise; BinDiff has to sort summands before
  adding because _"summation is *not* commutative for doubles"_. With a
  byte-identity neutrality gate, a float-keyed match is a landmine. Use a
  canonical integer signature instead: sort the per-edge feature tuples and
  hash them.
- **Address / ordinal sequence matching.** BinDiff itself rates it 0.4 and
  gates it behind an independent structural agreement. We have direct local
  evidence it is worse than that: positional assignment was tried here and
  cost **+50,606 noise lines** (exp035/036).
- **The whole CFG family** — flowgraph MD index, Lengauer-Tarjan dominators,
  loop entry, jump sequence. JS ASTs are trees; a CFG must be constructed.
  Poor cost/benefit at 40–115 candidates.
- **Call sequence matching.** BinDiff scores it 0.1/0.0/0.0.
- **BinDiff's global similarity score.** We have KPIs and measured noise
  bands; a second differently-weighted quality number is a duplicate owner
  for one question (docs/responsibility.md).

## Failure modes we would inherit

1. **No revocation, ever.** Verified from source: `MatchingContext` has
   `AddFixedPoint` and lookups, and **no removal**. A wrong high-tier match
   is permanent and seeds wrong propagation. BinDiff's whole answer is "never
   make the first mistake". **We can cheaply be better than BinDiff here:**
   we now have a graded content score, so a weak-tier match whose graded
   similarity falls below a floor can be DROPPED rather than kept.
2. **Greedy, provably not optimal.** No assignment problem is solved. This is
   the central criticism in the QBinDiff line of work. Their benchmark puts
   BinDiff's f1 at 0.59–0.95 across binaries — but that is cross-compiler
   diffing of stripped binaries, far harder than consecutive releases of one
   bundle. Read as evidence of a real ceiling, NOT as a prediction for us.
3. **Identical siblings stay ambiguous forever, by design.** Both-sides
   uniqueness means N structurally identical modules never match
   structurally, and propagation only helps if their NEIGHBOURHOODS differ.
   This is exactly our zustand `getState`/`getInitialState` case. **Adopting
   BinDiff will not fix it and we should not expect it to.**
4. **A disconnected unmatched region never gets anchored.** SSTIC 2005 names
   this explicitly. If a whole feature area is refactored at once, no matched
   neighbour exists and propagation contributes exactly zero.
5. **Topology signatures break globally on graph-shape change** — inlining
   and outlining in binaries; a bundler change merging or splitting modules
   for us. Level-bearing signatures degrade everywhere, not locally. Keep a
   level-free fallback tier.
6. **Confidence is a property of the algorithm, not the evidence** — "which
   step found this, through a sigmoid". The source carries an open TODO that
   perfect matches get voted down by it. Useful as a tier label; not a
   probability.
7. **Docs drift from implementation.** Found inside BinDiff itself: the docs
   say "topological graph ordering" where the code computes a BFS depth with
   a known unhandled case, and list a step the shipped config disables. Our
   own rule 9, in someone else's codebase.

## The sizing caution, and what we already know

QBinDiff measured "anchoring" at **+9.8% to +19.8% f1** — but that is from
_no_ anchors to _some_. We are at 97.6% anchored; total headroom is **2.4%**.
Those published gains do not transfer.

**And failure mode 4 is already partly measured for us.** Of the 41
enclosures still unmatched after tier D, the exp078 diagnostic found **19
have no matched import-graph neighbour at all** (18 with no name twin, 1
with). So propagation's ceiling on the current leftovers is ~22 of 41, not
41 — worth knowing before building anything that assumes neighbours exist.

## Sources

- Flake, DIMVA 2004 — https://static.googleusercontent.com/media/www.zynamics.com/en/us/downloads/dimva_paper2.pdf
- Dullien & Rolles, SSTIC 2005 — https://static.googleusercontent.com/media/www.zynamics.com/en/us/downloads/bindiffsstic05-1.pdf
- google/bindiff — https://github.com/google/bindiff (`graph_util.h`, `match/call_graph.cc`, `match/flow_graph.cc`, `differ.cc`, `bindiff.json`, `docs/concepts.md`)
- BinDiff manual — https://www.zynamics.com/bindiff/manual/
- Cohen, David, Mori, Yger, Rossi (QBinDiff), CAID 2024 — http://www.robindavid.fr/assets/pdf/caid-24.pdf
- Quarkslab Diffing Portal — https://diffing.quarkslab.com/differs/bindiff.html
- Jia et al., function inlining and binary similarity — https://arxiv.org/pdf/2112.12928
- Dullien et al., NATO/STO 2010 — **HTTP 403, not retrieved; nothing here rests on it**
