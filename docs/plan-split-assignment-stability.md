# Implementation plan: split-assignment stability (the 22% alias-churn bucket, beyond Lever B)

Written 2026-07-21. Companion to `issue-naming-instability-reconcile.md` and
`plan-naming-noise-levers.md`. Scope: the cross-version diff churn caused when a
**matched module binding is assigned to a different file than its prior home**,
which drags every importer's path-derived `require`-alias with it. This is the
"22% alias churn" bucket that Lever B (the split's dormant binding-identity
tier) reaches only **4.5%** of — it leaves **~95% of matched-binding relocations
untouched** (measured below; 033/README saw the same from the residue angle).
This plan diagnoses _why_ that 95% relocates and ranks fixes.

**Not implemented — investigation + plan only.** All numbers below are from the
deterministic 033 harness (no LLM), reproduced on the reference hop 215→216.

## Where this sits (recap)

- Files do **not** move or rename paths across versions (033 `hash4-ceiling`:
  100% of exporting files map to the same path). Lever #4 (export-set-keyed
  alias re-naming) is therefore dead — nothing to inherit.
- A require-alias is a pure function of the **target file's path**
  (`cjs-emit.ts` `nsCandidates` camelCases the path segments). So the alias
  churns iff a binding's **home file changes** — i.e. the split's
  `assignWithPrior` puts the binding's statement in a different file than last
  release. That home-file change is the entire problem; the alias churn is its
  amplification (`taskSerializer → errorMessagesAuthManager` = ~497 diff lines
  from one relocated binding).
- With a prior ledger present, the split **never runs the fresh clustering**
  (`cluster-assign.ts` / `reference-cluster.ts`); every statement's file is
  decided by `assignWithPrior`'s tiers. So this is a **tier-assignment**
  problem, not a clustering problem. (The 033 note calling it a "clustering
  stability problem" is loose; the mechanism is the name-vote tier — see below.)

## The tiers (today, `src/split/stable-split.ts` `assignWithPrior`)

Per wrapper-body statement, in order:

1. **hashTier** — rename-invariant content hash; inherits the prior file when
   the hash occurs the same count in both releases and every prior occurrence
   was in one file. Name-free, order-free, strongest. All identifier names are
   masked (`statement-hash.ts`), so `var x = null` collides with every sibling
   of its shape.
2. **name-vote tier** — each declared name votes for its prior file
   (`nameToFiles[name]`): all prior occurrences in one file → that file
   (`all-same`); equal occurrence counts → kth decl inherits kth file
   (`ordinal`); else abstain. `votes.size === 1` wins.
3. **identityTier (Lever B, dormant)** — only fires when `votes.size === 0`:
   a matched binding whose prior name has a unanimous home inherits it. Its
   input `priorMatchMap` is **not populated in production** (unified.ts:513).
4. **locality residue** — follow the preceding neighbor.

Precision-over-recall axis is the **file**: a matched statement should never
move; every ambiguity defaults to locality, never a guess.

---

## Evidence — what actually relocates and which tier does it (215→216)

Measured with `experiments/033-naming-noise/diagnose-relocations.ts` (new; a
verbatim replica of `assignWithPrior` with per-statement **tier tracking**,
self-checked to reproduce the real `ledger.order` byte-for-byte — so the tier
attribution is trustworthy). Population: the 3,065 top-level module bindings
that the oracle matcher says were **renamed** 215→216 and exist in both ledgers.

**2,781 of 3,065 (90.7%) relocate.** By the tier that placed them:

| deciding tier     | relocations | share     | Lever B reach?           |
| ----------------- | ----------- | --------- | ------------------------ |
| **name-all-same** | **2,185**   | **78.6%** | **no** (votes.size===1)  |
| hash              | 302         | 10.9%     | no (hash wins first)     |
| residue-novote    | 125         | 4.5%      | **yes** (votes.size===0) |
| residue-conflict  | 93          | 3.3%      | no (votes.size>1)        |
| name-ordinal      | 76          | 2.7%      | no (votes.size===1)      |

**Lever B's ceiling on relocations is 124/2,781 = 4.5%** — the harness
reproduces the 033 result exactly (124 net, B's unique+unanimous gate holds, 0
regressions). B, as gated, can only ever reach the `residue-novote` sliver.

### The mechanism: name-vote **collisions** on non-unique minted names

**2,164 of the 2,185 name-all-same relocations (99%) are confirmed name
collisions**: the binding was renamed _to_ a name that **already existed
(unanimously) in the prior ledger as a different binding**, so the all-same
tier confidently votes for that other binding's file. Concrete cases from the
run:

```
initializeModuleRef  (was __i)          exec-path-resolver.js  ->  config-loader.js
noop4                (was dummy)         http-proxy.js          ->  silent-handler.js
emptyFn              (was processDataVal) hash-algorithms.js    ->  discriminated-union.js
noopCallback         (was processTaskFunc) request-part.js     ->  header-query.js
```

The renamed-_to_ names are overwhelmingly **generic/minted** (the `noop*`,
`empty*`, `initializeModule*`, `*Val`, `__x` families the LLM and the naming
floor assign to un-nameable stubs and placeholders). Those names are **not
unique**, so they collide:

- 215 ledger, by the split's own `isRejectedStem` predicate: **6,425 / 54,466
  names (12%) are generic/minted-shaped**, and **6,340 (99%) are declared in
  exactly one file** — i.e. ~6,340 "collision-magnet" names. Any 216 binding
  renamed/minted to one of these gets a confident, wrong, all-same vote to the
  magnet's file. This is **structural across every hop** (priors
  185/202/207/213/215 all measure 11–12% generic, 99% single-file magnets).

