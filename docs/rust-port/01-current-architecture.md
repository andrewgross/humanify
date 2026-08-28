# Current architecture: what the tool is and what it knows

This is the inventory half of the Rust port plan: what humanify does today, which
parts are algorithms worth carrying, which parts are scar tissue worth designing
out, and which numbers describe the machine we are porting. The target design is
`02-rust-target-architecture.md`; the conversion sequence is
`03-migration-plan.md`; the measured performance case is
`04-performance-model.md`.

Nothing here is aspirational. Every mechanism named below exists on `main` (or
`feat/electron-unpacking` where noted) and is exercised by the eval gate.

## 1. Intent

Humanify turns a minified JavaScript bundle into a readable, runnable source
tree — and does it **stably across releases** of the same product. The end goal
(from `docs/roadmap-noise-reduction.md`) is a git history of deobfuscated
releases where `git diff v(N-1)..vN` shows only **real** upstream change, not
churn the pipeline itself introduced. Everything unusual about the architecture
follows from that goal:

- **Cross-version identity is the core problem.** Naming one bundle well is
  easy; naming release N+1 so that unchanged code keeps its release-N names,
  files, and order is the hard part, and it is where all the machinery lives.
- **Precision over recall, everywhere.** A wrong match ships a wrong name or a
  misplaced statement and costs two diffs; a missed match costs one LLM call.
  Every matching tier abstains rather than guesses.
- **Determinism is a feature, not an accident.** Byte-identical reruns from a
  saturated LLM cache are an enforced invariant (self-hop = 0 diff lines), and
  every source of draw-dependence found so far has been hunted down and removed
  (wave-scheduled prompt context, deterministic apply order, tie-breaks on
  stable keys).
- **Zero minted tokens.** The pipeline never renames anything TO a minified
  name, and drives the count of invented placeholder names toward zero
  (currently ~30 LLM-named mints per walk with zero collisions).

## 2. The pipeline

Twelve stages plus three post-placement passes (`docs/pipeline-stages.md` is
the authority). Only **three** touch the LLM; everything else is deterministic
local compute. The LLM/CPU column below was verified by tracing `LLMProvider`
imports.

| #      | stage                 | what it does                                                             | LLM?                 |
| ------ | --------------------- | ------------------------------------------------------------------------ | -------------------- |
| 1      | Detect                | bundler/minifier from a 16KB slice; directories via `detectElectronApp`  | no                   |
| 2      | Select unpack adapter | registry of 4 (bun, electron, webcrack, passthrough), chosen by name     | no                   |
| 3      | Unpack                | adapter splits app code from vendored modules                            | no                   |
| 4      | Detect libraries      | registry of 2, `supports()` probe                                        | no                   |
| 5      | Name vendor files     | vendor filenames (prior names carried; only fresh ones prompted)         | tiny (~17 names/hop) |
| 6      | Format                | beautify via babel plugin; output shape is a fixed point                 | no                   |
| 7      | Build function graph  | `FunctionNode` graph; `internalCallees` drives order AND matching        | no                   |
| 8      | Match against prior   | fingerprint cascade over functions and module bindings                   | no                   |
| 9      | Name identifiers      | mechanical transfer tiers, then LLM waves for the residue                | yes (the bulk)       |
| 10     | Place statements      | `PLACEMENT_TIERS` — which emitted file each statement belongs to         | no                   |
| 11     | Split                 | `stableSplitFromCode`: prior → inherit layout; no prior → fresh grouping | folder naming only   |
| 12     | Emit + finish         | runnable CJS tree, scaffold, relink, ledgers                             | no                   |
| post-a | post-split reconcile  | per-file rename reconciliation against the prior release's same path     | no (deterministic)   |
| post-b | carry into bundle     | write names back into `.humanify/humanified.js` — the NEXT run's prior   | no                   |
| post-c | finish on disk        | scaffold, bun factory relink, ledgers, eval stats                        | no                   |

Two registries are real plug points (unpack adapters, library detectors); the
Electron directory input (branch `feat/electron-unpacking`) is the model for
adding an input format: upfront detection, its own adapter, loud failure for
every flag the format cannot honor — never a silent mid-run fallback. The
legacy fallback splitter was deleted in 2026-08 after an execution census
measured it at zero runs; there is ONE split path.

## 3. The identity system (the part that is the product)

Cross-version identity rests on rename-invariant hashing plus a graph. This is
the intellectual core and ports as pure algorithm.

**Three hashes, deliberately not interchangeable** (`docs/matching-cascades.md`):

| hash                  | over                    | names         | literals                               | serves                            |
| --------------------- | ----------------------- | ------------- | -------------------------------------- | --------------------------------- |
| `structuralHash`      | function/binding bodies | masked        | BLURRED (string→length, num→magnitude) | match candidate bucketing         |
| `statementHash`       | top-level statements    | masked        | VERBATIM                               | statement twins, placement, order |
| `structuralSignature` | whole vendor files      | slot ordinals | VERBATIM                               | vendor body byte-reuse            |

