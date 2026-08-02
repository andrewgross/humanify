# 058 — RESULTS: binding-derived placement. (B) is REFUTED. (A) is BUILT and GATED.

> ## STATUS: **(A) SHIPPED (pending merge) — gated on four pairs draw-pinned, a tree-level self-hop and a four-hop walk. (B) CLOSED — it is worse than (A), not better.**
>
> **The walk lands on the pre-code prediction within 7 lines**: −1,477 predicted
> before a line of code existed, **−1,484 measured** on the walk's own output,
> **0 lines created on any hop**. The gate's four pairs read
> **0 / 0 / 0 / −1,149** with isolation proven on all four.
>
> **One gate criterion reads FAIL and is reported as such, not waved away.** > `reloc(st)` goes 0→1 on 215→216. It is the target statement, and the KPI
> pairs statements by the same fingerprint inference the change declines — see
> "the one failing criterion" below, with the witness.
>
> The brief asked: does (A) capture most of (B)'s value? **It captures all of it and
> more.** (B) removes 21 more gate lines and 122 more walk lines than (A) — and
> **creates 61 and 154 that (A) does not**. Against a threshold of "beat (A) by
> ~1,500", it loses.
>
> **The brief's premise that the matcher "has already resolved which prior binding
> each fresh binding is, for ~97% of them" does not survive contact with the
> boundary.** What arrives at the split is 0–11 entries per hop, and on the one
> hop where a binding-identity tier contradicts the fingerprint at all, the
> evidence is a **function parameter** (`dataKey → $key`) which drags four
> plugin-cache functions into a query-params parser for **+154 git lines**.
>
> Read this file, not the brief. Claims of the brief's that did not survive are at
> the bottom (rule 9).

## TOTAL, first

**Git-capped ceilings. Counterfactual trees built with the real splitter and the
real runnable emitter; the same `diff` a reviewer runs, re-run against the prior
release. Lines removed and lines CREATED are never netted.**

| gate hop   | (A) removed | created | (B) removed | created |
| ---------- | ----------: | ------: | ----------: | ------: |
| 85→86      |       **0** |   **0** |          21 |  **61** |
| 118→119    |       **0** |   **0** |           0 |       0 |
| 197→198    |       **0** |   **0** |           0 |       0 |
| 215→216    |   **1,025** |   **0** |       1,025 |       0 |
| **gate 4** |   **1,025** |   **0** |   **1,046** |  **61** |

| walk hop       | (A) removed | created | (B) removed | created |
| -------------- | ----------: | ------: | ----------: | ------: |
| 212→213        |     **449** |   **0** |         571 |       0 |
| 213→214 (calm) |       **0** |   **0** |           0 | **154** |
| 214→215 (calm) |       **0** |   **0** |           0 |       0 |
| 215→216        |   **1,028** |   **0** |       1,028 |       0 |
| **walk 4**     |   **1,477** |   **0** |   **1,599** | **154** |

The decision rules, fixed in the brief before the measurement:

- **(A) under ~800 git-capped lines across four hops ⇒ close it.**
  **1,025 gate / 1,477 walk. Clears on both, by 28% and 85%.** Survives.
- **(B) must beat (A) by more than ~1,500 across four hops.** It beats it by
  **21 (gate) and 122 (walk)** — 1.4% and 8% of the bar — **while creating 215
  lines across the two measurements that (A) never creates.** **Closed.**

**If (A) captures most of (B)'s value, ship (A) and close (B) — the brief's own
instruction. It captures all of it. (B) is closed.**

## Task 0a — the veto. The matcher's precision, and what "0 of 37,966" measures

### The singleton-rejection guard is not a precision result. Two of its three tests are tautologies, and it does not exist on the cascade exp058 would promote.

`singleton-guard-probe.ts` runs the real `matchFunctions`; `singleton-census.ts`
replays the singleton branch over the real 215→216 pair.

| question                                                           | answer                                                                                          |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Can the guard fire at all?                                         | **Yes** — built a same-hash pair with differing `memberKey`, watched `singletonRejected` go 0→1 |
| Can two members of one hash bucket disagree on `propertyAccesses`? | **No.** 0 of 3,037 multi-member buckets, 64,493 functions                                       |
| … on `externalCalls`?                                              | **No.** 0 of 3,037                                                                              |
| Do module bindings carry `features`?                               | **No** — `buildBindingFullFingerprint` emits neither it nor `memberKey`                         |

