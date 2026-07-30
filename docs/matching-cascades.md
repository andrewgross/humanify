# Matching cascades: every check that pairs a thing to its prior self

Jargon: see the [vocabulary](../experiments/034-eval-harness/VOCABULARY.md).
Naming's execution order is [`naming-pipeline.md`](./naming-pipeline.md); this
document is the **evidence** side — what gets compared, in what order, and what
each tier does when it cannot decide.

One rule runs through all of it: **every tier abstains rather than guesses.** A
wrong match is worse than a missed one — it moves code into the wrong file or
puts a wrong name on a thing, and both cost two diffs instead of one.

## The three hashes everything starts from

They are NOT interchangeable, and the difference is load-bearing.

```mermaid
flowchart LR
  subgraph keys["the join keys"]
    SH["<b>structuralHash</b><br/>function / binding bodies<br/>names masked<br/><b>literals BLURRED</b><br/>string→length, number→magnitude"]
    ST["<b>statementHash</b><br/>top-level statements<br/>names masked<br/><b>literals VERBATIM</b>"]
    SS["<b>structuralSignature</b><br/>whole vendor files<br/>bindings→slot ordinals<br/><b>literals VERBATIM</b>"]
  end
  SH --> A["<b>A.</b> function + binding<br/>matching cascade"]
  ST --> B["<b>B.</b> statement twins<br/>(whole-statement identity)"]
  ST --> C["<b>C.</b> file placement<br/>cascade"]
  ST --> D["<b>D.</b> within-file<br/>emission order"]
  SS --> E["<b>E.</b> vendor body reuse"]
```

`structuralHash` **cannot see** a changed endpoint URL or a changed timeout of the
same length — that is why vendor reuse is keyed on `structuralSignature` instead
(exp046). `statementHash` masks every identifier name, so its own docstring warns
that short statements _"collide across unrelated code"_; every consumer of it
carries an equal-count or uniqueness gate for exactly that reason.

## A. The function / binding matching cascade

`src/analysis/fingerprint-index.ts` → `matchFunctions`. Module-level bindings run
the **same** cascade via `buildBindingFingerprintIndex`, alternating rounds with
functions so each side's matches crack the other's ambiguous buckets.

```mermaid
flowchart TD
  start(["prior function"]) --> bucket{"candidates sharing<br/>its structuralHash"}

  bucket -->|"0"| unm["<b>UNMATCHED</b><br/>→ close-match, then LLM"]
  bucket -->|"1"| single{"singletonContradicts?<br/>memberKey / propertyAccesses /<br/>externalCalls disagree"}
  bucket -->|"2+"| casc

  single -->|"yes"| unm
  single -->|"no"| ok["<b>MATCH</b> — structuralHashUnique<br/><i>the majority of all matches</i>"]

  subgraph casc["disambiguation cascade — first tier to leave ONE candidate wins"]
    direction TB
    t1["<b>1. identity</b><br/>caller-supplied: correspondence<br/>under the binding match result"]
    t2["<b>2. memberKey</b><br/>the property key it is assigned to"]
    t3["<b>3. enclosing statement</b><br/>equal-count bijection over the<br/>bucket, paired by source position"]
    t4["<b>4. calleeShapes</b> — blurred callees"]
    t5["<b>5. callerShapes</b> — blurred callers"]
    t6["<b>6. calleeHashes</b> → <b>7. twoHopShapes</b>"]
    t7["<b>8. shingleSimilarity</b><br/>Jaccard tie-break"]
    t1 --> t2 --> t3 --> t4 --> t5 --> t6 --> t7
  end

  casc -->|"exactly 1 left"| ok
  casc -->|"filter emptied the pool"| contra["<b>CONTRADICTION → ambiguous</b><br/>a candidate rejected by strong<br/>evidence must not win later"]
  casc -->|"still 2+"| amb["<b>AMBIGUOUS</b>"]

  ok --> inj{"is this fresh function<br/>claimed by 2+ priors?"}
  inj -->|"yes"| demote["<b>injectivity demotion</b><br/>ALL claimants back to ambiguous"]
  inj -->|"no"| kept(["matched"])

  demote --> amb
  contra --> amb

  amb --> p1["<b>propagation</b><br/>call-graph constraints +<br/>matched-reference evidence"]
  p1 --> p2["<b>binding alternation</b><br/>re-run both cascades with the<br/>other side's new matches"]
  p2 --> p3["<b>ordinal</b><br/>equal counts, EVERY member<br/>undecided, identical evidence<br/>keys → pair by source order"]
  p3 --> p4["<b>interchangeable pools</b><br/>greedy prior-anchor affinity<br/>over certified twins"]
  p4 --> stillamb(["still ambiguous → LLM names it"])

  unm --> cm["<b>close match</b><br/>cosine ≥ threshold on feature<br/>vectors, top-K, greedy 1:1"]
  cm --> corr{"corroborated?<br/>≥1 aligned statement<br/>OR shingle overlap"}
  corr -->|"yes"| xfer(["signature + body-local transfer"])
  corr -->|"no"| hint(["LLM context only — never a rename"])
```