This is the **same root as the naming-instability issue's "noise class 2"**
(placeholder-mint churn): non-unique minted names. There it re-mints synonyms;
here it teleports files. One cause, two symptoms.

### The other tiers, briefly

- **hash (10.9%)** — content-changed bindings whose _new_ masked hash collides
  unanimously with a cluster of generic statements in another file (e.g.
  `logHandler`(was `noOpAsync`), and a clean `prefix`↔`messageContent` file
  swap). Real but secondary; the hash tier is name-free so it _correctly_ pins
  the renamed-but-content-identical bindings — these 302 are only the
  content-changed generic-body collisions.
- **residue-conflict (3.3%)** — a statement declaring multiple names whose votes
  disagree → locality. B can't help (only fires `votes.size === 0`).
- **name-ordinal (2.7%)** — the kth-occurrence rule picks the wrong file when a
  name's declaration count shifts between releases.

### Control: same-named bindings are already stable

Bindings that keep their name across versions are **99.5% file-stable**
(246/54,107 move, mostly Bun runtime temporaries `__i`/`__m`/`__d` whose name is
spread across dozens of files so all-same abstains). **Relocation is a
rename-driven phenomenon** — which is why the matcher's identity map is the
right signal and why Lever A (fewer renames) indirectly helps here too.

---

## Ranked approaches

The prize is the **78.6% name-collision bucket**, which Lever B (as gated)
cannot touch. Ranked by leverage ÷ effort, precision-first.

### C. Wire Lever B in production — the prerequisite, modest on its own

**Mechanism.** Populate `priorMatchMap` (new-final-name → prior-name) from the
rename matcher and thread it to `stableSplitFromCode`. The matcher already
produces every entry needed: `moduleBindingRenames`, each exact-matched
function's `state.transfers`, and `closeMatchContext[*].nameTransfers`
(`prior-version.ts`). The seam is documented in `unified.ts:513-523`: capture
the map **before `releaseSplitSourceState`** frees the rename AST.

**Where.** `commands/unified.ts` (expose the map on `RenamePluginResult`, pass
as `priorMatchMap`); no change to `assignWithPrior` — the tier already exists
and gates hard.

**Ceiling.** As-gated (fires only on `votes.size === 0`): **124 bindings /
~3,056 lines** on 215→216 (033 b-ceiling; reproduced here). The production map
(minified↔final) matches _less_ than the final↔final oracle, so this is an
upper bound.

**Risk.** LOW. Unique + unanimous + already role-checked; abstains to locality
on any ambiguity. A wrong inherit can't break the concat invariant, only
mis-file — but the gate blocks that.

**This is the foundation for A.** On its own it is the small, safe win.

### A. Promote the identity tier to preempt collision votes (the big lever)

**Mechanism.** The 78.6% bucket is the name-vote tier being _confidently wrong_
because a minted name collided. The matcher already knows the binding's true
prior identity. So: when a statement declares a matched binding whose prior name
has a **unanimous home H**, and the incumbent tier (name-vote, or hash) would
place it **elsewhere**, prefer **H** — i.e. let a **role-corroborated** identity
vote _preempt_ the name-vote instead of only filling the `votes.size === 0`
gap.

**Where.** `assignWithPrior` loop, reordering the tiers to:
`hash → identity-preempt (gated) → name-vote → identity-fill → locality`.
The gate must be strict (this is the precision-critical change):

1. the binding is in `priorMatchMap` (a confident cross-version match), AND
2. its prior name's home is **unanimous** (single file), AND
3. **role agreement** — reuse `bindingRolesAgree` (`binding-role.ts`: content
   hash-equal or slot-blind shingle overlap ≥ 0.5, callee-identity veto). This
   is the same gate fix #1 uses; it needs the binding's role evidence plumbed
   alongside the map, AND
