# Plan — settings, harnesses, DRY, and stage/strategy structure

Four workstreams, broken down so none of it gets lost. Written 2026-08-02, after
exp058 merged. Each item says what "done" means, because several of these are the
kind of change that can look finished and not be.

**Rule that applies to all of it:** anything that can change an emitted byte is
gated like a lever — draw-pinned A/B on four pairs, boot both halves, self-hop,
and `056/walk.sh`. A pure consolidation should produce **byte-identical output**,
and that is a much cheaper gate than a behavioural one: build the tree twice and
`cmp`. Where a workstream item is byte-identical-by-construction, say so and use
the cheap gate. Where it is not, it is a lever and needs the full one.

---

## Workstream A — every setting resolved once, up front

**Problem.** Settings enter the program in at least four ways: CLI options, env
vars, values re-derived downstream from `opts.*`, and files read off disk. There
are **12 `HUMANIFY_NO_*` kill switches among 26 inline `process.env` read
sites**, with
_inconsistent predicates_ — some test `=== "1"`, some `!== "1"`, some bare
truthiness. `HUMANIFY_NO_FAMILY_PERMUTE` is truthy-tested, so `=0` disables the
pass; its neighbours require the literal `"1"`. Nothing can enumerate them, which
is why the A/B harness hard-coded one flag name until exp058 parameterised it.

**End state.** One resolved, frozen settings object built at startup. Downstream
code reads fields. No `process.env` outside the resolver, no `??` defaulting far
from the entry point, no per-call-site error handling for a missing setting.

**Three divergences the inventory found — and the correction that none of them
was live.** They were first written up here (and in a commit message) as bugs.
Checking reachability refuted all three. Recorded in full, because the
over-claim is more instructive than the finding:

1. **Module-lane concurrency.** `rename/processor.ts` sizes the lane
   bundler-aware; `commands/unified.ts` sizes the LLM rate limiter's
   `maxConcurrent` at a flat `+40`. **Not a bug:** that ceiling is an OUTER
   bound over both of the processor's limiters, 40 is the widest the lane
   default can be, so it always exceeds what is scheduled and never binds. It
   also _cannot_ be bundler-aware — `buildProvider` runs before `detectBundle`.
2. **A 10× timeout default.** CLI `300000` ms vs `llm/openai-compatible.ts`
   falling back to `30000`. **Not a live bug:** commander always supplies its
   default, so the small one is unreachable from the CLI. It _did_ apply to two
   experiment scripts that construct a provider directly without a timeout.
3. **`resolveRunConfig` "the single place" being bypassed.** **Not a bug:**
   `analysis/function-graph.ts:761` is a documented default parameter for
   analysis-only callers and the pipeline passes its resolved fn
   (`rename/plugin.ts:859`); `prior-version.ts:232` passes `() => true`
   deliberately; `rename/diff-reconcile.ts:1230` is a `DEFAULT_OPTIONS` value
   every real caller overrides; `commands/unified.ts:1110` passes the real
   bundler and minifier.

**What was actually true, and what shipped:** the same default written in more
than one place, one edit away from mattering. `DEFAULT_LLM_TIMEOUT_MS` and
`defaultModuleConcurrency()` now live in `commands/default-args.ts` with the
ceiling invariant pinned by a test. The eligibility case is left alone and
belongs in C5's responsibility table as _intentional_.

**The lesson for the rest of this plan:** "resolved in two places with different
defaults" is a true statement about the source and says nothing about whether
either value is reachable. Check reachability before calling something a bug —
that is the same rule-3 trap as a predicate that does not test what its name
implies.

- [ ] **A1. Inventory.** Every `process.env` read, every CLI option, every
      downstream re-parse/re-default, every disk-loaded setting. Flag any setting
      resolved in two places _with different defaults_ — that is a live bug, not
      a tidiness issue. **Done — 26 env read sites, 3 predicate styles, the three
      divergences above.** Two more to confirm: `skipLibraries` defaulted to
      `true` in three independent places, and the Bun manifest read twice with
      two separate try/catch guards (`unpack/adapters/bun.ts:99` and `:114`).