The three guards are the whole precision story: **singleton rejection** (a deleted
helper and an unrelated added helper are alone in their bucket and would
auto-match), **contradiction** (an emptied filter stops the search instead of
falling through to weaker evidence), and **injectivity demotion** (two priors
claiming one fresh function means at most one is right, so neither gets it).

## B. Statement twins

Between the cascade and the transfer phase: a top-level statement whose
`statementHash` occurs **exactly once on each side** is whole-statement identity —
literals included — and outranks everything, including an ordinal exact match that
cross-paired same-shaped siblings. Equal-count family buckets pair by matched-
reference identity keys instead. This is tier 1.1 of the transfer pipeline.

## C. The file placement cascade

`src/split/stable-split.ts` → `PLACEMENT_TIERS`, in evidence-strength order. The
first tier to name a file wins; the last never abstains, so every statement lands
somewhere. `--diagnostics` records which tier placed each statement
(`placement-trail.ts`).

```mermaid
flowchart TD
  s(["fresh top-level statement"]) --> h{"<b>1. hash</b><br/>same statementHash, equal counts<br/>both sides, every prior occurrence<br/>in ONE file"}
  h -->|"yes"| place(["file decided"])
  h -->|"no"| pre{"<b>2. identity preempt</b><br/>matched binding's unanimous home<br/><i>and it disagrees with the name vote</i>"}
  pre -->|"yes"| place
  pre -->|"no"| ap{"<b>3. anchor preempt</b><br/>EVERY declared name is a minted<br/>counter, and rare literals point<br/>at a different prior statement"}
  ap -->|"yes"| place
  ap -->|"no"| votes{"do the declared names<br/>agree on a file?"}

  votes -->|"agree, some vote positional"| ord["<b>4. ordinal</b>"]
  votes -->|"agree, every voter has<br/>exactly ONE prior home"| nm["<b>5. name</b>"]
  votes -->|"disagree"| all{"<b>6. all-same</b><br/>unanimous subset of<br/>single-home voters?"}
  votes -->|"nobody voted"| fill{"<b>7. identity fill</b><br/>matched binding's home<br/>unanimous (fill only)"}

  ord --> place
  nm --> place
  all -->|"yes"| place
  fill -->|"yes"| place

  all -->|"no"| anch{"<b>8. content anchor</b>"}
  fill -->|"no"| anch
  anch -->|"yes"| place
  anch -->|"no"| loc["<b>9/10. conflict / novote</b><br/>LOCALITY: follow the<br/>preceding neighbour"]
  loc --> place
```

`declaredNames` includes **function parameters**, which is why tier 6 exists: on
215→216 a parameter named `inputData` (39th of 53 prior homes) outvoted the
statement's own two correct votes and sent 149 lines to locality.

The content anchor (tier 8) is four gates, each abstaining:

