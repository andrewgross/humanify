# Experiment 014 — driving cross-version rename-noise to zero

> **Progress:** noise hunks 22,998 → 10,128 (round 1, slot-keyed transfer,
> `SLOT-KEYED-RESULTS.md`) → 8,745 (round 2, bucket cracking + closure
> votes + rename retry, `ROUND2-RESULTS.md`) → 6,206 (round 3, stable
> factory identifiers at the unpacker, `ROUND3-RESULTS.md`) — **−73%
> cumulative** on branch `exp014-slot-keyed-transfer`. Fresh-pool matcher
> diagnostic: `fresh-pool-overlap.ts`. Next levers: wrapper-scope class
> visibility, megafunction truncation, close-match LLM naming stability.

**Goal:** make the `diff` between two adjacently-humanified bundle versions
(e.g. Claude Code v119 → v120) contain **only genuine source changes** — zero
hunks that differ purely because the same binding got a different name in the
two runs.

This doc is self-contained: motivation with measured data, a root-cause
taxonomy (with a working diagnostic), prioritized approaches, and exact
replication steps. Pick it up cold and run.

---

## Motivating results (Phase 6, commit `1d71c4f`)

Full run in `experiments/013-bun-cjs-classification/PHASE6-RESULTS.md`. The
v119→v120 diff of the humanified `runtime.js`:

- **143,106 lines / 24,999 change hunks.**
- **92.0% is rename-noise** — 22,998 hunks / ~76.8K lines are the _same code_
  with _different identifier names_.
- Only **2,001 hunks (~17K lines) are genuine change** — the version bump,
  build metadata, real logic edits, feature additions.

So ~92% of what a reader wades through to see "what changed v119→v120" is
noise we generated. The cross-version-diff goal (a diff that shows only real
source changes) is ~8% of the way there; this experiment closes the rest.

Correctness is **not** the concern here — the rename-only structural invariant
(`checkStructuralInvariant`, `src/output-validation.ts`) proved at full-bundle
scale that humanify changes nothing but binding names. This is purely about
making those name choices _stable across versions_.

---

## Root-cause taxonomy (measured)

Run `attribute-noise.py` on the diff (see Replication). It recovers every
`old → new` rename a noise hunk encodes (the two sides are structurally
identical, so their identifier streams align 1:1) and buckets each by what it
reveals. On the Phase 6 diff:

```
rename-noise hunks:              22,998
distinct renamed bindings:       12,299   (unique old->new pairs)
total rename occurrences:        47,761   (~diff lines)

root-cause buckets (by occurrences | distinct bindings):
  transfer-gap         25,839 occ (54.1%) |  7,535 bindings
  asymmetric           14,572 occ (30.5%) |  3,539 bindings
  minifier-reroll       7,350 occ (15.4%) |  1,225 bindings
```

### 1. transfer-gap — 54%, ~7,500 bindings (the main event)

Both names are **descriptive but different**: the LLM named the same binding
two different ways across the two runs, and no transfer reused one.
Example (423 diff lines from a single binding):

```
serializeWithErrorHandling  ->  stringifyWithTemplate
```

The binding is structurally identical in both versions; v119's fresh leg
called it `serializeWithErrorHandling`, v120's incremental leg re-named it
`stringifyWithTemplate` instead of reusing the prior name. Every one of its
423 references is a noise hunk.

**Cause:** the `--prior-version` transfer didn't cover this binding —
because its function wasn't matched (fingerprint drift, or it fell to the
fresh-LLM pool), or it was a module binding dropped by the two-vote floor /
phantom gate, or a close-match remainder the LLM re-named. Whatever the path,
the binding was **re-named freshly** in v120 and the LLM is not stable across
the two prompts (v119 has no prior context; v120 does).

### 2. asymmetric — 30%, ~3,500 bindings

One side is descriptive, the other minified-looking: **one run named the
binding, the other left it minified.** The dominant pattern is
error/catch bindings:

```
errorVal        -> K       (and -> q, -> _, -> T, -> O ...)
formatErrorInfo -> T
validationRule  -> O
```