- [ ] **A2. Normalise the kill-switch predicate.** Pick one (`=== "1"`) and make
      every switch obey it. **This can change behaviour** for anyone passing `=0`
      or `=true` today, so it is a real change, not a rename — call it out.
- [ ] **A3. A `KILL_SWITCHES` registry** — name, what it disables, which
      experiment gated it, default. Makes "what can I A/B?" answerable as data and
      lets the gate harness take a flag from the registry instead of a string.
- [ ] **A4. `resolveSettings()`** — one function, CLI + env + disk → a frozen
      object, with all invariants enforced there (`enforceFlagInvariants` moves
      in). Everything downstream takes the resolved type.
- [ ] **A5. Thread it.** Replace inline reads. Byte-identical by construction
      once A2 is separated out; gate with a double-build `cmp`.
- [ ] **A6. Lint rule / test** that fails on a new `process.env` read outside the
      resolver, so this does not silently regrow.

**Done means:** `grep -rn 'process\.env' src/ | grep -v settings/` is empty, and
the kill-switch registry lists every switch with its gating experiment.

---

## Workstream B — one measurement library, not thirty

**Problem.** ~30 `*ceiling*.ts` files and ~26 shell scripts across 25 experiment
dirs, re-implementing the same handful of operations. Errors persist between runs
because a fix in one copy never reaches the others. Two live examples:

- The boot gate **skips silently** when `bun` is not on PATH. Three scripts carry
  their own copy of the warning; any that does not gets a green run with the check
  not performed.
- `pinned-ab.sh` inferred isolation from a cache-write count. It passed while
  isolation had actually failed, and printed a confident −144 for a hop whose
  predicted effect was 0. exp058 fixed that copy. Nothing propagates it.

**End state.** `experiments/lib/` holds the shared operations; experiment dirs
hold only what is genuinely specific to that experiment.

- [ ] **B1. Inventory the shared operations** and, for each, where the copies
      _disagree in behaviour_ — that is the bug surface, not the duplication
      itself. **Done:** ≥12 changed-line counters, ~24 tree walkers, 3 ways of
      reading `pairs.json` (two shell variants, one TS, only the shell ones
      honour `EVAL_ENDPOINT`/`EVAL_INPUTS_BASE`), 4 cache-dir env names, and 7
      scripts whose boot gate can skip silently.
- [ ] **B2. `experiments/lib/diff.ts`** — one changed-line counter. Today's copies
      differ on `<`/`>` vs `+`/`-`, on `-r`, on `-N`, and on whether headers are
      counted. Pick one, document what it counts, migrate.
- [ ] **B3. `experiments/lib/trees.ts`** — walk a tree's `.js` files, read a
      ledger, re-derive top-level statements from a bundle. One implementation.
- [ ] **B4. `experiments/lib/gate.sh`** — the draw-pinned A/B, self-hop and boot
      gate, parameterised on the flag under test. Already half-done: exp058
      parameterised `pinned-ab.sh` on `PINNED_AB_FLAG` / `PINNED_AB_TRAIL` and
      added the bundle-identity check. Promote it out of `054/` and migrate.
- [ ] **B5. Make the boot gate fail loudly** when `bun` is missing, everywhere.
      A check that can skip silently is worse than no check.
- [ ] **B6. `experiments/lib/counterfactual.ts`** — exp058's construction
      (perturb a copy of the input ledger, run the REAL splitter and emitter,
      re-diff) generalised. It is the only ceiling harness that exercises
      production code end to end, and it reproduced the shipped result 8 hops of 8. Include its fidelity control: assert the reconstruction reproduces the
      shipped tree, and classify what differs.
- [ ] **B7. Leave archived experiments alone.** Do NOT rewrite closed
      experiments' scripts to use the library — their published numbers are
      pinned to the code that produced them (rule 9). Add a pointer instead.

**Done means:** a new experiment can gate a lever without writing a shell script,
and the boot gate cannot skip silently anywhere.

---

## Workstream C — DRY, with designated responsibility

