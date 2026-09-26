# Migration plan: TypeScript → Rust without losing the thing that works

The port's central risk is not writing Rust — it is silently changing
DECISIONS while rewriting the machinery that makes them, in a system whose
whole value is decision stability, judged by instruments with a known ±2,800
line/hop noise floor. The plan is built around one idea:

> **Hold the text fixed, port the decisions, verify equivalence exactly;
> only then move the text, and re-baseline once.**

The seam that makes this possible already exists: stage 6 (beautify) is a
one-time normalization of the input, and everything downstream — hashing,
matching, naming, placement, even split emission (which slices rendered text
by byte offsets) — operates on that normalized text. While the Rust pipeline
consumes TS-beautified text, every downstream artifact is byte-comparable
between implementations, including LLM prompts, which makes the shared
response cache a cross-implementation replay instrument. The formatter swap
happens last, alone, as its own measured event.

A second principle, inherited from the house rules: **the instruments are not
ported.** The eval harness, neutrality, boot gates, and reports drive the
pipeline as a subprocess over file trees, so the TS pipeline and the Rust
binary are judged by the same judge throughout. The TS core is deleted at the
end, not maintained alongside (no backwards compatibility, one path).

This document is the strategy; the deep-planning set expands it. The parity
gates and artifact schemas are specified in
[`07-differential-validation.md`](./07-differential-validation.md), the
phase-by-phase work packages and milestones in
[`10-work-breakdown.md`](./10-work-breakdown.md), and the risks with their
early probes in [`11-risk-register.md`](./11-risk-register.md).

## Phase 0 — freeze the contract and the oracle (pure TS work)

1. **Write the pipeline contract doc**: CLI flags, exit codes, tree layout,
   `-run.json` / `-run-status.json` / stats / ledger / trail JSON shapes.
   This is what the harness consumes and what the binary must produce.
2. **Add `--dump-artifacts <dir>` to the TS pipeline**: serialize the
   decision record at each stage boundary — function graph (nodes, callees,
   scope parents), all three hash tables, match results with tier per pair,
   resolutionStats, transfer ledger (every mechanical rename with tier),
   prompt set, placement decisions with tier, emission order. Mostly these
   exist as trails already; this makes them complete and machine-diffable.
3. **Prove the flag neutral** with `experiments/lib/neutrality.sh` (this is
   exactly the should-change-nothing case it exists for).
4. **Freeze the oracle corpus**: the four eval pairs + the fixture set, with
   dumped artifacts committed or content-addressed. A warm LLM cache for the
   four pairs is captured at the same commit.

Exit gate: neutrality NEUTRAL for the dump flag; oracle artifacts stored.

## Phase 1 — core model: parse, semantic, graph, hashes

`humanify-core`: oxc parse + semantic over the TS-beautified text; function
graph; the three hash families with the canonical serialization (02 §4a);
shingles.

Exit gate — **decision parity, not byte parity**: hash bytes will differ from
TS by design (different serialization), so the gate compares what hashes are
FOR: bucket partitions equal (same groups of functions share a key on both
implementations), twin sets equal, graph edges equal. Any partition
difference is a real semantic divergence to chase. Runs on the oracle corpus.

## Phase 2 — the matching cascade

Function + binding cascades, alternating rounds, all guards (singleton,
contradiction, injectivity), close-match candidate generation with its
corroboration gates, role evidence, statement twins.

Exit gate: **identical match sets** (pair-for-pair, tier-for-tier) and
identical resolutionStats on all four oracle pairs. This must be exact — the
cascade is deterministic, so any diff is a bug in one implementation or a
real behavioral fork to adjudicate consciously.

## Phase 3 — mechanical transfer and validated rename

`TransferContext`, the tier registry, vote propagation and single-vote
ladders, retry/unwind, validated rename with the seven rules and claims
ledger.

Exit gate: identical rename ledgers (every applied and every REJECTED rename,
with reason) versus the TS dump on the oracle pairs. Rejections matter as
much as applies — the guards are the product.

## Phase 4 — LLM waves, replayed

Wave profile, frozen context, prompt rendering, response validation ladder,
barrier apply; the cache module reading the SAME cache format.

Exit gate, in two steps:

1. **Prompt byte-parity**: the Rust leg's prompt set is byte-identical to the
   TS dump (possible because both read TS-beautified text). Until this holds,
   nothing downstream is comparable.
2. **Warm-replay equivalence**: run Rust against the captured warm cache;
   every name assignment equals the TS warm run's. Zero cache writes on the
   Rust leg — the same proof obligation neutrality already uses (rule 10:
   the cache is legitimate here because the verdict is about determinism
   with the model held fixed, and the write count proves the replay).

## Phase 5 — placement, split, emit — and the formatter swap

Placement tiers, layout inheritance, emission order + load-order filter,
alias ladder, vendor body reuse, CJS emit, scaffold/relink, carry,
post-split reconcile, sidecar emission (02 §4b). Two sub-steps:

- **5a, still on TS-beautified text**: exit gate is byte-identical emitted
  trees versus TS on warm replay (placement, order, and reconcile are
  deterministic; the text is shared; so the trees must match exactly).