The literal policy is load-bearing: `structuralHash` cannot see a changed
endpoint URL of the same length, so it must never gate a correctness decision
(exp046 — keying vendor reuse on it would have shipped stale endpoints). The
Rust design should make this distinction a type, not a convention.

**Placeholders are keyed by resolved binding**, not by name text — two
occurrences of `x` hash identically only if they resolve to the same binding.
Property names and free identifiers are hash CONTENT (kept verbatim). The
current pipeline hashes the BEAUTIFIED text form; this couples hashing to the
formatter and is one of the things the port should change (see 02, section on
hashing).

**The function graph**: `FunctionNode.internalCallees` drives both processing
order (callees named before callers so caller prompts see callee names) and
fingerprint matching (callee/caller shapes are cascade evidence).
`scopeParent` tracks lexical nesting separately.

## 4. Matching: the cascade and its guards

`matchFunctions` pairs prior↔fresh functions through resolution tiers,
strongest first: unique `structuralHash` → binding identity → memberKey →
enclosing-statement hash → callee shapes → caller shapes → callee hashes →
two-hop shapes → shingle similarity → scope ordinal. Module bindings run the
SAME cascade in alternating rounds — each side's matches crack the other's
same-hash buckets via reference identity.

Three guards carry the whole precision story:

- **Singleton rejection** — a bucket of one on each side would auto-match a
  deleted helper to an unrelated added one; the guard demands corroboration.
  (Known weakness, measured: two of its three signals are functions of the
  bucket key and cannot fail; on the binding path it examines nothing —
  11,094 unguarded accepts on 215→216. `singletonUnguarded` counts this.)
- **Contradiction** — a candidate rejected by strong evidence must not win by
  weaker evidence later; an emptied filter stops the search.
- **Injectivity demotion** — two priors claiming one fresh function means at
  most one is right, so neither gets it.

Unmatched functions go to **close-match** (cosine ≥ 0.8 over 12 count
features, then gated by statement alignment or shingle Jaccard ≥ 0.5) — which
yields LLM _context_, never a mechanical rename, because content changed.

Three distinct similarity measures exist on purpose (close-match cosine,
content shingles, binding-role shingles); they answer different questions and
must not be merged (`docs/responsibility.md` has the table).

## 5. Naming: evidence-ordered transfer, then LLM waves