**Problem.** The dangerous duplication is not "two functions look alike" — it is
**two functions that answer the same question differently, where nothing declares
the difference.** The seed case: `buildFullFingerprint` populates `features` and
`memberKey`; `buildBindingFullFingerprint` populates neither. Both feed the same
matching cascade, so `singletonContradicts` — the guard meant to stop a deleted
helper auto-matching an unrelated added one — is **structurally dead on the
binding path**: 11,094 accepts, 0 checks, reported as "0 rejections", which reads
like precision. exp058 spent real time discovering that.

- [ ] **C1. Fan-out sweeps.** **Done** — six parallel sweeps. Rename application
      is genuinely single-path (one documented exception for `#private` names);
      the real finds are the fingerprint asymmetry (C3), `nsNameIsFree`
      reimplementing three of `getRenameRejection`'s seven rules, and three
      similarity constructions with different math and cutoffs.

      **One sweep result did NOT survive checking, and is recorded here so it is
      not rediscovered as a bug:** ~7 experiment scripts were flagged for reading
      `ast.program.body` instead of entering the wrapper. They all take
      split-tree directories, where files have no wrapper and `program.body` is
      correct. The real (latent) issue is that nothing declares which shape a
      helper expects — point one at a bundle and it returns 1 statement and a
      plausible near-zero number instead of erroring. Add an assertion, not a
      consolidation.

- [ ] **C2. Triage by RISK, not by line count.** HIGH = a consumer can silently
      get a dead field or a different answer for the same input. MEDIUM =
      redundant but consistent. LOW = cosmetic. Only HIGH is urgent.
- [ ] **C3. Fix the fingerprint asymmetry first** — either populate the missing
      fields on the binding path, or make the fingerprint type _declare_ which
      fields it carries so a guard cannot read an absent one silently. **This
      changes matching behaviour** (the guard would start firing), so it is a
      lever: ceiling first, then the full gate.
- [ ] **C4. One `RESOLUTION_STAT_KEYS`-style registry per cascade**, so per-stage
      counters cannot drift from the stages (see D3).
- [ ] **C5. Write down designated responsibility** — one owner per question:
      "who decides a name is legal", "who applies a rename", "who counts changed
      lines", "who reads the ledger". A short table in `docs/`. Cheap, and it is
      what stops the next duplicate being written.

**Done means:** every HIGH finding is closed or has a recorded reason not to be,
and the responsibility table exists.

---

## Workstream D — stages and strategies

Your four stages are right, and the code is genuinely organised around them. The
list is **missing four more**, and they are disproportionately where the bugs have
been:

| #   | stage                                | owner                                                   | notes                                                                                                                             |
| --- | ------------------------------------ | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **detect** bundler/minifier          | `detectBundle` → `buildPipelineConfig`                  | produces `PipelineConfig`                                                                                                         |
| 2   | **unbundle**                         | `unminify` + unpack adapter                             | bun / webcrack / passthrough                                                                                                      |
| 3   | **name**                             | rename plugin                                           | LLM + prior version + minifier type                                                                                               |
| 3a  | **match** _(missing)_                | `matchFunctions`, binding cascade                       | produces the correspondence BOTH naming and placement consume — exp058 showed the naming→placement boundary is where it gets lost |
| 4   | **place**                            | `PLACEMENT_TIERS`                                       | prior version + AST layout + minifier type                                                                                        |
| 5   | **emit** _(missing)_                 | `alignEmissionOrder`, `buildNsVars`                     | within-file ORDER and require-ALIAS are separate decision systems from placement, with their own kill switches                    |
| 6   | **vendor** _(missing)_               | body reuse, manifest order, relink                      | was unscored for 13 experiments and turned out to be **2.4× all measured `src` noise** (exp046)                                   |
| 7   | **finish on disk** _(missing)_       | `finishSplitOutput` → relink, `using` desugar, scaffold | **mutates files after the tree is written** — starved exp054 by 922 lines when a pass ran before it                               |
| 8   | **post-split reconcile** _(missing)_ | `post-split-reconcile.ts`                               | must run after 7, on disk                                                                                                         |
| 9   | **carry** _(missing)_                | bundle carry                                            | decides what the NEXT release inherits — only visible on a walk                                                                   |