```mermaid
flowchart LR
  a["rare string literals<br/>of the fresh statement"] --> g1{"<b>1.</b> rare on BOTH sides<br/>(one statement per release)"}
  g1 --> g2{"<b>2.</b> all of them resolve to<br/>the SAME prior statement"}
  g2 --> g3{"<b>3.</b> ≥50% identifier<br/>tokens shared"}
  g3 --> g4{"<b>4.</b> no other fresh statement<br/>claims that same prior"}
  g4 --> v(["inherit its file"])
  g1 -.->|"no"| ab(["abstain"])
  g2 -.->|"no"| ab
  g3 -.->|"no"| ab
  g4 -.->|"no"| ab
```

Gate 3 is not decoration: without it one shared string paired a 5,073-line
statement with a 7-line one and inflated a hop's relocation reading 4.9×.

## D. Within-file emission order, and the require alias

Placement decides _which file_; these decide _where in the file_ and _what the
importer calls it_. Both are pure emit-time functions — deterministic, downstream
of every LLM prompt.

```mermaid
flowchart TD
  subgraph order["order — alignFileStatements"]
    o1{"prior emit sequence<br/>for this file?"} -->|"no"| ob(["fresh bundle order"])
    o1 -->|"yes"| o2{"<b>precision gate</b><br/>alignmentKey = (hash, declared name)<br/>occurs exactly ONCE per side"}
    o2 -->|"fewer than 2 qualify"| ob
    o2 -->|"ok"| o3["claim prior rank;<br/>ambiguous statements anchor to<br/>their predecessor (rank + 0.5)"]
    o3 --> o4["<b>load-order filter</b><br/>keep read-after-write, write-after-write,<br/>write-after-read; effect-bearing<br/>statements are barriers"]
    o4 --> oe(["emitted order"])
  end

  subgraph alias["import alias — buildNsVars"]
    a0{"<b>0. prior alias</b><br/>uncontested among prior claims<br/>AND still legal"} -->|"yes"| ak(["keep it"])
    a0 -->|"no"| a1["<b>ladder:</b> basename → widen up the<br/>path → sanitized path → path hash"]
    a1 --> a2{"contested by two files<br/>at this tier?"}
    a2 -->|"yes"| a3["<b>NEITHER takes it</b><br/>both widen a tier"]
    a3 --> a1
    a2 -->|"no"| a4{"<b>nsNameIsFree</b><br/>valid ident · not reserved · not a<br/>global · unclaimed · <b>not shadowed in<br/>ANY importing file</b>"}
    a4 -->|"no"| a1
    a4 -->|"yes"| ak
  end
```

The bolded clause in `nsNameIsFree` is the mechanism exp051 measured and declined
to change: aliases are **one per module tree-wide**, so one importer gaining a
local named `kairosCron` widened the alias in all ~20 importers
(`kairosCron → logTaskEventKairosCron`, ≤256 git lines on 118→119).

## E. Vendor

Vendored libraries are never named, so Bun's local-name reroll would otherwise
churn ~1,540 files a hop. Two checks:

- **body reuse** — if `computeStructuralSignature` matches, write the prior
  release's bytes verbatim. Keyed on the signature, **not** `structuralHash`,
  which would ship last release's endpoints and timeouts.
- **manifest order** — `_bun-modules.json` is emitted in the **prior release's**
  order, with `hashOrdinal` (position within the hash group) as the tie-break.
  Sorting it by any content-derived key was measured and is worse: a changed sort
  key relocates the entry, turning an in-place edit into a delete plus an add.

## Where each tier's decisions are recorded

| surface                             | trail                                  | flag            |
| ----------------------------------- | -------------------------------------- | --------------- |
| function / binding matching         | `resolutionStats` per stage            | always          |
| naming, per identifier              | `strategyTrails` (`strategy-trail.ts`) | `--diagnostics` |
| file placement, per statement       | `placement-trail.ts`                   | `--diagnostics` |
| what a rename pass actually shipped | pass-specific move trail               | pass-specific   |

A pass with an empty trail cannot have moved a KPI, however the KPI reads —
that is measurement-pitfalls rule 11, and it is the reason these exist.