`structuralHash` keeps **non-computed member property names and free identifiers
verbatim**, so `propertyAccesses` and `externalCalls` are _functions of the bucket
key_: two candidates in one bucket cannot disagree on them. The guard's docstring
lists three signals as if they were independent; only `memberKey` is.

On the real hop:

| cascade                                        | singleton accepts | guard had a testable signal | rejections |
| ---------------------------------------------- | ----------------: | --------------------------: | ---------: |
| function                                       |        **37,980** |     6,202 memberKey (16.3%) |      **0** |
| **module binding — the one (B) would promote** |        **11,094** |                  **0 (0%)** |      **0** |

**"0 rejections out of 37,966" is the function cascade's number, and it means the
guard checked a tautology 37,980 times and a real signal 6,202 times. On the
binding cascade the guard is asked nothing at all, on 100% of accepts.** Rule 3:
the predicate does not test what its name implies. This is not reassurance about
the binding matcher's precision; it is the absence of a measurement.

### A measurement bug of this experiment's own, and what it had suppressed

The first pass built its carry the way `057/trail-check.ts` does —
`{...emptyPriorCarry(), statementTexts}` — which leaves `matchMap` **empty** and
therefore runs the splitter with the `preempt` and `fill` tiers **switched off**.
Those are the two binding-identity tiers, i.e. exactly the subject of this
experiment. It duly reported that identity never dissents anywhere.

That is rule 10's corollary — _anything that suppresses variance also suppresses
the evidence that the variance matters_ — and it was caught by asking the gate's
own question of the reconstruction: **does it reproduce the shipped run?** It did
not. The shipped runs place 6 / 0 / 1 / 2 statements by `preempt` and 2 / 0 / 0 / 0
by `fill`.

Rebuilt reading the real map off `prior-match-map.json` (`058/trail-dump.ts`), the
reconstruction now reproduces the shipped run's placement **exactly, all ten tiers,
on both hops where a `--diagnostics` dump exists to check against**:

```
85→86   recon {hash:10954, name:7551, preempt:6, ordinal:477, novote:692, allsame:206, anchor:32, anchorPreempt:20, conflict:26, fill:2}
        ship  {hash:10954, name:7551, preempt:6, ordinal:477, novote:692, allsame:206, anchor:32, anchorPreempt:20, conflict:26, fill:2}
215→216 recon {hash:21223, name:12560, ordinal:928, novote:1019, conflict:24, allsame:130, anchor:15, anchorPreempt:2, preempt:2}
        ship  {hash:21223, name:12560, ordinal:928, novote:1019, conflict:24, allsame:130, anchor:15, anchorPreempt:2, preempt:2}
```

**And enabling the tier changed the answer.** With it live, the identity tier
_does_ contradict the fingerprint — once in eight hops — and it is wrong. See
Task 1.

### What the correspondence actually is at the rename→split boundary

The brief's premise is that the matcher resolves ~97% of bindings and that this is
compressed to a 4-entry map. Both halves are true; the conclusion drawn from them
is not.

| hop     | identifiers settled by `binding-cascade` | entries reaching the split (`matchMap`) |
| ------- | ---------------------------------------: | --------------------------------------: |
| 85→86   |                               **13,156** |                                  **11** |
| 118→119 |                               **15,202** |                  **0** (no map written) |
| 197→198 |                               **20,362** |                                   **2** |
| 215→216 |                               **23,097** |                                   **4** |

The compression is not lossy in the way the brief assumes. **`binding-cascade`
applies the PRIOR name to the fresh binding** — that is what the strategy does —
so for all ~23,000 of them `finalName === priorName`, and `buildPriorMatchMap`
skips them because there is nothing to carry. A widened map would be the
**identity function** on those names, and `identityTier` would then compute

```ts
priorNames.get(priorMatchMap.get(name)) == priorNames.get(name);
```

