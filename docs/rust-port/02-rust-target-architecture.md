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

Five crates (revised 2026-08-28 by the Rust-idioms review; the original
three-crate sketch could not support the build order 07 §4 commits to —
`humanify-parity` is the first code written and needs the shared types to
exist before the pipeline does):

```
humanify/
  crates/
    humanify-model/    # pure data, no oxc/rayon/tokio: version-record tables,
                       # dump schemas, the three hash newtypes, span newtypes,
                       # the NameProvider trait + request/response types
    humanify-core/     # the pipeline library: all twelve stages + post passes,
                       # one public entry point; never reads std::env, never async
    humanify-llm/      # implements NameProvider for the real client; owns
                       # tokio/reqwest, disk cache, rate limiting — behind a
                       # SYNCHRONOUS public API (block_on inside)
    humanify-cli/      # thin binary: clap-derived RunConfig passed by value
                       # into core, the ONE env/kill-switch-reading module,
                       # wiring the concrete client, exit codes / ERROR: blocks
                       # / progress UI
    humanify-parity/   # the dump comparer; depends on humanify-model only —
                       # buildable before any pipeline Rust exists
```

Detection, unpack, and library detection live in `humanify-core` (they are
pipeline stages 1–4, not CLI concerns); `humanify-cli` stays argv-to-config
translation plus output formatting, so tests and tools can run the pipeline
as a library without pulling in `clap`. The env/kill-switch module lives in
the CLI crate deliberately: core receives switches as config values and has
zero process-environment coupling.

`humanify-core`'s internal modules mirror `docs/responsibility.md` (module
tree: `ingest`, `hash`, `graph`, `modules`, `detect`, `unpack`, `libdetect`,
`matching::{cascade,close,twins}`, `prior`, `rename::{validated,transfer,votes}`,
`naming::{prompts,waves,reconcile,passes,driver}`, `place`, `layout`, `emit`,
`finish`, `sidecar`, `trail`), and module visibility enforces the ownership
table. The three loudest examples:

- **`rename::validated` owns names as an OVERLAY, and the AST is read-only.**
  The original design claimed oxc's symbol-table name field could be made
  private; it cannot — `Scoping` publicly exposes `rename_symbol` and
  `set_symbol_name`, and a third-party API cannot be resealed. The corrected
  design is stronger: `rename::validated` owns a private `SymbolId → name`
  overlay (an `IndexMap`, per the HashMap ban) plus the trail;
  `attempt_validated_rename` (the seven legality rules, checked against the
  scope tables) is the only writer; every other module reads names through
  one accessor (overlay entry, else the original bound name). Nothing
  mutates the AST or `Scoping` during decision-making — names are applied
  exactly once, at render, via `Codegen::with_scoping` (oxc's own mangler
  works this way: build a renamed scoping, hand it to codegen). The
  enforceable privacy fact is "no module outside `core::emit` ever holds
  `&mut Scoping`", which a wrapper struct with only shared accessors makes
  true.
- **`hash`** exposes three distinct key types — `MatchKey` (literal-blurred
  `structuralHash`), `IdentityKey` (literal-verbatim `statementHash`),
  `VendorSignature` — as separate newtypes (defined in `humanify-model`). A
  correctness gate that consumes a `MatchKey` does not compile. exp046's
  near-miss (vendor byte-reuse keyed on the blurred hash, which would have
  shipped stale endpoints) becomes unrepresentable.
- **`trail`** owns counters and per-item trails, and the tier runner requires
  them. See section 5.

## 3. Concept mapping

