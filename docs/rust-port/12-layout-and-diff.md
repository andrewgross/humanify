# Layout and diff: the version record, the layout plan, and the virtual diff

**Status: adopted into the target architecture 2026-08-28** — this document
promotes the diff/layout design from discussion to specification. It amends
`02-rust-target-architecture.md` (memory model, sidecar), simplifies one step
of `03-migration-plan.md` (phase 5b ledger continuity), and adds work to
`10-work-breakdown.md` (the dump schema becomes load-bearing). Cross-cutting
compatibility posture lives in 02 §9; this doc assumes it.

## 1. Three principles

1. **One canonical text per version.** The minified input exists only upstream
   of beautify. Everything else — the AST, every span, every hash input, every
   emitted file — indexes into the single rendered text the pipeline produces
   once per version. Emitted files are SLICES of that text (the TS pipeline
   already works this way: `stable-split.ts` slices the rendered bundle by
   statement byte offsets and never re-generates). Formatting is baked into
   the slice, so on-disk position is arithmetic, not rendering.
2. **The prior version is data, never a program.** Run N persists a version
   record — its canonical text plus span-keyed tables (section 2). Run N+1
   consumes the record directly. No prior AST is ever parsed in production;
   the fresh AST is the only AST, in one arena, held until the single final
   render in `core::emit` (the render must reflect every settled name, so it
   needs the AST) and dropped there; every stage between ingest and emit
   consumes owned, arena-free tables.
3. **The final diff is a pure function of decision state**, computable in
   memory before any write. It is used as an INSTRUMENT (section 4) and as
   EVIDENCE ROUTING (section 5), and never as an OBJECTIVE (section 6 — the
   graveyard is measured).

## 2. The version record

What run N writes for run N+1 (and for the `explain` verb, and for the parity
harness — one format, three consumers). All spans are UTF-8 byte offsets into
the canonical text (`07-differential-validation.md` §1); all arrays sorted by
span; `tableVersion` at the top of every file.

