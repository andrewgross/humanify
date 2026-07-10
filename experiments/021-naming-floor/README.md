# Experiment 021 — naming floor: leave NO minted tokens in the output

**Goal of this experiment:** make a humanify run leave **zero minified
(Bun-minted) identifiers** in its output — not by forcing names, but by
closing the specific coverage gaps that let 744 bindings slip past every
naming path, and by making the pipeline's own coverage counters admit it
when something slips. Every gate still defaults to skip; the floor is a
coverage fix, never a force-name pass.

This doc is self-contained. Pick it up cold and run. Read the campaign
context, then the measured population, then the design, then the
runbook. Every code anchor below was re-verified on 2026-07-10 against
branch `exp020-tail-polish` (commit `6d9139d`); re-verify before editing.

---

## The end goal (the whole campaign)

Humanify two adjacent versions of a minified bundle (fixture: Claude
Code v119/v120, Bun single-file binary decompiled to
`.../binary-decompiled/src/entrypoints/index.js`) and make the `diff`
between the two humanified outputs contain **only genuine source
changes** — reviewable by a human like a real git diff.

We measure on the **shared-lineage diff** (see Runbook): each release is
humanified incrementally on the previous output (`--prior-version`), so
both diffed legs inherit one naming lineage — the production scenario.

### Campaign trajectory (lineage-diff noise hunks)

Phase-6 baseline 22,998 → exp014 6,206 → exp015 5,788 → exp016 2,960 →
exp017 2,516 → exp018 1,504 → exp019 1,288 → **exp020 684** (branch
`exp020-tail-polish`, flag-on leg). Noise share 26.7%; genuine-change
hunks have held in the **1,868–1,975** band the entire campaign — that
is the SIGNAL and must never rise. Each round's story is in
`experiments/01{4,5,6,7,8,9}-*/RESULTS.md` and
`experiments/020-tail-polish/RESULTS.md`; read exp016 (lineage protocol)
and exp020 (text-diff reconciliation — the pass whose residue this
experiment starves) at minimum.

---

## Why this change now — the user directive, and why a floor

**User directive (Andrew, 2026-07-10):** never rename TO a minified
name — exp020's downgrade gate already enforces that in the reconcile
pass. The real strategy is upstream: **ensure runs leave no minted
identifiers in the output at all.**

Why a minted leftover is future noise, not a cosmetic blemish:

- **Reroll noise.** Bun re-mints tokens between builds, so a binding
  left minified on both legs diffs as `uq` ↔ `vq` forever — exp020's
  residual reroll floor is 54 occurrences, and the reconcile pass
  rightly refuses to snap minified→minified (pointless).
- **Asymmetric noise / downgrade conflicts.** A binding one leg names
  and the other leaves minified becomes either an asymmetric hunk or a
  downgrade decision the reconcile pass must refuse (27 bindings in
  exp020's skip dump).
- **The lineage carries names forward — and it carries GAPS forward
  too.** This is the load-bearing fact, verified in code: exact-match
  prior transfer replays the prior leg's names, and a pair whose old
  and new names are equal is silently skipped (the
  `oldName === newName` guard at `src/rename/prior-transfer.ts:186`).
  The matched function is then **settled** ("transferred"
  state, `prior-transfer.ts:270–281`), so the LLM pass never visits it
  (`src/rename/plugin.ts:590–593` — the processor derives its done set
  from node state). A param the prior left minified is therefore
  re-settled minified **every release, indefinitely**. Flooring is a
  ONE-TIME cost per binding: once named, the transfer carries the name
  forward and the gap never re-opens.

With the floor in place, exp020's downgrade bucket (27 bindings) and
reroll floor (54 occurrences) starve within one release cycle.

### Why these bindings escape (verified mechanisms, not guesses)

- **Class-expression inner ids** — `BaseError = class uq extends Error {}`:
  the outer binding got named, `uq` did not. The inner id binds
  in the ClassExpression's OWN scope (`binding.path.isClassExpression()`
  is true, kind `local` — probed with Babel directly). No collector
  ever visits that scope: `collectFunctionNameBinding`
  (`src/rename/function-bindings.ts:172–189`) handles only
  FunctionExpression/FunctionDeclaration ids, and a ClassExpression is
  not a `t.Function` at all, so it never becomes an `fnPath`; the
  module-binding path names the OUTER binding only (the inner id never
  appears in Program scope — probed). For class expressions nested
  inside functions, the shadowed-bindings second pass
  (`function-bindings.ts:198`, `Scope` visitor) can reach them, but the
  328 measured escapees are module-level assignments, which have no
  enclosing FunctionNode.