`docs/naming-pipeline.md` is the authority. The invariant across all of it:
**every rename goes through `attemptValidatedRename`** (seven legality rules:
binding exists, target legal, not in scope, no outer capture, not a
previously-free name, no child shadowing; collisions reject — a tier never
overwrites another tier's work). Babel's `scope.rename` is never called
directly in transfer code; capture bugs from unvalidated applies are how this
rule was earned.

Phase order is monotone-decreasing evidence strength:

1. **Statement twins** — unique 1:1 `statementHash` on both sides is
   whole-statement identity, literals included; outranks everything.
2. **Exact-match slot tables** — byte-identical-modulo-names functions
   transfer params + locals mechanically.
3. **Close-match positional transfer** for signatures and aligned body locals.
4. **Binding-cascade renames.**
5. **Vote propagation** — external-reference testimony; ≥2 agreeing votes name
   module bindings and cold heads; below the floor, the shared single-vote
   ladder (one EXACT slot vote + cross-map injectivity + role corroboration).
6. **Retry** — collision-rejected renames re-attempted as phases free tokens;
   pure cycles broken via a temp name.
7. **LLM waves** — cold and close-matched functions in graph order, prompt
   context FROZEN pre-wave, renames applied at a barrier in deterministic
   order. This is the only scheduler; the free-running loop was deleted.
8. **Post passes** — prior-name snap (stem-unique suggestions snap to the
   prior name), naming floor (deterministic minted-token coverage),
   reconcile-prior-diff (text-diff tiers incl. the ≥2-caller consumer tier,
   with a corpus gate that turns the pass off on shuffle-shaped pairs),
   deferred prior-aware sweep, family-permute (context-strict swap
   correction), and per-file post-split reconcile applied to emitted TEXT.

Collision decoration has ONE owner (`DECORATION_WORDS` in
`src/llm/validation.ts`); the ladder retries under stem pressure rather than
minting.

## 6. Placement, order, and emit

**Placement** (`PLACEMENT_TIERS`, `src/split/stable-split.ts`) decides each
statement's file by evidence strength: hash (equal counts, one prior home) →
identity preempt → anchor preempt → name votes (with the all-same and
identity-fill fallbacks) → content anchor (four abstaining gates; the ≥50%
token-overlap gate exists because one shared string once paired a 5,073-line
statement with a 7-line one) → locality. A shape refusal excludes the one
statement class whose masked hash carries no content (initializer-less
declaration lists) — deliberately a whitelist of one, measured against the
alternative.

**Within-file order** inherits the prior emit sequence through a uniqueness
gate, then a load-order filter keeps read-after-write/write-after-write facts
honest (exp038 — reorder fell from 33% of noise to under 7% everywhere).
**Import aliases** are one per module tree-wide, with a widening ladder and a
both-widen rule on contest.

**Vendor** is never named: unchanged bodies are byte-reused from the prior
release keyed on `structuralSignature`, and the module manifest keeps prior
order so an in-place edit does not become delete+add.

**Emit**: one full-tree `@babel/generator` render (`compact: false` — prettier
was removed; its Doc IR exceeded 4GB on a 14MB file), then the split SLICES
the rendered text by statement byte offsets — split files are never
re-generated. Up to three optional re-parse+re-generate cycles exist
(reconcile step, sweep step, family-permute step). The emitted tree is
runnable CJS (accessors before requires, `using` desugar, namespace re-export
relocation, bun factory relink), verified by a boot gate.

**Carry**: names are written back into `.humanify/humanified.js`, which is the
next release's prior — the only stage whose output is consumed by a future
run. Top-level renames never carry (export keys are strings; 238/238 drifted
when they did).

## 7. Cross-cutting discipline (the scar tissue with the lessons in it)

- **One owner per question.** `docs/responsibility.md` maps ~30 questions
  (name legality, applying a rename, counting changed lines, walking a tree,
  reading a ledger...) to single owners. The expensive bugs were never "two
  functions look alike" — they were two functions answering the same question
  DIFFERENTLY with nothing declaring the difference (a precision guard that
  was structurally dead for 11,094 accepts read as perfect precision).
- **Cascades want to be registries.** Fourteen ordered-fallback ladders run in
  stages 8–10; the three that decide the output (naming, matching, placement)
  now have per-stage counters and per-item trails, and every one of those
  counters has caught a dead guard or explained a KPI. The other eleven are
  hand-written `if` ladders. "A pass with an empty trail cannot have moved a
  KPI" (measurement rule 11) is the reason trails exist.
- **Kill switches** have a single registry (`src/kill-switches.ts`) with a
  test that fails if `src/` reads one anywhere else.
- **The clone census** (`npm run census:clones`) is an advisory gate against
  cross-file twin functions.

## 8. The measurement stack (ports as CONTRACT, not code)

Every quality claim routes through instruments that interact with the pipeline
**as a subprocess plus file trees** — which is exactly what makes them
port-friendly: they do not care what language the pipeline is written in.

- `npm run check` — 8 stages (typecheck, lint, knip×2, clone census, unit,
  fingerprint snapshots, e2e), ~25s, the commit gate.
- **The eval harness** (`experiments/034-eval-harness/`, driven by
  `npm run eval -- score <label>`): four real version pairs (85→86 quiet,
  118→119 feature, 197→198 feature-on-large-base, 215→216 largest), KPI table
  with declared directions (`noise`/`reloc`/`mints` drive down; `novel`/
  `realLn` are real change and must NOT move), measured noise bands from
  same-commit cold repeats, boot gates, self-hop idempotence invariant.
- **Neutrality** (`experiments/lib/neutrality.sh`): for should-change-nothing
  edits, byte identity over a warm shared cache — 0 differing files, 0
  differing lines, baseline leg writes zero cache entries. The warm-cache
  requirement is proven: a cold null control diverged 177 files on
  byte-identical `src/`.
- **`docs/measurement-pitfalls.md`** — eleven rules, seven learned by
  publishing a wrong number first. The ones that bind any future harness:
  rule 8 (enumerate what the harness does not look at before believing a
  floor — `vendor/` went unscored for thirteen experiments at 2.4× the
  measured noise), rule 10 (a determinism aid left on for the verdict makes
  the verdict a lie; live-call evidence is CACHE WRITES, not a request
  counter), rule 11 (a gate cannot resolve an effect below its own noise
  floor — ±2,800 git lines per hop — and will print a confident sign anyway).

These rules are about measurement, not TypeScript. They survive the port
verbatim, and the port's own parity gates are designed under them.

## 9. The machine being ported: scale and shape

Measured on the eval pairs (cold, `results/baseline-2026-08-03`):

- Inputs: minified bundles 11.7 → 19.5 MB across the pair set; the prior
  carry bundle parsed alongside is 16.8 → 31.9 MB. Two multi-MB ASTs are live
  at once during matching.
- Scale per hop (largest pair): **64,493 functions**, **200,425 bindings**,
  **388,478 exact-match slot attempts**, **35,903 statements placed**, ~1,500
  emitted files + ~1,625 vendor files, output tree ~93 MB.
- LLM traffic per cold hop: 1,600–3,500 calls, 2.6–5.2M tokens, avg call
  1.2–1.8s, concurrency 32 → **75–135s of LLM wall time**.
- Wall time per pair: 532–1,013s. **≈85% of a hop is pure CPU on one thread**
  — the process pins one core while the GPU sits idle.
- Peak RSS 15–30 GB; the eval sizes the heap at 64 GB because 215→216 OOMs at
  14 GB cold. Recurring pathology class: V8 heap pressure (ephemeron/WeakMap
  rehash, retained ASTs) — three separate incidents each fixed by manual
  lifetime management that a Rust arena would make structural.
- Determinism: 98% of functions and 97.7%+ of module bindings settle
  mechanically without the LLM; only ~3.3% of functions reach a prompt.

The performance implications — what a Rust/oxc port buys and what it cannot —
are quantified in `04-performance-model.md`.