**Where strategy selection stands today.** The picture is the opposite of what it
first looks like, and the distinction is the whole point of workstream D.

**Selection between strategies is 100% static.** Three registries exist and all
three decide once, from detection, and are never revisited:

- **unpack adapters** — `src/unpack/index.ts:11` `[Webcrack, BunUnpack, Passthrough]`,
  chosen by `find(a => a.supports(detection))`. This is the model to copy.
- **library detectors** — `src/library-detection/index.ts:9`, same shape.
- **split adapters** — `src/split/adapters/index.ts:14`, same shape, but carries
  no label/description/counters, so the caller never learns which one ran.

**Branching on a run-time measurement is already pervasive — just never at the
strategy level.** Inside the split and rename stages, behaviour is decided by
evidence measured from the current-vs-prior comparison, not by flags:
`PLACEMENT_TIERS` per statement; `claimPriorAliases` and `alignEmissionOrder` on
whether prior hashes/names still align; vendor body reuse on a measured signature
equality; manifest ordering on whether prior hash groups are recoverable;
`reconcilePostSplit` self-discarding when it measures no divergence; the naming
floor sweep gated on whether prior-aware naming actually ran.

So the gap is **not** "we can't react to what we learn." It is that reacting
happens _inside_ a stage and selecting happens _only at the top_, with nothing in
between. The ~14 kill switches only turn a measured tier OFF; none selects an
alternative.

**Two detected facts are recorded and never read**: `minifierTier` has no reader
anywhere in `src/`, and `config.signals` has none either (the verbose log reads
`detection.signals`, the pre-config copy). Worth knowing before designing
anything that assumes detection output is load-bearing.

**One thing that looks like duplication and is not**: the split stage
deliberately re-parses the bundle from the code string rather than reusing the
rename stage's AST, and `runSplit` explicitly drops `renameResult.ast` first — to
avoid holding two full-bundle ASTs at once. Documented at `unified.ts:696-698`
and `:828`. Do not "fix" it.

- [ ] **D1. Map the true stage boundaries** — inputs, outputs, and every place a
      stage re-derives something an earlier stage already computed (re-parsing the
      bundle, rebuilding the graph, recomputing hashes). (Agent sweep in flight.)
- [ ] **D2. Decide which stages genuinely need alternatives.** Not all four-plus
      do. Detection and unbundling clearly do. Naming and placement have
      _cascades within one strategy_, which is a different shape and may be the
      right one — do not convert on principle.
- [ ] **D3. Convert `resolveMatch` to a registry** if D2 says so — it is the
      worst offender at ~3 edits in 3 files per stage. Byte-identical by
      construction if the order is preserved; gate with a double-build `cmp`.
- [ ] **D4. Give each stage a declared strategy set** where D2 justifies it,
      following the unpack-adapter pattern (register, select by config).
- [ ] **D5. Decide separately whether runtime-measured selection is wanted.**
      This is a genuine architecture change, not a refactor. It also weakens
      reproducibility — a pipeline that picks its own strategy mid-run is harder
      to A/B, because the two legs may not run the same code. If we do it, the
      chosen strategy must be recorded per run and be pinnable.
- [ ] **D6. Fold stages 5–9 into the mental model and the docs**, so the next
      person does not put a pass before `finishSplitOutput` and lose 922 lines to
      it again.

**Done means:** the stage table above is accurate and in `docs/`, each stage's
extension point is named, and D5 has an explicit yes/no.

---

## Sequencing

A → B → C → D is roughly right, and not because of dependencies but because of
**risk**:

1. **A** is mostly mechanical and makes everything else easier to A/B.
2. **B** must land before C and D, because those need a trustworthy gate and the
   gate is currently forked per experiment.
3. **C** is where behaviour changes hide. Do it with B's harness available.
4. **D** is design, and the inventory from A–C is what tells us which parts are
   worth restructuring rather than documenting.

The one item that should jump the queue is **C3** (the fingerprint asymmetry): it
is a live correctness gap in the matcher, not a tidiness problem.