`errorVal` was v119's name for many distinct catch-clause bindings; in v120
those stayed minified (`K`, `q`, `_`, …). **Leading hypothesis to verify:** a
function settled by exact transfer is marked done and skips the LLM pass — and
with it the _shadowed/catch-binding second pass_ (`collectShadowedBlockBindings`
in `src/rename/processor.ts`). So a transferred function reuses names for its
own scope bindings but leaves catch-clause / shadowed block bindings minified,
while the fresh leg named them. Confirm by checking whether transferred
functions' catch bindings are in the placeholder mapping (`translatePriorNames`,
`src/prior-version/prior-version.ts`) or handled anywhere post-transfer.

### 3. minifier-reroll — 15%, ~1,200 bindings

Both sides minified (`QH → dH`, `G → R`, `$_ → w_`): **neither run named the
binding**, and Bun re-minted the token between builds. These are skip-listed /
un-nameable bindings (single-char, `$`-prefixed, helper names). The single
biggest noise contributor overall is here — `QH → dH`, 474 diff lines.

---

## The path to zero

A rename-noise hunk exists iff a **structurally-stable binding got a
different name in the two runs.** So the target is exact:

> Every binding whose rename-invariant structural context is unchanged
> v119→v120 must receive an **identical** name. Only bindings in genuinely
> new or structurally-changed code may differ — and those are _signal_, not
> noise.

The unifying mechanism is **naming as a deterministic function of structure**,
realized as a persistent `structural-context → name` cache. `--prior-version`
is a partial, match-gated version of this today. Generalizing and completing
it drives each bucket to zero:

- **transfer-gap → close the coverage + determinism gaps** so every matched
  structure reuses its prior name.
- **asymmetric → make naming coverage symmetric** so both legs rename (or
  leave minified) the same bindings.
- **minifier-reroll → add a deterministic naming floor** so structurally-fixed
  slots get the same name in both runs even when the LLM declines them.

### Approaches, in priority order

**A. Complete the transfer (attacks transfer-gap, ~54%).**