| today (TS/Babel)                                        | target (Rust/oxc)                                                                                                                                                                                                                                 |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Binding` object identity                               | `SymbolId` (+ `ReferenceId` for each use)                                                                                                                                                                                                         |
| binding-keyed placeholder hashing over beautified text  | hashing over a canonical serialization (section 4)                                                                                                                                                                                                |
| `FunctionNode` graph (`internalCallees`, `scopeParent`) | same graph, nodes keyed by semantic `NodeId`/`SymbolId`                                                                                                                                                                                           |
| `matchFunctions` cascade + guards                       | same tiers, same order, same abstain semantics — ports verbatim                                                                                                                                                                                   |
| `TRANSFER_PIPELINE` registry                            | closed ENUM of pure tiers — `fn(&NamingState, &Evidence) -> Vec<Proposal>`; a single applier commits proposals into the overlay + trail                                                                                                           |
| `attemptValidatedRename` (7 rules)                      | same rules, checked against scope tables; writes the name OVERLAY (section 2) — the AST is never mutated                                                                                                                                          |
| wave scheduler + barrier apply                          | same design: frozen pre-wave context, batch fan-out, ordered apply                                                                                                                                                                                |
| `@babel/generator` `compact: false` + beautifier plugin | `oxc_codegen` + a small normalization pass (section 4)                                                                                                                                                                                            |
| split slices rendered text by byte offsets              | same technique — spans into the rendered output                                                                                                                                                                                                   |
| `PLACEMENT_TIERS`                                       | same tiers, as a closed enum with exhaustive `match` (all five registries dispatch this way — adding a tier is a compile error until every site handles it; `dyn` is reserved for nothing, and the LLM provider is a GENERIC, not a trait object) |
| kill-switch registry + guard test                       | the env-reading module lives in `humanify-cli`; core receives switches as config, reads `std::env` nowhere (lint-enforced)                                                                                                                        |
| `--diagnostics` trails, stats JSON                      | serde structs, **same JSON shapes** so existing report tooling works                                                                                                                                                                              |

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
breaks artifact compatibility, and the cost is carried by permanent
infrastructure rather than a one-time hop: whenever `tableVersion` bumps, the
next run re-derives the prior's tables from the prior tree
(`12-layout-and-diff.md` §2), and TS-era hashes are never read in production
at all (section 9) (amended 2026-09-19: was "the migration plan (03, phase 5)
pays for it once ... across the transition hop"; 12 §2 wins).

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

The sidecar has since been promoted into something larger: the **version
record** (`12-layout-and-diff.md`) — the same span-keyed tables, plus the
canonical text, serving as the NEXT run's prior-version input. The prior is
never parsed as an AST in production; it is consumed as data. Doc 12 owns
that design (the layout plan, the virtual diff, and the invariants they add).

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
- **Structural fix: the trail requirement moves into the runner/applier.**
  Tier registries (matching, transfer, placement) are closed enums run by a
  generic runner that matches exhaustively and records
  attempt/settle/abstain per item itself; tiers are PURE FUNCTIONS
  (`fn(&NamingState, &Evidence) -> Vec<Proposal>`) and a single applier
  validates and commits proposals into the overlay and trail. A tier
  physically cannot run untrailed and cannot mutate anything — every
  decision takes the one path through the applier — so "a pass with an empty
  trail cannot have moved a KPI" holds by construction, and dead-guard
  incidents (`singletonRejected: 0` read as perfect precision) become
  visible as `unguarded` counters from day one. Retry (collision-rejected
  renames re-attempted as later phases free tokens) needs no shared
  mutability: a later tier reads the updated accumulator and re-proposes
  against new state — ordinary sequencing.

Beyond the HashMap hazard, **decision output must be invariant to the
toolchain** (section 9 states why this is the one compatibility that is
sacred). The ordering rules that guarantee it:

- Every decision-relevant ordering derives from an explicit TOTAL order over
  stable keys — span start/end, then kind, then byte-wise name comparison —
  never insertion order, never iteration order, never allocation order.
  Total keys mean even `sort_unstable` cannot diverge; where a key is not
  provably total, the sort is stable and the code says why.
- String comparison in decision paths is byte-wise, never locale or Unicode
  collation.
- No floating-point values in any hash content or sort key; numeric literals
  enter hashes through the canonical serialization's rules only.
- **oxc's `SymbolId`/`NodeId`/`ReferenceId` are run-local**: deterministic
  for a given source and oxc version, NOT stable across oxc versions —
  usable inside a run, never persisted, never a tie-break that outlives the
  process (spans are the persistent identity, `12-layout-and-diff.md` §2).
- Wall clock, randomness, environment (beyond the kill-switch registry), and
  host filesystem enumeration order never reach a decision; directory reads
  sort before use.

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
- **Memory:** ONE arena, ever — the fresh bundle's. The prior version enters
  as tables + text (`12-layout-and-diff.md` §2), never as a second AST, so
  the old peak model (both multi-GB ASTs live during matching) does not
  exist in this design; the matcher's prior side is table lookups. The
  fresh arena is HELD FOR THE RUN and dropped after the one final render in
  `core::emit` — the render must reflect every settled name (applied via
  the overlay at codegen time), so it needs the AST; everything between
  ingest and emit operates on owned, arena-free tables and never borrows
  from the arena. Carrying one passive ~15–30 MB-bundle AST for the run is
  nothing like the GC retention pathologies the arena exists to end.
  Expected RSS well under the earlier ~2–4 GB estimate versus today's
  15–30 GB under a 64 GB heap.

## 7. The LLM layer

- **The sync/async bridge is one trait at one seam.** `NameProvider` is a
  plain SYNCHRONOUS trait in `humanify-model`
  (`fn run_wave(&self, reqs: Vec<Request>) -> Vec<Response>`);
  `humanify-core` is generic over it and never imports tokio or writes
  `async fn`. `humanify-llm`'s client implements it by owning a
  `tokio::runtime::Runtime` internally and `block_on`-ing a
  semaphore-bounded fan-out. The test mock is a zero-dependency fake
  implementing the same trait — no runtime, no network stack.
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

## 9. Compatibility posture

Made explicit because it changes what the plan does and does not owe
(directive 2026-08-28):

- **There are no external users.** This tool has one operator, and the
  emitted libraries will be regenerated by the Rust pipeline from scratch.
  Therefore: **no TS↔Rust artifact compatibility survives the migration.**
  Everything compatibility-shaped in this plan — byte-compatible prompts,
  cache-key bit-parity, the TS dumper's span conversion, the CLI flag parity
  table — is MIGRATION SCAFFOLDING, existing so the parity gates can compare
  the two implementations, and it is deleted at phase 6 with the TS core.
  The two survivors are the version-record schema (which graduates into the
  production prior-input format, `12-layout-and-diff.md` §2) and the harness
  contract (exit codes, stats shapes, tree layout — kept because the
  TypeScript measurement stack is not being rewritten, not as a compat
  promise).
- **The sacred compatibility is the binary with itself.** Decisions are a
  function of (input bytes, prior version record, config, cache contents)
  and NOTHING else — not the rustc version that compiled the binary, not a
  dependency patch level, not the build profile, not the host. A toolchain
  bump that changes one tie-break silently churns a 124-hop walk; that is
  the failure this posture exists to prevent. Enforcement: the ordering
  rules in section 5, plus two probes owned by `05-rust-toolchain.md`'s bump
  protocol — (a) every toolchain/dependency bump re-runs warm byte-identity
  (self-vs-self across the bump), and (b) a cross-build check: the same
  commit built by two pinned compiler versions (and dev vs release profile)
  must produce byte-identical trees on a warm fixture run before a bump
  merges.
- Post-cutover, flags, formats, and internal schemas evolve freely (fail-loud
  on unknowns, `tableVersion` re-derivation for the record) — the only
  regression tests that constrain them are the pipeline's own gates, not any
  TS-era artifact.

## 10. Idioms

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