- **Params of already-named functions** —
  `function updateReplBridgeState(H)`: the exact-match transfer + settled-state
  mechanism above. Arity drift (a param the prior version didn't have)
  and inherited gaps (the prior itself never named it) both land here.
- **Whole minified function/class declarations** (`j2_`, `FH3`, one an
  `async function`): matcher misses whose vote propagation
  (`prior-transfer.ts:603–662`) never reached the agreement floor, plus
  by-design skip paths (`settleModuleBindingNode`,
  `prior-transfer.ts:692` — settles a module binding under its ORIGINAL
  name).
- **The bookkeeping cannot see any of this.** Coverage aggregates
  RenameReports (`src/rename/coverage.ts:87–110`) — a binding never
  collected into any report is invisible. The exp020 flag-on run's own
  diagnostics claim near-perfect coverage: functions failed 5, module
  bindings notRenamed 3, identifiers notRenamed 18
  (`/tmp/exp020-chain-on/cc-119-lineage-diag.json`) — while the census
  finds **744** minted leftovers. "notRenamed" must become truthful.

---

## The measured population (census, 2026-07-10)

`census-minted-tokens.ts` (this directory) counts surviving Bun-token
bindings that pass rename-eligibility
(`createIsEligible("bun","bun")`). Run on the exp020 flag-on lineage
leg (`/tmp/exp020-chain-on/cc-119-lineage/runtime.js`; if /tmp is gone
the script regenerates the numbers on any lineage output):

```
eligible minted bindings: 744

  family        count   samples
  classExprId     328   uq@L3809→BaseError  Cq@L3810→HttpError  u3@L3868→AbortError
                        PG@L3875→ConnectionError  nQ@L3886→ConnectionTimeoutError
                        TN@L4487→SSEIterator  uJ6@L4740→PaginatedResponse
                        M2_@L4773  hg@L4782  K7H@L6681→Messages ...
  fnExprId          0
  param           112   H@L3360  _@L8720  H@L8912  P@L8985 ...
  fnDecl           16   j2_@L4634  FH3@L380579  $@L49531 ...
  varOther        288   RP_ConstructorKey@L1395  u4Function@L13429
                        initializeApp_@L31603  OS_MODULE@L44732 ...

overlays: halfNamedSuffix 3, decoratedDescriptive 17
expression inner ids: 328 total, 328 with a derivable non-minted
source name, 276 with zero references
```

Two census facts materially de-risk the design:

- **All 328 class-expression inner ids have a derivable source name**
  (assignment target / declarator id / property key) — the
  deterministic pass can reach 100% of the family, modulo validation
  rejections.
- **276 of 328 have zero references** — self-reference handling is a
  real but small sub-case (52 bindings).

**Known false positives — the census shape is a heuristic and
over-counts.** `ec2MetadataServiceEndpointSelector` and `OS_MODULE`
match the letterhead+digit rule; `LZ77Compressor` (fnDecl family!) and
`is2017Api` / `$context` (param family) are fine names;
`initializeApp_` is a descriptive name wearing a trailing-underscore
collision decoration (wants an undecorate retry, not a fresh name);
`RP_ConstructorKey`-style names are minified-stem+suffix. Treat
varOther ≤288 as an upper bound, and see the sweep-targeting gate below
— **the sweep's predicate must be stricter than the census's**, or the
floor will rename good names (that is noise, the exact thing we are
killing).

Keep two "minified" definitions straight — do NOT unify them:

- `isMinifiedName` (`src/rename/diff-reconcile.ts:674`) is the
  attribute-noise.py–compatible METRIC heuristic (rename-type tiering
  in the reconcile pass). It must stay in lockstep with the Python
  metric.
- The census's `isBunToken` targets Bun's mint shapes (letterhead+digit,
  trailing `_`, `$`) for FLOOR TARGETING. It flags `initializeApp_` and
  `RP_ConstructorKey`, which `isMinifiedName` does not, and that is
  correct for each one's job.

---

## The design (worked out in advance — implement this)

Four workstreams, safest first. Each lands as its own commit(s) with
red/green tests; each rename goes through the validated path; every
gate defaults to skip.

### 0. Truthful bookkeeping (do this FIRST — it is the experiment's meter)

Add an end-of-run census to the pipeline: after all naming passes,
walk every scope once (the census script's `Scopable` + seen-set walk)
and count surviving eligible minted bindings by family; put the counts
in `CoverageSummary` / diagnostics with per-binding skip reasons where
known (frozen, validation-rejected, no-derivation-source,
below-sweep-confidence, census-false-positive-shape). Success for the
whole experiment is judged by this counter, so build it before the
fixes. Unit test with a small fixture containing one escapee of each
family. Anchor: `buildCoverageSummary` (`src/rename/coverage.ts:126`),
`formatCoverageSummary` (:285), wired at `src/rename/plugin.ts:622`.

### 1. Deterministic class/function-expression inner-id derivation (the 328)

No LLM. Derive the inner id's name from, in priority order: assignment
target name, variable declarator id, object property key
(`BaseError = class uq ...` → rename `uq` to `BaseError`, yielding
`BaseError = class BaseError ...`, which is what the original source
almost certainly said). Only when the inner id is Bun-token shaped AND
rename-eligible AND its scope is not eval/with-frozen. Stable across
versions by construction — this starves the reroll bucket at the
source.

**Pipeline placement: AFTER the naming passes, not at graph-build
time.** The derivation source (the outer binding) only has its final
name after prior transfer + `runRenamePass` + library prefix — derive
earlier and you copy a minified name. Slot it in
`createRenamePlugin` after `runLibraryPrefixPass`
(`src/rename/plugin.ts:613`) and before `buildCoverageSummary` (:622) /
`checkStructuralInvariant` (:641) / `generate` (:648), so the invariant
and output validation net the new pass exactly like every other rename.

**CRITICAL, verified this session — the standard validation rejects
this pass by construction.** `getRenameRejection`
(`src/rename/validated-rename.ts:80`) returns `target-visible` when
`scope.parent?.hasBinding(newName)` (:89) — and the derived name is BY
DEFINITION the name of a binding visible from the parent scope (the
assignment target). Calling `attemptValidatedRename` naively floors
zero bindings. Do NOT weaken `getRenameRejection` for everyone;
instead add an intentional-shadow variant IN validated-rename.ts
(reuse `isValidRenameTarget`, `fastRenameBinding` — CLAUDE.md's
unification rule) that replaces the blanket `target-visible` /
`target-free-name`-adjacent checks with a PRECISE capture check:

- reject unless the outer binding whose name we are copying is exactly
  the derivation source (same Binding object as the assignment
  target/declarator/property owner);
- reject if ANY reference to that outer binding sits inside the
  class/function expression subtree (after the rename those references
  would re-resolve to the inner id — same object today, but
  reassignment semantics differ; precision over recall says skip);
- keep every other check: `isValidRenameTarget`, `no-binding`,
  `target-in-scope` on the inner scope itself (a method named
  `BaseError`? skip), `shadows-child`.

The structural invariant is compatible with the intentional shadow:
`checkStructuralInvariant` (`src/output-validation.ts:115`) tracks free
names + binding count + structure; the derived name was bound (not
free), counts don't change. Verified against
`captureSemanticBaseline` (:101).

**Red tests first** (colocated `*.test.ts`, house TDD):

- plain assignment `X = class q {}` → inner id becomes `X`;
- variable declarator `var X = class q {}` → `X`;
- property assignment `obj.Foo = class q {}` / `{ Foo: class q {} }` →
  `Foo`;
- inner scope already binds the target (static method / param named
  `X`) → skip;
- outer `X` referenced INSIDE the class body → skip (the capture case);
- inner id self-referenced in the body (`q.count++`, 52 real cases) →
  rename applied AND the self-reference follows
  (`fastRenameBinding` rewrites referencePaths — test it anyway);
- eval/with-tainted scope → skip (`collectEvalWithTaint`,
  `src/analysis/soundness.ts:54`; the pipeline freezes via
  `markEvalWithTaintPreDone`, `src/rename/plugin.ts:272` — the floor
  pass must consult the same taint, not re-invent it);
- derivation source itself minted (`W1 = class q {}` where `W1`
  survived minified) → skip (the sweep gets both).

### 2. End-of-run coverage sweep (params + whole decls + true var/let leftovers)

After the main naming pass, prior transfer, AND direction 1, collect
every remaining eligible minted binding (the same walk as workstream
0), batch to the LLM with code windows, apply via
`attemptValidatedRename` — the normal path, no shadow variant here.
Reuse the existing machinery: `selectFunctionCode`
(`src/rename/code-window.ts:168`) / `capContextCode` (:68) for windows,
the batch/retry protocol in `src/rename/processor.ts` (see
`processFunctionBatched` call sites at :220/:241 and
`uniquifySameNamedBindings` :255 for same-named leftovers — sweep
batching should group by enclosing function for window quality).

**Targeting gate (this is where precision lives):** the sweep predicate
must be STRICTER than the census heuristic. Concretely: skip names
that carry a ≥3-letter lowercase dictionary-ish run unless they also
end in `_` or start with a 1–2-letter head + digit AND are short
(tune on the census dump — `LZ77Compressor`, `is2017Api`, `$context`,
`OS_MODULE`, `ec2MetadataServiceEndpointSelector` must NOT be swept;
`H`, `j2_`, `FH3`, `u4Function`'s `u4` stem... `u4Function` is a
judgment call — when in doubt, SKIP and log). A skipped true-minified
binding costs one more release of reroll noise; a swept good name
costs immediate genuine-looking noise. Precision over recall.

Settled/frozen state is respected: the sweep must not touch bindings
in eval/with-frozen scopes, and must record per-binding outcomes into
the workstream-0 counters (renamed / rejected-by-validation /
below-confidence / skipped-shape).

Cost estimate: ~400–500 identifiers ≈ 1/30th of the exp020 run's LLM
budget (that run: 1,212 calls / 2.2M tokens for 3,007 fresh
identifiers, from the run diagnostics) — and one-time, because the
lineage carries the names forward (see the `oldName === newName`
mechanism above).

### 3. Decoration retry (small — 17 bindings)

`initializeApp_`, `React_`, `noop_`, `FS_`, ... are collision
decorations that stuck after the blocking name moved on. At sweep
time, first try the UNDECORATED stem through `attemptValidatedRename`
(it re-checks the collision); only if rejected, leave as-is (do not
LLM-rename a decorated descriptive name — the stem is already the
right name). Red test: decorated name with blocker gone → undecorated;
with blocker still present → unchanged.

### 4. Flag and staging

One CLI flag `--naming-floor` next to `--reconcile-prior-diff`
(`src/commands/unified.ts:483`), one plugin option next to
`reconcilePriorDiff` (`src/rename/plugin.ts:118`). Workstreams land as
separate commits inside the flag (bookkeeping first, unflagged — it
observes only). Interplay with the reconcile step
(`maybeReconcilePriorDiff`, `plugin.ts:211/:657`): floor runs
pre-generate (in-AST), reconcile runs post-generate (on text); a
floored binding whose prior name is minified hits reconcile's
downgrade gate and correctly stays floored. No conflict — but add one
integration test for the combined flags.

---

## Safety ladder

1. **Workstream 0** — observation only, zero rename risk. Land first.
2. **Workstream 1** — deterministic, red/green offline, no LLM. The
   intentional-shadow variant is the one novel safety surface; its
   subtree-capture check is the gate that must be red-tested hardest.
3. **Workstream 3** — deterministic retry of names the pipeline itself
   minted. Tiny.
4. **Workstream 2** — LLM naming of leftovers; risk concentrated in
   the targeting predicate (sweeping a good name = self-inflicted
   noise). Dry-run mode first: log would-sweep list, eyeball it against
   the census false positives, then enable.

---

## Runbook

### Checks (before every commit — CLAUDE.md)

```bash
npm run check   # typecheck + prettier + biome (complexity ≤ 15) + unit + fingerprint
```

Red/green TDD is the house rule: failing test first, then the fix.

### Offline development (no LLM needed for 0/1/3)

Workstreams 0, 1, 3 are deterministic — build them with colocated unit
tests, then validate at scale by running the census before/after on an
existing output. The census also works as an offline harness pattern
(imitating `experiments/020-tail-polish/run-reconcile.ts`): parse an
existing lineage output, run the derivation pass, re-census, check the
structural invariant — no bundle run required.

```bash
npx tsx experiments/021-naming-floor/census-minted-tokens.ts \
  /tmp/exp020-chain-on/cc-119-lineage/runtime.js --samples 10
```

### The LLM box (needed for workstream 2 and lineage runs)

- Endpoint `http://192.168.1.234:8000/v1`, model `openai/gpt-oss-20b`,
  `HUMANIFY_API_KEY=local`.
- Check it's up: `curl -s -m 3 http://192.168.1.234:8000/v1/models`.
  If unreachable, do offline dev and ask Andrew to power it on.
- Knobs (baked into the harness, overridable): `HUMANIFY_CONCURRENCY=120`,
  `HUMANIFY_MODULE_CONCURRENCY=40`, `HUMANIFY_MAX_TOKENS=2000`,
  `HUMANIFY_REASONING_EFFORT=low`.

### Produce a lineage leg + measure

```bash
EXTRA_HUMANIFY_FLAGS="--reconcile-prior-diff --naming-floor" \
  CHAIN_OUT=/tmp/exp021-chain \
  bash experiments/016-diff-noise-convergence/run-chain.sh
```

Needs the v120 prior at `/tmp/exp016-r1/cc-120/runtime.js` (present as
of 2026-07-10); if gone, regenerate per exp020's README runbook with
`experiments/013-bun-cjs-classification/run-phase2.sh`.

Measure:

```bash
npx tsx experiments/021-naming-floor/census-minted-tokens.ts \
  /tmp/exp021-chain/cc-119-lineage/runtime.js
python3 experiments/013-bun-cjs-classification/classify-diff.py /tmp/exp021-chain/runtime-diff.txt
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp021-chain/runtime-diff.txt 10
```

### Baseline (exp020 flag-on lineage leg, branch `exp020-tail-polish`)

- lineage noise **684 hunks / 26.7% share / 1,126 occurrences**;
  genuine **1,878**; reroll floor 54 occ; downgrade bucket 27 bindings.
- census **744** minted bindings (classExprId 328 / param 112 /
  fnDecl 16 / varOther 288).
- run cost 1,212 LLM calls / 2.2M tokens.

### Success criteria

- **Census → as close to 0 as the gates allow** on the new lineage leg,
  and — because the census shape has false positives, "0" is not the
  literal bar — **every remaining entry carries a logged skip reason**
  (frozen / validation-rejected / no-derivation-source / shape-FP /
  below-confidence). Coverage counters ACCOUNT for all of them:
  "notRenamed" is truthful.
- Lineage noise **at or below 684 hunks**, genuine inside
  **1,868–1,975**.
- Parse clean, `checkStructuralInvariant` clean, output validation
  clean, zero `internalErrors`.
- **In the NEXT chain hop, reroll + downgrade buckets shrink.** Honest
  expectation-setting: the FIRST floored run may not drop noise much —
  a leftover pair `uq`↔`vq` (reroll) becomes `BaseError`↔`vq`
  (asymmetric) on the same hunk, and where both legs minted the SAME
  token the floor briefly CREATES a hunk. The win lands when both legs
  are floored. Verify with a second hop: humanify v120 with
  `--prior-version` = the floored v119-lineage output and the floor
  flags on (adapt run-phase2.sh's incremental-v120 leg), then diff the
  two floored legs — reroll and downgrade buckets should approach zero.

### Tripwires

- **Genuine hunks rise above ~1,975** → a floor rename mis-mapped or
  re-named something real; find it before celebrating anything.
- **Noise rises more than the one-time conversion wobble (> ~+100
  hunks)** → the sweep is renaming good names (targeting predicate too
  loose) or the derivation is firing where it shouldn't. The dry-run
  dump is the debugging tool.
- **Census drops but skip reasons don't add up** → the bookkeeping is
  lying again; that defeats the experiment's purpose even if the
  numbers look good.

---

## Guardrails / issues to avoid (project law)

- **Precision over recall.** A wrong name applied confidently is worse
  than a minified leftover. Every gate defaults to skip; the floor must
  never become a force-name pass. Log every skip so "covered
  everything" is never implied.
- **Every rename goes through the validated path** —
  `attemptValidatedRename` (`src/rename/validated-rename.ts:165`), or
  the new intentional-shadow variant which must live in
  validated-rename.ts, share its primitives, and be equally tested.
  Never raw `scope.rename` in floor code, never textual replacement.
- **The structural invariant is the net.** `checkStructuralInvariant`
  runs after the floor passes (it already sits at plugin.ts:641, after
  the proposed slot); any violation is fatal, never shipped.
- **LLM is for naming only, never code rewriting.** Workstreams 0/1/3
  are LLM-free; workstream 2 sends names out and applies them through
  validation.
- **Respect frozen scopes** (eval/with taint) — consult the pipeline's
  own taint marks, don't re-derive policy.
- **Branch discipline:** work on branch `exp021-naming-floor`, branched
  from `main` AFTER exp020 merged (2026-07-10, `62da41e`); commit
  incrementally with descriptive messages; do NOT merge — Andrew reviews
  and merges. The floor does not depend on the reconcile pass to
  function, but both flags coexist (see workstream 4).
- **Biome cognitive complexity ≤ 15**; colocated `*.test.ts`;
  `npm run check` green before every commit.

## Code anchors (verified 2026-07-10; exp020 merged to `main` @ 62da41e)

Note: exp020's review-hardening shifted a few line numbers in
`diff-reconcile.ts` (e.g. `isMinifiedName` is now :724, not :674) and
exported `isExportInvolved` from `validated-rename.ts`. Re-verify each
anchor before editing — the design reasoning below does not depend on
exact line numbers.

- Validated rename: `src/rename/validated-rename.ts` —
  `attemptValidatedRename` :165, `fastRenameBinding` :123,
  `getRenameRejection` :80 (the `target-visible` check at :89 is the
  one the shadow variant must replace with a precise subtree check),
  `isValidRenameTarget` :43.
- Eligibility: `src/rename/rename-eligibility.ts` — `createIsEligible`
  :27 (skip-set + `__`-prefix + SWC-helper patterns; everything else
  eligible).
- Soundness: `src/analysis/soundness.ts` — `collectEvalWithTaint` :54;
  frozen via `markEvalWithTaintPreDone` `src/rename/plugin.ts:272`
  (called at :554).
- Pipeline order (`src/rename/plugin.ts`): buildUnifiedGraph :522 →
  captureSemanticBaseline :533 → markEvalWithTaintPreDone :554 →
  markWrapperPreDone :557 → detectAndMarkLibraries :563 →
  applyPriorVersionIfPresent :578 → runRenamePass :597 →
  runLibraryPrefixPass :613 → **[floor slots here]** →
  buildCoverageSummary :622 → checkStructuralInvariant :641 →
  generate :648 → validateGeneratedOutput :652 →
  maybeReconcilePriorDiff :657 (def :211). Options:
  `priorVersionCode` :111, `reconcilePriorDiff` :118.
- Escape mechanisms: `src/rename/function-bindings.ts` —
  `collectOwnedBindingInfos` :35, `collectFunctionNameBinding` :172
  (FunctionExpression/FunctionDeclaration only — the class-id gap),
  `collectShadowedBlockBindings` :198. `src/rename/processor.ts` —
  `processFunction` :189 (collection at :194, shadowed pass :228,
  `uniquifySameNamedBindings` :255). `src/rename/prior-transfer.ts` —
  `applyFunctionNameTransfers` :168 (`oldName === newName` skip :186),
  settled "transferred" state :270–281, close-match :287, function-name
  votes :603–662, `settleModuleBindingNode` :692.
- Coverage counters (the under-reporting): `src/rename/coverage.ts` —
  report-derived counting :87–110, `buildCoverageSummary` :126,
  `formatCoverageSummary` :285.
- Metric-vs-floor "minified" definitions:
  `src/rename/diff-reconcile.ts` — `isMinifiedName` :674 (metric
  heuristic; keep attribute-noise.py-compatible); census `isBunToken`
  in `census-minted-tokens.ts` (floor targeting).
- Reconcile step (exp020, for interplay + harness pattern):
  `src/rename/reconcile-step.ts` — `runPriorDiffReconciliation` :37;
  offline harness `experiments/020-tail-polish/run-reconcile.ts`; CLI
  flag `src/commands/unified.ts:483`.
- Code windows (exp015): `src/rename/code-window.ts` —
  `selectFunctionCode` :168, `capContextCode` :68, `MAX_CODE_LINES` :27.
- Lineage protocol: `experiments/016-diff-noise-convergence/run-chain.sh`
  (EXTRA_HUMANIFY_FLAGS passthrough :50/:58, PRIOR_V120 default :28);
  two-leg A/B `experiments/013-bun-cjs-classification/run-phase2.sh`.
- Metrics: `experiments/014-rename-noise-elimination/attribute-noise.py`,
  `experiments/013-bun-cjs-classification/classify-diff.py`.

## Adjacent, explicitly OUT of scope

- **Swap-cycle temp-name renames** in the reconcile pass (~150 bindings
  blocked by `target-in-scope`, e.g. the `buildQueryParam` ⇄
  `buildQueryParamVal` pair) — separate follow-up (exp020 RESULTS "Next
  candidates" item 2).
- **Reconcile Tier 3** (signature-line param reconciliation on drifted
  bodies) — exp020 item 3.
- **Matcher/fingerprint changes** — the floor is a coverage pass, not a
  matching improvement; do not touch `src/analysis/` hashing.
- Runtime verification of renamed outputs (npm cli.js `--version`
  smoke) — exp020 item 4.