1. **Recover safe single-vote binding transfers** (handoff follow-up #1): the
   two-vote floor + phantom gate dropped precision-risky transfers. Recover the
   _safe_ ones — exact-matched voter + prior-unique name — or downgrade to
   `suggestedName` (an LLM hint) rather than dropping. Anchors:
   `getTopVote`/`MIN_MODULE_BINDING_VOTES`, `applyPropagatedModuleBindings` in
   `src/rename/prior-transfer.ts`.
2. **Direct structural-hash → name cache.** Instead of only transferring via the
   match cascade, build a persistent `structuralHash → {placeholderMapping}`
   store from the prior run and look up EVERY binding's owning function/binding
   by hash, transferring the name even when the function-level matcher didn't
   pair them. A superset of exact-match transfer. Anchors: `computeFingerprint`,
   `buildPlaceholderMapping` (`src/analysis/structural-hash.ts`);
   `applyMatchedRenames` (`src/rename/prior-transfer.ts`).
3. **Raise function match rate** so fewer functions fall to the fresh-LLM pool
   (1,670 in v120). Measure how many of those 1,670 actually EXIST in v119
   (a real match miss) vs are new (legitimate). Anchors: `matchFunctions`,
   `fingerprint-index.ts`; hash instability is the usual culprit.

**B. Fix naming-coverage asymmetry (attacks asymmetric, ~30%).**

1. **Verify & fix the transferred-function catch-binding gap** (hypothesis
   above). If confirmed, ensure a transferred function still transfers/derives
   names for its catch-clause and shadowed block bindings.
2. Audit any decision that can differ between a fresh leg and an incremental
   leg for the _same_ binding (skip-list application, shadowed-binding pass,
   collision suffixing).

**C. Deterministic naming floor (attacks minifier-reroll, ~15%).**

1. For bindings no run will name (skip-listed helpers, single-char locals),
   assign a name that is a pure function of structural position (e.g.
   `slot_<structuralHash-prefix>_<ordinal>`) so BOTH runs emit the same token.
   Ugly but stable → zero noise for this bucket. Alternatively, extend LLM
   naming to cover them (costlier). Decide per the quality/noise trade.

**D. LLM naming determinism (reduces fresh-name divergence broadly).**
Even with perfect transfer there is a residual: genuinely-new functions in v120
have no prior name, and if v120's build re-orders/duplicates code, fresh names
diverge from any future version. Explore naming a function purely from its
_own_ structure/context (strip prior-version-specific prompt material) so the
same structure yields the same name run-to-run. This makes the cache in (A.2)
populate consistently and shrinks the floor.

### Is zero actually reachable?

The residual floor is bindings in code that genuinely changed structure
between versions (legitimately re-named) plus true hash collisions. Both are
small and, for changed code, are _signal_. So the practical target is
**rename-noise ≈ 0, with the diff dominated by the ~2K genuine-change hunks.**
Track progress as: `noise_hunks / (noise_hunks + genuine_hunks)` → 0.

---

## Replication

Everything runs off the exp013 harness; no new run infra needed.

### 1. Produce a fresh cross-version diff

```bash
# Needs the local LLM box (http://192.168.1.234:8000/v1, gpt-oss-20b) up.
git worktree add /tmp/humanify-run-014 HEAD
ln -s "$PWD/node_modules" /tmp/humanify-run-014/node_modules
PHASE2_OUT=/tmp/exp014 bash /tmp/humanify-run-014/experiments/013-bun-cjs-classification/run-phase2.sh
# Throughput knobs (HUMANIFY_CONCURRENCY/MODULE_CONCURRENCY/MAX_TOKENS, reasoning low)
# are baked in as overridable defaults. ~13m fresh + ~8m incremental.
```

Artifacts land in `/tmp/exp014/`: `cc-119/runtime.js`, `cc-120/runtime.js`,
`runtime-diff.txt`, `cc-{119,120}-diag.json`.

### 2. Measure the noise

```bash
# Total split (rename-noise vs genuine):
python3 experiments/013-bun-cjs-classification/classify-diff.py /tmp/exp014/runtime-diff.txt
# Root-cause attribution + top contributors:
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp014/runtime-diff.txt 40
```

### 3. A/B a fix

Land a candidate fix on a branch, re-run steps 1–2 into a different
`PHASE2_OUT`, and compare the `NOISE hunks` and per-bucket occurrences. A fix
"works" if it moves noise down **without** raising the genuine-change count
(that would mean it corrupted real diffs) and without any parse / structural-
invariant / internalErrors failure in the run log.

### Success metric

`classify-diff.py`'s `noise share of hunks` (baseline **92.0%**). Drive it
toward 0. Secondary: the three `attribute-noise.py` buckets should each shrink;
watch which approach moves which bucket.

---

## Guardrails (from project memory)

- **Precision over recall.** A _wrong_ transfer (renaming an unrelated binding
  to a prior name) is far worse than a missed one — it produces incorrect code
  a reader trusts. This is exactly why the two-vote floor exists; recover
  single-vote transfers only with independent corroboration.
- **Correctness is already guaranteed** by the structural invariant regardless
  of naming, so experiments can be aggressive on _names_ — the invariant will
  hard-fail the run if a change ever alters more than a name. Use that as the
  safety net.
- Keep `npm run check` green; every name-application path must go through
  `attemptValidatedRename` (never raw `scope.rename` in transfer code).

## Key code anchors

- Transfer + propagation: `src/rename/prior-transfer.ts`
  (`applyMatchedRenames`, `attachCloseMatchContext`, `applyPropagatedModuleBindings`,
  the vote floor).
- Prior-name translation: `src/prior-version/prior-version.ts`
  (`translatePriorNames`, `matchPriorVersion`).
- Matching / fingerprints: `src/analysis/fingerprint-index.ts`,
  `src/analysis/function-fingerprint.ts`, `src/analysis/structural-hash.ts`.
- Binding collection (incl. catch/shadowed): `src/rename/function-bindings.ts`
  (`collectShadowedBlockBindings`), `src/rename/processor.ts`.
- Noise tools: `experiments/013-bun-cjs-classification/classify-diff.py`,
  `experiments/014-rename-noise-elimination/attribute-noise.py`.