which is, term for term, what `voteFor` already computes for the `all-same` vote.
`identityTier` requires the same unanimity `allSameVote` requires, over a
**subset** of the names (`declaredNames` minus everything that is not a matched
module binding). So a widened carry gives **strictly less reach and identical
content** to a tier that already runs. The brief pre-registered this ("do not
spend a day widening a map that adds no information"); it is now established by
code identity rather than by argument, and it means **(B)'s only lever is rank**.

Which is what the (B) counterfactual measures.

## Task 1 — read every disagreement. Thirteen is the whole population, not a sample

The population where a promoted identity/name tier would change any placement is
**hash-placed statements some other tier contradicts**. Across eight hops that is
**13 statements** — so all 13 were read, with both competing texts side by side
(`read-disagreements.ts`, generalising `057/hash-collision-probe.ts`).

| class                                       |     n | who is right | why, read not deduced                                                                                                                                                       |
| ------------------------------------------- | ----: | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| verbatim string constant, `ordinal` dissent |     5 | **HASH**     | fresh and prior texts are byte-identical and the prior is in the file the hash chose; the dissent is positional                                                             |
| minted lazy-init block, `name` dissent      |     4 | **HASH**     | prior twin is in the same file with the same call set permuted and the same object literal; the name is a re-minted counter (exp042's failure mode)                         |
| **zero-initializer declaration**            | **4** | **NAME**     | the prior statement the hash matched **shares not one declared name** — MCP client internals matched against env-var constants, git command sets against host-settings keys |

**9 of 13: promoting the dissenter would move CORRECT code. 4 of 13: the hash
tier is wrong.** The pre-registered veto — "more than 2 of 20 read disagreements
showing the matcher placing correct code wrongly closes (B)" — is met nine times
over, at a rate of 9 in 13.

And the split is clean: **the zero-initializer predicate separates the population
perfectly — 4 of 4 of the wrong placements, 0 of 9 of the right ones.**

### The one time the binding-identity tier itself dissents, it is wrong — and it is a parameter

On walk 213→214, `preempt` contradicts the hash tier on four consecutive
statements:

```
[11596] function getPluginCacheDataPath(dataKey)      hash: color-validator/hook/plugin-cache.js
[11597] function createPluginCacheDirectory(dataKey)  preempt: http-client/namespace-logger/query-params-parser.js
[11598] async function getPluginCacheSize(dataKey)
[11599] async function deletePluginDataDir(dataKey)
```

The whole of walk 214's `matchMap` is five entries:

```
dataKey     -> $key                     priorHome: http-client/namespace-logger/query-params-parser.js
shouldExit  -> __familyPermuteSwap75$   priorHome: renderer/flex-layout/style-setter.js
getStateVar -> __familyPermuteSwap134$  priorHome: command-runner/session-management/mode-info.js
jsxVal      -> __familyPermuteSwap47$   priorHome: list/usage-dialog/purchase-confirmation.js
React13     -> __familyPermuteSwap46$   priorHome: separator/show-history-search-picker/repl-bridge.js
```

**The identity evidence is a function parameter renamed from `$key`, plus four
exp048 family-permute placeholders.** `identityTier` reads `declaredNames`, which
includes parameters, and a matched parameter with exactly one prior home looks
like unanimous identity. This is the same defect the `allsame` tier's own
docstring records for the name vote ("a parameter named `inputData`, 39th of 53
prior homes, outvoted the statement's own two correct votes"), except the identity
tier has no counterpart guard because a _matched_ parameter looks stronger than a
voted one.

Promoting it costs **+154 git lines and creates all 154 of them**, measured.

## Task 0b — how the ceiling was built, and the control that makes it a bound

Both candidates are "the hash tier does not get to claim this statement", and the
hash tier's only input is `prior.hashes`. So the counterfactual replaces those
hashes with unmatchable tokens **in a copy of the ledger** and runs the production
`stableSplitFromCode` + `emitRunnableCjs` unmodified (`ceiling-ab.ts`). No
pipeline code exists yet, which is what Task 0b requires.

- `emitHashes` — what `alignEmissionOrder` reads — is a separate array and is left
  alone, so the perturbation reaches placement and nothing else.
- **Collateral is counted, not assumed: 0 on every hop.** No statement outside the
  refusal set ever lost its hash placement to a shared hash.
- The construction is deterministic: every hop was run twice and reproduced to the
  line.

**The control: does the reconstruction reproduce the shipped tree?** Classified
file by file on 215→216 — **1,031 of 1,497 files byte-identical, 459 differing
only by the vendor re-link, 7 only by the `using` desugar**. Those are exactly the
two post-tree passes `finishSplitOutput` runs and 054 was starved by; they are
placement-independent and appear identically in both legs, so they cancel in the
delta. Independent confirmation that they are a constant offset: on the two calm
walk hops the reconstruction's churn minus its own control gap gives **1,361 and
1,537**, against 056's published **1,391 and 1,567** — the same 30-line residue
both times.

**What this construction cannot see, stated with the number:** a statement that
moves takes its vendor `require` header with it, and the re-link is not run here,
so each move is under-charged by roughly the number of vendor libraries it
references. That makes the reported (A) figure a slight **under**-estimate, which
is the safe direction for a threshold it already clears.

## Rule 6 — is (A) whack-a-mole? Measured, per statement

Rule 6 says refusing the fingerprint on one class does not remove a mis-placement,
it moves it. `rule-a-moves.ts` runs the splitter twice and follows every statement
the rule takes off the hash tier:

| hop      | taken off `hash` | landed in the SAME file anyway | re-placed |
| -------- | ---------------: | -----------------------------: | --------: |
| 85→86    |           **19** |                         **19** |     **0** |
| 215→216  |           **17** |                         **16** |     **1** |
| w212→213 |           **14** |                         **12** |     **2** |
| w215→216 |           **17** |                         **16** |     **1** |

**63 of 67 land in the file the hash tier had already chosen** — where the hash is
right, the rule is inert, and it is inert _demonstrably_, not by assumption. All
**4** that move land **exactly where every one of their declared names lived in
the prior release**:

```
[33461] hash: storage/error-messages/auth-manager.js   -> floor/cli-interaction/task-serializer.js   (all 32 names)
[18743] hash: formatter/resource-cleanup/telemetry-config.js -> command-runner/git-operations/path-validation.js (all 17)
[21030] hash: command-executor/http-client/merged-config.js  -> skill/image-validator/image-check.js  (all 32)
[33461] hash: command-executor/http-client/merged-config.js  -> survey/status/left-arrow-handler.js   (all 32)
```

**0 lines created on any hop** is the same fact from the other side: the
mis-placement is removed, not relocated.

### Why the class is small, and why that is the point

Only **16–22 zero-initializer declarations per hop are hash-placed at all** (out of
2,476–3,627 such statements), and **every one of them has 8 or more declarators**.
A one- or two-declarator `var a, b;` masks to a shape thousands of statements
share, so the equal-count gate stops it long before this rule would. The class the
rule touches is precisely the narrow band where a declarator count is rare enough
to look unique and carries no other content — which is the collision 057 read.

The hash `8a7597db519cfa8d` — "a `var` with 32 empty declarators" — collides on
**three of the eight hops measured**, and on the walk it compounds: 212→213 puts
the MCP declaration list into `merged-config.js`, and 215→216's collision then
inherits `merged-config.js` from it.

## Task 2 — what was built, and the prediction it was gated against

`carriesNoContent` in `stable-split.ts`: the `hash` tier abstains on a
`VariableDeclaration` with at least one declarator and **no** initializers, so the
statement falls through to the name/identity evidence. Behind
`HUMANIFY_NO_EMPTY_DECL_HASH_GUARD=1`. The trail records a new `shapeless`
`hashMiss`, so the refusal explains itself where the collision was first
diagnosed. Written red-first: the failing test reproduces the measured
misplacement — same declarator count, disjoint names, one occurrence per side —
and landed the group in the collision's file before the change existed.

**The prediction, written before the gate ran** (rule 11 — a cheap
mechanism-bound is what tells you whether a delta is even the right order of
magnitude to be yours): −1,025 on gate 215→216, 0 on the other three; −449 / 0 /
0 / −1,028 on the walk; 0 created anywhere; 67 statements re-tiered, 4 re-placed.

**The shipped code reproduces the pre-code ceiling exactly, 8 hops of 8** — the
harness's counterfactual and the production splitter agree to the line (054's
discipline).

## The gate

### Draw-pinned A/B, four pairs, judged per hop

| hop     | GIT CHURN OFF |     ON |  **delta** | predicted | bundles ON↔OFF | cache written OFF |
| ------- | ------------: | -----: | ---------: | --------: | --------------- | ----------------: |
| 85→86   |        40,279 | 40,279 |      **0** |         0 | identical       |             **0** |
| 118→119 |        38,815 | 38,815 |      **0** |         0 | identical       |             **0** |
| 197→198 |        42,160 | 42,160 |      **0** |         0 | identical       |             **0** |
| 215→216 |        25,896 | 24,747 | **−1,149** |    −1,025 | identical       |             **0** |

`realLn`, `novel`, `vendor.churnLines`, `vendor.noise` are **exactly 0 on every
hop**. Boot gate green on all **8** trees, both halves (`--version` and a live
`-p "say exactly: boot-ok"`; `bun` 1.3.14 confirmed resolvable).

**The mechanism trail, per leg** — a hop whose count is 0 cannot have had its KPIs
moved by this change, however they read:

| leg                      | 2.1.86 | 2.1.119 | 2.1.198 | 2.1.216 |
| ------------------------ | -----: | ------: | ------: | ------: |
| ON — statements refused  |  2,476 |   2,784 |   3,352 |   3,627 |
| OFF — statements refused |  **0** |   **0** |   **0** |   **0** |

**Isolation was not free, and the harness did not catch it.** The first gate run
had leg OFF write 16 and 11 cache entries on 118→119 and 197→198, whose bundles
then differed by **204 and 44 lines** — and the report printed a confident
**−144** delta for a hop whose mechanism-derived prediction was 0. Re-running both
legs against the now-warm cache brought both to 0 written, byte-identical bundles,
and a delta of **exactly 0**. That is rule 10's corollary landing on the gate
itself: the write count is a PROXY for isolation, and the proxy passed while the
thing failed. `pinned-ab.sh` now compares the bundles and says so.

### Self-hop, draw-pinned, tree-level

| leg | bundle diff | move hunks | `src/` tree diff | cache written |
| --- | ----------: | ---------: | ---------------: | ------------: |
| OFF |         148 |          0 |               40 |             7 |
| ON  |     **148** |      **0** |           **40** |         **0** |

Identical, and predicted: on a self-hop the prior tree IS the fresh tree, so every
empty declaration's hash matches its own prior self, the name evidence agrees and
the refusal costs nothing. The 148/40 residue is the control's.

### The four-hop walk — the binding check, because this class compounds

Cold walk seeded from `2.1.212`, each hop taking the previous hop's own output as
its prior. Boot **ok on all four**.

| hop     | baseline `treeLn` (056) | with the change |
| ------- | ----------------------: | --------------: |
| 212→213 |                  30,886 |          30,507 |
| 213→214 |                   1,391 |           1,381 |
| 214→215 |                   1,567 |           1,560 |
| 215→216 |                  25,501 |          23,419 |

**Those differences are NOT claimed as the effect.** That is a cross-run cold
comparison and the per-hop draw band is **±2,800** — wider than three of the four
gaps (rule 11). The effect is measured draw-free instead, by running the real
splitter and emitter over the **walk's own bundles** with the guard on and off:

| hop       |   removed | created |
| --------- | --------: | ------: |
| 212→213   |   **450** |   **0** |
| 213→214   |     **0** |   **0** |
| 214→215   |     **0** |   **0** |
| 215→216   | **1,034** |   **0** |
| **total** | **1,484** |   **0** |

**Predicted −1,477 before a line of code existed; measured −1,484. Seven lines
apart, on a different walk, with different draws.**

And the compounding the walk exists to test is the mechanism itself: in the
baseline trees the collision fires on 212→213 AND again on 215→216, because the
wrong home one hop chooses is what the next hop inherits. The refusal breaks that
chain at both ends.

### The one failing criterion, reported as a failure

`reloc(st)` goes **0 → 1** on 215→216. Every other hop is 0.

`analyze.ts` computes it by pairing a fresh statement to a prior one when their
`statementHash` occurs **exactly once on each side** — the identical inference the
placement hash tier makes, with no content check. On the statement class this
experiment is about, the KPI therefore **re-computes the premise the change
declines**, and cannot disagree with the tier it is meant to audit.

That is a claim, so `reloc-witness.ts` prints the witness rather than asserting it.
Of **20,811** compared statements it flags exactly one:

```
hash            : 8a7597db519cfa8d   (1 occurrence on each side)
prior file      : src/storage/error-messages/auth-manager.js
fresh file      : src/floor/cli-interaction/task-serializer.js
statement type  : VariableDeclaration  ZERO-INITIALIZER DECLARATION
declared names  : 32 fresh / 32 prior
names in COMMON : 0   <-- disjoint: the hash paired unrelated statements
```

It is the target statement, it shares **zero** declared names with the statement
its hash paired it to, and both texts were read by hand in Task 1. The OFF leg
reads 0 for the same 20,811 comparisons.

This is measurement-pitfalls **rule 7 one level down**. Rule 7 is about the
NAME-keyed `reloc` rising when you place things correctly; `analyze.ts`'s own
docstring recommends `relocatedStatements` as the trustworthy alternative. It is
order-independent, which is what rule 7 was about — but it is still hash-keyed,
and a hash can only be wrong about what it serializes (rule 8's corollary).
**A KPI that shares a premise with the code under test cannot audit it.**

### What the gate cannot see, stated with the number

1. **Draw-dependent interactions.** With prompts pinned, a placement that would
   have changed what the LLM proposes elsewhere cannot show up. The walk is cold
   and covers this; its per-hop resolution is ±2,800, which is why the walk's
   verdict is taken from the draw-free re-measurement and not from its own
   per-hop churn.
2. **The three zero hops are a prediction landing, not an absence.** The trail
   shows the refusal firing 2,476–3,352 times on each of them and moving nothing —
   the "inert where the fingerprint was right" property, at gate scale.

## Claims in this directory's own brief that did not survive

- **"The matcher has already resolved which prior binding each fresh binding is,
  for ~97% of them … that correspondence is then compressed into a `Map` of 4
  entries."** The compression is not the loss. ~23,000 of those resolutions
  _shipped the prior name_, so carrying them is the identity function and
  `identityTier` degenerates into the `all-same` vote over a subset of the names it
  already reads. **(B) has no information to promote — only rank.**
- **"Its singleton-rejection guard fired 0 times out of 37,966 accepts — either
  excellent precision or a guard that doesn't test its own claim."** Neither, quite:
  it is a guard two of whose three tests **cannot** fail, on a cascade it does not
  reach at all. The number is the function cascade's; the binding cascade has no
  singleton guard.
- **"(B) subsumes (A)."** It does not. On the gate (B) misses nothing (A) catches
  but adds 61 created lines; on the walk it adds 122 removed and 154 created. The
  two sets overlap on the empty declarations and diverge on everything else, and
  the divergence is (B) moving correct code.
- **"Ceiling on 215→216 is ~962 lines and it fires once on that hop."** Fires once,
  and the git-capped figure is **1,025** — 057's 962 was measured through the alias
  rewrite alone and did not include the declaration's own move.
- **"The two placement tiers that consume [the matchMap] sit BELOW the
  fingerprint"** — true, and the one hop where that ordering matters is the one
  where the fingerprint is right and the identity tier would have scattered four
  functions on the evidence of a parameter.
- **The brief's caution that raw AST position is out of scope** — untested here and
  left alone; nothing in this measurement argues with it.

## Instruments added

| file                       | what it does                                                                                                     |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `singleton-guard-probe.ts` | live check that the singleton guard can fire; at-scale test of whether its signals can disagree inside a bucket  |
| `singleton-census.ts`      | replays the singleton branch over a real pair, per cascade — how often the guard was asked anything              |
| `trail-dump.ts`            | placement trail with EVERY prior-carried tier live; supersedes `057/trail-check.ts`, whose carry disables two    |
| `disagree.ts`              | the disagreement population and (A)'s blast radius, per hop, off a trail dump                                    |
| `read-disagreements.ts`    | prints both competing statement texts plus `nameToFiles`, for the whole population rather than one name          |
| `ceiling-ab.ts`            | git-capped counterfactual for (A) and (B) via ledger-hash refusal; real splitter, real emitter, fidelity control |
| `rule-a-moves.ts`          | per-statement account of what (A) re-tiers and where it lands — the rule 6 answer                                |