4. **skip generic/minted new-names** (`isRejectedStem`, already in
   `stable-split.ts`) — those are the least-reliable matches and the highest
   mis-pin risk; better to abstain than to guess for a `noop`.

**Ceiling (measured, oracle map, 215→216, deterministic — `diagnose-v2.ts`).**

- Pure ceiling (all matches trusted): relocations **2,781 → 90** (fixed
  **2,691 = 97%**), with **0 away-from-prior regressions** (2,455 statements
  moved, every one toward its prior home).
- Role-gated / non-generic (the shippable subset — skips generic new-names via
  `isRejectedStem`): **2,781 → 619** (fixed **2,162 = 78%**), **1**
  away-from-prior regression.
- Identity-addressable relocations (matched binding with a unanimous prior
  home): **2,719 / 2,781 (98%)** total, **1,963 (71%)** non-generic.
- Concrete diff impact (baseline vs promoted 216 tree, only the tier differs —
  same methodology as the B ceiling): role-gated **717 files / ~18,833 lines**
  moved back to prior home; pure **790 files / ~27,041 lines**. This is the
  direct statement-relocation churn; the require-alias amplification on the
  exported subset rides on top. Compare **Lever B: 145 files / ~3,056 lines.**

**Risk.** MEDIUM-HIGH — this overrides a confident name-vote (and can override
hash). A wrong match pins a statement to a semantically-wrong file, which the
concat invariant will **not** catch. Mitigation is entirely in the gate (1-4);
the generic-name skip (4) deliberately trades recall for precision on exactly
the population where the matcher is weakest. Validate on the quiet hops
(213→214, 214→215): a promoted pin is invisible in its own hop by construction
(new-name = prior-name there), so it can only _remove_ churn — any _new_ quiet-
hop diff means the gate is too loose.

**Relationship to B.** A _is_ B, promoted above the name-vote and populated. It
subsumes C's plumbing (same map) and reuses fix #1's role gate. Do C first
(map + no reorder) to de-risk, then A (reorder + role gate + generic skip).

### B2. Distrust generic/minted names in the name-vote tier — MEASURED, REFUTED

**The idea.** A generic/minted name is not a reliable identity key, so (the
hypothesis went) when a statement's only all-same vote comes from a generic name
(`isRejectedStem`), don't cast it — abstain, so it falls to locality instead of
teleporting to a collision magnet. Cheap, split-local, no matcher, no map.

**Measured (215→216, `diagnose-v3.ts`) — it BACKFIRES.** Demoting every generic
name to _no vote_:

- removed only **14** of the 2,781 renamed-binding relocations, **but**
- **regressed 1,403** previously-stable _same-named_ generic bindings (a kept
  `noop`/`emptyFn` that the all-same vote had correctly pinned to its own prior
  home now drifts to locality),
- **net −1,389** — strongly negative.

**Why.** The generic all-same vote does far more good than harm: it correctly
pins **6,326** same-named generic bindings. The churn comes only from the subset
where a generic name is applied to a _differently-identified_ binding (a rename/
mint collision). Demoting can't tell a **keep** from a **collision** without an
identity signal, so it throws out 6,326 good votes and still fixes only 14
relocations (the rest just drift to a different wrong file via locality).
The keep-vs-collision distinction **requires the matcher's map** — i.e. it
_is_ approach A. **Keep B2 as a documented dead-end, like Lever #4** — the
measurement kills the cheap shortcut and proves A's identity gate is necessary,
not optional.

### D. Root fix — make minted names unique (upstream, shared with the naming plan)

**Mechanism.** The collision surface is created by the naming floor / LLM
minting **non-unique** generic names (~6,340 magnets). If minted names were
unique per tree (or generic names were barred from being identity keys
end-to-end), the collision bucket collapses at the source — and the naming-
instability issue's "noise class 2" shrinks with it (one fix, both symptoms).

**Where.** Rename layer — `class-id-floor.ts`, `coverage-sweep.ts`,
`sweep-step.ts` (mint counter not prior-seeded / not globally unique). Out of
the split's scope; the split inherits the benefit for free.

**Ceiling.** Large but indirect and hard to bound without building it; it
attacks the _creation_ of magnets rather than the assignment. Cross-reference
`plan-naming-noise-levers.md` A3 (decoration/ordinal bugs) and the issue doc's
class 2.

**Risk.** MEDIUM — changes naming semantics; unique minted names are more
churny _as names_ unless prior-seeded. Coordinate with Lever A.

### E. Usage-context identity for un-nameable placeholders (hardest, the residue floor)