- **5b, swap in oxc formatting** (beautify + codegen native). This is the one
  intentionally breaking step. Everything downstream of text changes at once,
  so it is judged like a formatting change is judged today: `REBASE_PRIOR`
  regenerates bases, then the standard gates — self-hop = 0 (the new fixed
  point), boot gates ×4, concat-equivalence, mints ≈ 0, eval KPIs against the
  freshly rebased reference with `novel`/`realLn` unmoved. Ledger continuity
  is a non-issue under the compatibility posture (02 §9): the emitted
  libraries are regenerated from scratch by the Rust pipeline — the
  post-cutover walk starts fresh, each hop consuming the previous Rust hop's
  version record (`12-layout-and-diff.md` §2). No TS-era ledger or hash is
  ever read in production; re-derivation-from-text exists as the standing
  `tableVersion`-bump mechanism, not as TS compatibility.

## Phase 6 — cutover and deletion

**DONE 2026-09-26 (branch `rust/cutover`): see `19-cutover.md`.** The walk segment below was not run at the cutover (open in 19 §6).

- Point `run.sh`, `walk-versions.sh`, and `npm run eval` at the binary (one
  variable each — they spawn a subprocess).
- Run one full walk segment (e.g. 20 hops) side-by-side sanity vs the TS
  reference era: hop times, KPI bands, boot gates.
- **Delete `src/`** (keep `experiments/`, `scripts/`, `test/e2e` where they
  are harness-side). The TS pipeline does not enter maintenance mode; the
  oracle dumps and the git history preserve it for archaeology.

## Sequencing realities

- Phases 1–3 are the bulk of the intellectual transfer and are pure-CPU,
  fully deterministic, and gate-checkable in seconds per run — the fast part
  of the loop. Phase 4's gates are cheap too (warm replay). Phase 5b is the
  only expensive, noisy-instrument step, and it happens exactly once.
- The port is a **months-scale project**: `src/` is ~45k lines of non-test
  TypeScript plus ~41k of tests encoding behavior, and the parity harness is
  new work on both sides. The honest framing: phases 0–2 prove whether the
  approach works at a fraction of total cost; treat phase 2's exact-match
  gate as the go/no-go milestone before committing to the rest.
- During the port, `main` stays live: the freeze applies only to the ORACLE
  commit. If mainline decisions change (levers land), re-dump the oracle at a
  new commit and rebase parity — the dump flag makes that a re-run, not a
  re-design.
- **What a no-go at the phase-2 milestone means, concretely.** The abort path
  deserves the same explicitness as the success path: the port branch is
  archived (not deleted — it is the record of why), and two deliverables are
  kept regardless because they pay for themselves in the TS era: the
  `--dump-artifacts` flag (a machine-readable decision record useful for any
  future refactor) and the `humanify-parity` differ (it compares TS dumps to
  TS dumps, which is an instrument for TS refactors too). The README's STATUS
  line records the no-go and the reason; nothing else changes.

## Risks

The table below is the strategy-level summary; the maintained, probe-carrying
version is [`11-risk-register.md`](./11-risk-register.md) — its rows are
edited in place as probes land, so when the two disagree, the register is the
current one (rule 9).

| risk                                         | exposure                                           | mitigation                                                                |
| -------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------------------------- |
| Decision drift hidden in "equivalent" code   | the whole value of the system                      | phase gates compare decisions exactly; formatter swap isolated to 5b      |
| Babel↔oxc parse differences (edge syntax)   | low; both parse the same spec, bundles are vanilla | phase 1 partition diffs surface any node-shape mismatch immediately       |
| Prompt byte-parity harder than expected      | phase 4 blocks                                     | prompts are pure functions of dumped context; snapshot tests per prompt   |
| Scope/capture semantics differ in validation | wrong renames = correctness                        | port the 7 rules against `Scoping` with the existing capture fixtures     |
| `HashMap` iteration nondeterminism           | draw-dependence returns                            | ordered structures in decision paths + self-hop/warm-rerun gates (02 §5)  |
| webcrack has no Rust equivalent              | webpack-bundle inputs only                         | subprocess adapter; bun/electron/passthrough port natively                |
| `using`-desugar plugin                       | runnable-tree boot                                 | small dedicated pass or Node scaffold step; boot gate decides             |
| Two stacks alive too long                    | split attention, drift                             | oracle freeze + phase gates keep TS read-only as a reference; delete at 6 |
| Team iteration speed in Rust                 | slower lever work post-port                        | the levers are data/algorithm work over flat tables; the sidecar and      |
|                                              |                                                    | trails-by-construction are designed to make investigation cheaper, not    |
|                                              |                                                    | harder — judge after phase 2                                              |

## What would falsify the plan early

Worth stating in advance (the briefs-are-hypotheses rule applies to this one
too):

- Phase 1 partitions that disagree for reasons that are neither bugs nor
  conscious forks — e.g. beautified-text hashing turns out to encode a
  distinction the canonical serialization cannot reproduce. That would force
  hash-compat mode (hash the shared text instead) and weaken 4a.
- Phase 4 prompt parity failing on ordering that traces to something
  unfixably nondeterministic in the TS side rather than the Rust side — that
  inverts the oracle relationship and needs a TS fix first.
- The phase-2 gate needing tolerance instead of equality. It should not; if
  it does, stop and understand why before proceeding — an inexact match gate
  is how decision drift gets in.
