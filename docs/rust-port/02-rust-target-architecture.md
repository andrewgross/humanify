# Rust target architecture

Not a transliteration of `src/` — a redesign that keeps the algorithms and
invariants (see `01-current-architecture.md`) and uses the port to make
structural what today is conventional. The organizing question for every
design choice below: **which of our hard-won rules can the language enforce
instead of the code review?**

## 1. Foundation: oxc, and why it fits this problem specifically

The Rust oxc crates are a complete Babel-class toolkit: `oxc_parser` →
`oxc_semantic` → `oxc_traverse` → `oxc_codegen`. Two properties matter more
here than raw speed:

- **`oxc_semantic`'s model IS our identity model.** It assigns every binding a
  numeric `SymbolId` and pre-resolves every reference to it, in flat
  struct-of-arrays tables (`Scoping`: scope tree + symbol table + reference
  table). Rename-invariant identity — the thing binding-keyed hashing
  reconstructs on top of Babel `Binding` objects — is the native primitive.
  Hashing, matching, vote propagation, and validated rename all become joins
  over `SymbolId`/`ReferenceId`/`ScopeId` tables.
- **Arena allocation makes AST lifetime a type-level fact.** Every AST lives
  in an allocator whose drop frees everything at once. The ephemeron-cache
  saga, the retained-prior-AST OOM, and the 4.4× scope-refill pathology were
  all manual lifetime management fought against a GC; here "the prior AST is
  released before the split" is a borrow that stops compiling if violated.

The npm/JS side of oxc is parse-only and is not part of this design; the port
is a Rust binary.

## 2. Shape: one workspace, boundaries from the ownership table

Start with a small workspace — resist premature crate proliferation:

```
humanify/
  crates/
    humanify-core/     # the pipeline: model, hash, graph, match, name, place, emit
    humanify-llm/      # provider client, cache, rate limiting, wave scheduler
    humanify-cli/      # arg parsing, config, kill switches, subcommands
```

`humanify-core`'s internal modules mirror `docs/responsibility.md`, and module
visibility enforces the ownership table. The three loudest examples:

- **`rename::validated`** is the only module that can mutate a symbol's name.
  The symbol table's name field is not public; `attempt_validated_rename` (the
  seven legality rules) is the only writer. "Never call `scope.rename`
  directly" stops being a review rule and becomes privacy.
- **`hash`** exposes three distinct key types — `MatchKey` (literal-blurred
  `structuralHash`), `IdentityKey` (literal-verbatim `statementHash`),
  `VendorSignature` — as separate newtypes. A correctness gate that consumes a
  `MatchKey` does not compile. exp046's near-miss (vendor byte-reuse keyed on
  the blurred hash, which would have shipped stale endpoints) becomes
  unrepresentable.
- **`trail`** owns counters and per-item trails, and the tier runners require
  them. See section 5.

## 3. Concept mapping

| today (TS/Babel)                                        | target (Rust/oxc)                                                    |
| ------------------------------------------------------- | -------------------------------------------------------------------- |
| `Binding` object identity                               | `SymbolId` (+ `ReferenceId` for each use)                            |
| binding-keyed placeholder hashing over beautified text  | hashing over a canonical serialization (section 4)                   |
| `FunctionNode` graph (`internalCallees`, `scopeParent`) | same graph, nodes keyed by semantic `NodeId`/`SymbolId`              |
| `matchFunctions` cascade + guards                       | same tiers, same order, same abstain semantics — ports verbatim      |
| `TRANSFER_PIPELINE` registry                            | trait objects over a `TransferContext`; runner owns trails           |
| `attemptValidatedRename` (7 rules)                      | same rules over `Scoping` (capture checks walk the scope tree)       |
| wave scheduler + barrier apply                          | same design: frozen pre-wave context, batch fan-out, ordered apply   |
| `@babel/generator` `compact: false` + beautifier plugin | `oxc_codegen` + a small normalization pass (section 4)               |
| split slices rendered text by byte offsets              | same technique — spans into the rendered output                      |
| `PLACEMENT_TIERS`                                       | same registry, same tiers                                            |
| kill-switch registry + guard test                       | one `env` module; a lint/test forbids `std::env` elsewhere           |
| `--diagnostics` trails, stats JSON                      | serde structs, **same JSON shapes** so existing report tooling works |

The cascade tiers, guards, placement tiers, vote ladders, reconcile tiers, and
alias ladder port as pure algorithm — they are the product and they do not
change in this migration. (Improving them stays a separate, eval-gated
activity; the port must not mix the two.)

## 4. The two deliberate architecture changes

### 4a. Hashing decoupled from the formatter

Today the pipeline hashes BEAUTIFIED text, which couples every hash — and the
split ledger's `emitHashes`, which persist across releases — to the exact
byte behavior of `@babel/generator` plus a beautifier plugin. That coupling is
why a formatter change forces `REBASE_PRIOR` today, and it would chain us to
replicating Babel's formatting forever.

Target: hashes consume a **canonical serialization** produced by the hash
module itself (a deterministic token stream with the same masking rules:
symbols → slot placeholders keyed by `SymbolId`, property names and free
identifiers verbatim, literal policy per key type). Formatting can then
change without moving identity. This is the one place the port intentionally
breaks artifact compatibility, and the migration plan (03, phase 5) pays for
it once: prior-ledger continuity across the transition hop is re-derived from
the prior tree rather than read from TS-era hashes.