**Mechanism.** The `residue-novote` (4.5%) plus the generic bindings the matcher
**cannot** match (so they never enter `priorMatchMap`, so A/C can't see them)
need a **non-name, non-content** identity: position in a stable enclosing
structure (the tool-registry array), the neighbor set, or the value later
assigned into a forward-declared `= null` slot. This is the issue doc's
approach 2, transposed to the file axis.

**Where.** A new pre-tier feeding `assignWithPrior` (a positional/neighbor
fingerprint per statement, matched across releases).

**Ceiling.** Structurally the only thing that reaches the truly-ambiguous slots,
but unbounded here and a new mechanism. Defer until A/C are measured on the
full benchmark set — A already covers 78% of relocations, so E's residual is
small (the ~29% of relocations whose new name is generic AND the production
matcher can't map).

**Risk.** HIGH — new identity signal; easy to mis-fire. Precision gate essential.

---

## Suggested sequence

1. **C (wire B)** — small, safe, unlocks the map. Land + measure the 124/~3k;
   it is the plumbing A also needs, so it de-risks A's map before the reorder.
2. **A (promote identity, role-gated + generic-skip)** — the 78.6% prize
   (measured ceiling: 2,162 relocations / ~18,833 lines fixed, ≤1 regression).
   The heaviest change, but the map (C) and role gate (fix #1) already exist;
   the gate is the whole ballgame — validate hard on the quiet hops.
3. **D (unique minted names)** — coordinate with the naming plan; shrinks the
   collision surface at the source (and the naming issue's class 2 with it).
4. **E (usage-context identity)** — only if A/C leave a material residue (the
   generic placeholders the production matcher can't put in the map).

**Not on the path: B2** (distrust generic votes) — measured net −1,389, a
dead-end retained above for the record.

## Tier ordering — concrete before/after

```
today:     hash  →  name-vote(size==1)  →  identity(size==0)  →  locality
proposed:  hash  →  identity-preempt(gated)  →  name-vote(size==1)
                 →  identity-fill(size==0)  →  locality
```

`identity-preempt` fires only under the strict gate (matched + unanimous home +
role-agrees + non-generic new-name) **and only when it disagrees with the
incumbent** (so it is a no-op on the 99%+ already-correct assignments — it can
only _reduce_ relocation, never introduce a move on a binding the name-vote got
right). `hash` stays first because it is name-free and already correct for
content-identical renames; A only overrides hash in the measured collision cases
(guarded by the same gate). Note the two identity steps are the SAME tier at two
priorities: a **preempt** step (gated, may override) above name-vote, and the
existing **fill** step (Lever B, `votes.size===0` only) below it. B2 was the
attempt to get the collision fix _without_ the map by weakening name-vote; the
measurement (net −1,389) shows that can't work — the map is required.

## Measurement methodology + invariant

- **Harness (all extend `b-ceiling.ts`, deterministic, no LLM, added by this
  investigation):**
  - `diagnose-relocations.ts` — replicates `assignWithPrior` verbatim with
    per-statement **tier tracking**; self-checks that its assignment reproduces
    the real `ledger.order` byte-for-byte, then attributes every relocation to
    the tier that placed it.
  - `diagnose-v2.ts` — the **promoted-identity ceiling** (pure + role-gated) and
    the away-from-prior regression count; `WRITE_TREES=<dir>` dumps baseline /
    promoSafe / promoAll trees for a direct diff. Caches the oracle map to
    `/tmp/humanify-split-oracle-*.json` to skip the ~10-min matcher on re-runs.
  - `diagnose-v3.ts` — the B2 (distrust-generic-votes) measurement, incl. the
    same-named regression; reuses v2's cached oracle.
- **Primary metric:** matched-binding relocations (home-file changes) and the
  amplified require-alias churn (`alias-classify.py`), before/after, on the
  benchmark hops (215→216 done here; 202→203, 185→186 next) with the quiet
  controls (213→214, 214→215). Other hops not yet run (each is a ~10-min match);
  the collision mechanism is structural (11–12% generic / 99% magnets in every
  prior ledger measured), so they are expected to scale similarly.
- **Invariant:** the split's concat-equivalence (`reconstructBody` /
  `assertConcatEquivalence`) validates statement **bytes**, not file choice — it
  is fail-loud on structural breakage but **blind to a semantically-wrong file
  assignment**. That is exactly the risk of A/E, so the gate (unique +
  unanimous + role-agrees) is the real safety, not the invariant. Treat a
  wrong-but-valid pin as a failure, not a win.
- **Precision check:** for a sample of preempted pins, confirm prior and new
  binding agree on callees/usage (not just shape), and that quiet hops gain **no**
  new diffs.
