# 058 — RESULTS: binding-derived placement. (B) is REFUTED. (A) survives Task 0/1 and is a build candidate.

> ## STATUS: Task 0a, 0b and 1 COMPLETE. **(B) CLOSED — it is worse than (A), not better.** (A) clears its pre-registered threshold on both four-hop measurements, with **0 lines created on every hop**. Task 2 not started; the user's call.
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

## What (A) would be, if built

Not written. Stated here so the ceiling is on record before any code (rule 11):

- In `PLACEMENT_TIERS`' `hash` tier, abstain when the statement is a
  `VariableDeclaration` with at least one declarator and **no** initializers.
- Behind `HUMANIFY_NO_EMPTY_DECL_HASH_GUARD=1`, the house kill-switch pattern.
- TDD red-first against the real 215→216 misplacement — the fixture is already
  reproducible offline in ~90 s (`rule-a-moves.ts`), no LLM.
- Predicted effect, from the mechanism, before the gate: **−1,025 git lines on
  gate 215→216, 0 on the other three gate hops; −449 / 0 / 0 / −1,028 on the walk;
  0 lines created anywhere; 67 statements re-tiered, 4 re-placed.** A gate that
  disagrees with those numbers is measuring something else.

Both effects sit inside the **±2,800 lines/hop** draw band, so the gate must be
draw-pinned (`054/pinned-ab.sh`), the pinning proven by both legs writing ~0 cache
entries, and `056/walk.sh` run as the final check — placement churn compounds and
this class demonstrably does.

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