| table        | keyed by        | contents                                                                                                                                                                                                                                                                                                        |
| ------------ | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `text`       | —               | the canonical rendered bundle text (today's `.humanify/humanified.js`)                                                                                                                                                                                                                                          |
| `statements` | statement span  | `statementHash`, emitted file, emit rank, placement tier, substitutions applied to the slice                                                                                                                                                                                                                    |
| `symbols`    | decl span       | final name, kind, scope path, exported/module-level flags — plus, for module-level bindings, the BINDING FINGERPRINT: `MatchKey` hash, content shingles, role evidence (the binding cascade and the vote/pin ladders consume these; names alone are not enough)                                                 |
| `references` | ref span        | → owning symbol (by decl span)                                                                                                                                                                                                                                                                                  |
| `functions`  | fn span         | callee edges (fn spans), scope parent, and the FULL fingerprint record — `MatchKey` hash, memberKey, propertyAccesses, externalCalls, close-match feature vector, shingle set (the singleton guard compares fields beyond the hash; a partial fingerprint silently disables it — the dead-guard incident class) |
| `privates`   | occurrence span | class-private `#name` occurrences: name, owning class span (statement-twin private-name pairing consumes these; privates are not bindings in `symbols`)                                                                                                                                                         |
| `vendor`     | file path       | `VendorSignature`, manifest order, factory metadata                                                                                                                                                                                                                                                             |
| `names`      | decl span       | naming outcome: tier that settled it, trail reference, prior-name lineage                                                                                                                                                                                                                                       |
| `layout`     | file path       | header/accessor/import preamble composition, per-file statement ranks, alias decisions                                                                                                                                                                                                                          |
| `meta`       | —               | `tableVersion`, pipeline commit, sha256 of `text`, run config                                                                                                                                                                                                                                                   |

Rules:

- **Spans are the persistent identity; oxc IDs are run-local.** `SymbolId` /
  `NodeId` / `ReferenceId` are assigned per parse and are NOT stable across
  oxc versions — they never appear in a persisted table. Persistence is
  span-keyed; a fresh run joins tables to its own IDs by span when it needs
  to (it mostly does not: the record is consumed as tables, not re-attached
  to an AST).
- **Re-derivation is permanent infrastructure, not a transition hack.** When
  `tableVersion` bumps (hash rules change, a table gains a column), the next
  run re-derives the prior's tables from the prior's `text` — the same code
  path that builds fresh tables, pointed at the prior text. This replaces
  03's one-time "transition hop" framing: it is the standing upgrade
  mechanism, exercised by a test on every bump.
- **This is the same schema as the parity dump** (07 §2). During migration
  the Rust side already consumes TS-produced dumps as its prior input — which
  means prior-as-tables is parity-neutral by construction: phases 1–4 change
  nothing about what the Rust side reads. The schema graduates from
  migration scaffolding to production input format at cutover; it is the one
  piece of the parity harness that is NOT deleted at phase 6.
- **The prior emitted TREE remains an input beside the record.** Vendor
  byte-reuse copies the prior release's vendor files verbatim, and
  post-split reconcile compares against the prior file at the same path —
  both read the prior tree on disk, which is the walk's own output and was
  always required. The record replaces the prior AST, not the prior tree.
- **The sanctioned escape hatch is a micro-parse, never a resident AST.** If
  a pass needs interior structure of one specific prior function beyond what
  the tables carry (e.g. AST-statement-level alignment inside a
  close-matched function), it parses that function's text slice into a
  throwaway arena — microseconds for a 50-line snippet. Whole-bundle prior
  parsing stays forbidden; a scoped micro-parse is a lookup, not a
  regression to the two-AST memory model.
- The content anchor's rare-literal → statement index is BUILT AT LOAD from
  the prior text + statement spans (cheap, derivable); it is not persisted
  unless load-time profiling says otherwise.
- Size: the tables are the sidecar plus the ledger — order tens of MB per
  version against a ~32 MB text (estimate; the current split-ledger alone is
  7–9 MB). Retention follows the walk's needs, not the oracle's (08 §6).

What this deletes from the current design: the prior parse + prior graph
build (53 s + 137 s measured on the big pair — the single largest CPU block
in `04-performance-model.md`'s anatomy), the second arena, and the entire
retained-prior-AST bug class. The matcher's prior side becomes table lookups.

## 3. The layout plan

A first-class in-memory stage output, built after naming and placement
settle, consumed by everything downstream:

```
LayoutPlan {
  files: [ { path, preamble (imports/accessors/header), statement ranks } ]
  statements: statement span → (file, rank, slice offset in file, start line)
  substitutions: span-local edits applied to slices (alias widenings, reconcile renames)
}
```

- **Computation is arithmetic.** Per file: line = preamble lines + Σ line
  counts of preceding slices. Substitutions are applied to slices BEFORE the
  arithmetic, so the plan's positions are the on-disk positions, exactly,
  including formatting — the slice carries its formatting from the one
  canonical render.
- **Emit = write the plan.** The write step becomes the dumbest stage in the
  pipeline: serialize `files` to disk. Source maps, if requested, are
  generated from the plan for free (every slice knows its span in the
  canonical text; chaining to the minified original is one map composition).
- **The tiling invariant.** The plan's slices must partition the canonical
  text's statement spans exactly — every statement in exactly one file, no
  gaps, no overlaps. This is the source-map-flavored restatement of
  concat-equivalence, checked by arithmetic on the plan itself before any
  write, and it converts "a statement mapped into two files" from a
  discovered-on-boot bug into a failed assertion with a span attached.

## 4. The virtual diff, use 1: the in-run scorecard

Given the prior version record and the fresh layout plan, the per-file diffs
the reviewer will see are computable before writing:

- Align per file: statements pair by `statementHash` + name-table lookup
  (the same classification the eval performs post-hoc); unpaired = added /
  removed; paired-but-text-differs = the noise population, with the exact
  substitutions that made them differ attached.
- Outputs, computed in memory on every run and recorded in stats:
  - the noise KPI family (noise statements, noise lines, relocations,
    reorders) as PREDICTED values — byte-equal to what the eval will measure
    on disk, because both read the same pure function;
  - the pre-LLM mechanism counters (statements/bindings that reached the LLM;
    on version hops, the subset that had a prior twin — the matcher-miss
    count). These are upstream of every draw: deterministic, zero noise
    band, measurable without a live endpoint;
  - per-statement attribution: each predicted-noise row links to its sidecar
    row (match outcome including failures, naming trail, placement tier), so
    "why will this hunk exist" is answered before the hunk does.

The real on-disk diff remains the FINAL judge — the eval still reads real
trees with real `diff`, because the gate must see what a reviewer sees
(measurement rule 8). The scorecard is the pipeline's internal copy of that
judgment, not its replacement; a scorecard/eval disagreement is itself a bug
with a span attached.

## 5. The virtual diff, use 2: reconcile on the plan

Post-split reconcile's power is candidate concentration: the file partition
shrinks a name-masked comparison from ~60,000 candidates to ~20. Placement
produces that partition BEFORE emit — so the reconcile tiers (asymmetric,
descriptive-proof, consumer-witness, with the corpus gate and
`skipImportDeclarations` unchanged) run against the prior record's per-file
statement tables, on decision state, before rendering. Corrections become
ordinary validated renames through the one owner, and the text-substitution
machinery (`applySubstitutions`, the same-position splice guard, the
carry/post-split private copies it unified) is retired with the pass that
needed it.

**Migration sequencing (this is the one behavior-adjacent change, so it is
staged):** for phase 5a's byte-identical tree gate, the Rust side ports
reconcile in its text form, matching TS exactly. The plan-based form flips
AFTER cutover as its own change, gated the way any should-change-nothing
refactor is: warm byte-identity (neutrality form) proving the plan-based
tiers ship the identical rename set. Same evidence, same gates, different
plumbing — and the flip is only licensed by that proof.

## 6. What the virtual diff must never become

An objective. This graveyard is measured, and it is the reason this document
exists as a specification rather than an optimization proposal:

- Diff-objective / positional tie-break assignment: **+50,606 noise lines**
  on 215→216. Declaration position does not correspond across versions; the
  residue that survives the evidence tiers is genuinely isomorphic (10 of
  1,420 ambiguous functions recoverable, measured), and a diff-minimizer
  invents identities for it.
- Leftover-ordinal pairing: self-hop violated (36 lines) — position is not
  self-stable across a re-parse.
- The surviving pattern is family-permute's: diff-awareness as a VERIFIER
  that proposes, with strict context gates that dispose (−239 noise lines,
  context-strict). The virtual diff makes that verifier fast and universal;
  the gates do not loosen.

Anchored-locality evidence (unique statement-twins as landmarks, equal-count
interval pairing between them) is the one candidate promotion from this
family, pre-registered as an experiment with its ceiling measured from the
matcher-miss counter BEFORE any build — not part of the port.

## 7. Invariants this design adds

1. **Single-AST invariant**: production runs hold exactly one resident
   whole-bundle AST — the fresh one; the prior is tables + text. A scoped
   micro-parse of a single prior function's slice (§2 rules) is a lookup and
   does not violate this. (The parity phases consume dumps, which satisfy
   this trivially.)
2. **Span-keyed persistence**: no run-local ID (oxc `SymbolId`/`NodeId`) in
   any persisted artifact.
3. **Tiling**: the layout plan partitions the canonical text's statements
   exactly; checked pre-write.
4. **Plan determinism**: the layout plan is a pure function of (decision
   state, prior record); byte-equal across reruns — covered by the existing
   warm-rerun and self-hop gates, which now also compare the scorecard.
5. **Scorecard fidelity**: predicted diff = measured diff on every gated run;
   divergence is a released bug.
6. **The real diff stays the judge**: no gate consumes the predicted diff in
   place of the on-disk one.

## Open questions

- Whether the `layout` table persists derived line numbers or only ranks +
  slice lengths (derivable; persisting them makes `explain` cheaper and the
  record bigger — decide when the record format freezes at WP0.2).
- Whether the scorecard's predicted-KPI block lands in `--stats-json` or a
  sibling file; doc 09's schema ownership decides.
- The reconcile-on-plan flip needs its warm byte-identity proof defined as a
  named gate in 09 once the port reaches it — pre-register it then, not
  retroactively.