### 4b. Provenance sidecar, designed in

Debugging today answers "why does this diff hunk exist?" by offline
reconstruction. The port emits, per output file, a sidecar index built from
data every stage already produces:

```
statement span in emitted file → statementHash → SymbolIds bound/referenced
  → match tier that paired each (resolutionStats ref)
  → naming trail entry that named each (strategyTrail ref)
  → placement tier that housed the statement (placementTrail ref)
```

oxc makes this nearly free: spans exist on every node, the codegen emits
source maps, and the trails already exist as data. A small `explain` CLI verb
joins two releases' sidecars over a diff hunk and prints both sides' chain.
This is the "map a diff back to the AST and the decision" capability discussed
as the debugging goal — it is an output-artifact design, not a language
feature, but the port is the moment to make it native.

## 5. Determinism by construction

The current system earned determinism incident by incident (wave scheduling,
deterministic apply order, tie-breaks, Map-insertion-order bugs). Rust adds
one new hazard and one structural fix:

- **Hazard: `HashMap` iteration order is randomized per process.** A naive
  port of any JS `Map` loop into `HashMap` iteration re-introduces
  draw-dependence everywhere. Rule: decision paths iterate over sorted keys,
  `BTreeMap`/`IndexMap`, or ID-ordered arenas — enforced by a clippy lint
  config plus review convention, and verified the way it is verified today
  (self-hop = 0, warm-cache rerun byte-identical).
- **Structural fix: the trail requirement moves into the runner.** Tier
  registries (matching, transfer, placement) are driven by a generic runner
  that takes `impl Tier` objects and records attempt/settle/abstain per item
  itself. A tier physically cannot run untrailed, so "a pass with an empty
  trail cannot have moved a KPI" holds by construction, and dead-guard
  incidents (`singletonRejected: 0` read as perfect precision) become
  visible as `unguarded` counters from day one.

## 6. Parallelism model

- **CPU (rayon):** per-function fingerprinting and shingles, per-statement
  hashing, match candidate scoring within a tier, placement precompute,
  per-file emit and relink, validation gates. All of these are data-parallel
  over ID-indexed tables today in all but execution. Cross-item decisions
  (bucket resolution, injectivity demotion, vote tallies) stay sequential and
  ordered — parallelism generates evidence; a deterministic single thread
  spends it.
- **LLM (async):** an async client (tokio) whose only job is keeping the
  configured concurrency actually in flight — the measured 7.3× effective
  parallelism against a 32 ceiling is a starved client, not a slow server.
  Waves keep their semantics: frozen context computed before the wave,
  responses collected, renames applied at the barrier in deterministic order.
- **Memory:** one arena per parsed program (fresh bundle, prior bundle), freed
  whole at the stage boundary that no longer needs it. Peak model: both ASTs
  live during matching (as today), one afterward. Expected RSS ~2–4 GB versus
  today's 15–30 GB under a 64 GB heap.

## 7. The LLM layer

- OpenAI-compatible client against the same endpoints (local vLLM or API);
  same retry/timeout envelope (`DEFAULT_LLM_TIMEOUT_MS` semantics carry over).
- **The disk cache format carries over unchanged** (request-content-keyed).
  During migration this is load-bearing: byte-identical prompts let a Rust leg
  replay a TS-populated cache, which is what makes cross-implementation
  neutrality runs possible (03, phase 4). After migration it remains the
  iteration cache with the same rule 10 discipline: never for a verdict.
- Prompt rendering is a pure function of frozen wave context and is
  snapshot-tested (prompts are an external contract with the cache, not an
  implementation detail).

## 8. What does NOT move to Rust

- **The measurement stack stays TypeScript.** `experiments/lib`, the eval
  harness, neutrality, leaderboards, trail reports, sankey — they drive the
  pipeline as a subprocess and read file trees + JSON. The pipeline's side of
  that contract (exit codes, `-run.json`/`-run-status.json`/stats/ledger
  shapes, tree layout) is frozen and documented as the **pipeline contract**;
  the harness cannot tell what language produced it. This is also the safety
  rail: the instruments that judge the port are not themselves being ported.
- **webcrack stays a subprocess.** The webpack/browserify unpack adapter is a
  JS library with no Rust equivalent; the adapter shells out to a pinned Node
  helper. The bun, electron, and passthrough adapters — the ones the Claude
  Code workflow uses — port natively. `electron-unpack/` (installer → app
  directory) already lives outside the pipeline and is unaffected.
- **`using`-desugar needs a decision at build time, not a blocker:** port the
  small desugar (what `@babel/plugin-transform-explicit-resource-management`
  does for the emitted-tree case), or keep it in the Node scaffold step. The
  emitted `run.cjs` re-exec already handles the runtime flag side.

## 9. Idioms

- Errors: `thiserror` per module, loud failure at the CLI boundary — the
  "fail loudly instead of silently half-working" rule
  (`checkDirectoryInputInvariants`) is already the house style and matches
  Rust's grain.
- Tests: colocated unit tests (`#[cfg(test)]`) mirroring today's `*.test.ts`
  discipline; fixtures shared with the TS suite where they are pure
  input/output pairs; red/green TDD unchanged.
- Complexity ceiling: clippy's `cognitive_complexity` replaces biome's.
- No `unsafe` in `humanify-core` (the perf comes from the model, not from
  tricks).
