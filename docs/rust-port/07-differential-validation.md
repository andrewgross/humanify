# Differential validation: proving two implementations make the same decisions

This is the confidence machine for `03-migration-plan.md`: the concrete
artifacts, join keys, tools, and protocols that turn each phase's "exit gate:
identical X" into something an agent can build and run. The principle is the
one the whole plan rests on — while both implementations consume the same
TS-beautified text (03, "hold the text fixed"), every decision is comparable
**exactly**, and every gate is set equality or byte equality. No gate in this
document is statistical except the single one 03 already designates (phase 5b),
and none has a tolerance parameter — 03 lists the phase-2 gate needing
tolerance as an early falsifier; this document applies the same rule to every
parity gate.

Scope, per the compatibility posture (02 §9): everything in this document is
**migration scaffolding** — it exists so the two implementations can be
compared, and it is deleted at phase 6 with the TS core. The one exception is
the dump schema itself, which graduates into the production version-record
format (`12-layout-and-diff.md` §2) and is designed from the start to serve
both roles.

The harness has five parts:

1. a **TS-side `--dump-artifacts` flag** that serializes the decision record
   per stage (section 3),
2. a **Rust comparer** (`humanify-parity`) that diffs two dumps stage by stage
   (section 4),
3. **prompt and cache-key parity** with byte-exact vectors (section 5),
4. **cross-implementation neutrality** — the phase-5a tree gate, built as the
   smallest possible extension of `experiments/lib/neutrality.sh` (section 6),
5. **eval integration and the existing invariants** (self-hop, boot) pointed at
   the binary (sections 7–8).

Sections 9–11 cover adjudication, the oracle's lifecycle, and — per
`docs/measurement-pitfalls.md` rule 8 — the enumerated list of what parity
does NOT compare.

## 1. The join key: spans in shared text, in UTF-8 bytes

Both dumps describe items in the same two texts: the beautified fresh bundle
and the prior carry bundle (`.humanify/humanified.js` of the previous release),
both read from disk byte-identically by both implementations. Items are
therefore keyed by **where they live in that shared text**:

```
key = (text, start, end)        # half-open span, UTF-8 BYTE offsets
text ∈ {"fresh", "prior", "<tree-relative path>"}   # resolved via meta.json
```

**Decision: all artifact spans are UTF-8 byte offsets.** This is a decision

**Amended 2026-09-19 (WP0.2 author), the anchored-text set — a run has FOUR
texts, not two:** the plan's "the beautified fresh bundle and the prior carry
bundle" premise holds only for the RENAME-era decisions. The split-era
decisions anchor to a DIFFERENT fresh-side text: the split parses the SHIPPED
code (`renameResult.code` — the text after every naming/reconcile/
family-permute pass), which is not the same string as the beautified text
entering the rename plugin (those passes rewrite names, so lengths shift).
Observed on a real 2.1.85 run: a split-era span endpoint 9,027 units past the
beautified text's end. So the anchor set is:

- `fresh` — the beautified text entering the rename plugin (rename-era
  spans: functions, matches, transfers, votes, names, prompt targets);
- `shipped` — the split's input (split-era spans: statementHash family,
  placement, emit);
- `prior` — the prior carry bundle;
- `minified` — the run's original input (regions.json only).

The dump captures each text as it enters its stage and anchors its spans
accordingly; meta.json carries all four sha256s. The Rust leg consumes them
unchanged (`--beautified-input` phase 5a feeds from `text/shipped.js` for the
split stages and `text/fresh.js` for the matching stages).
because it is not what the TS side has natively. Babel `node.start/end` are JS
string indices — UTF-16 code units — and today's code both uses and
_mislabels_ them:

- `rename-ledger.json` persists `Span = [number, number]` taken from
  `node.start/end` (`src/rename/rename-ledger.ts:29-45,65-66,93`) and applies
  them with `String.prototype.slice` (`src/commands/unified.ts:255-281`); the
  doc comment says "byte range" and is wrong.
- The split slices statements by `stmt.start/end`
  (`src/split/stable-split.ts:637-638,1382-1388,1527-1530`) with error strings
  saying "byte offsets"; `src/split/cjs-emit.ts` (~20 sites, `offsetOf` at
  133-136) splices the same way. All are code-unit indices.
- `src/library-detection/comment-regions.ts:93-99` compares regex
  `match.index` string offsets against Babel node offsets — consistent only
  because both are UTF-16 indices into one string.

oxc spans are UTF-8 byte offsets natively, and the dumps outlive the port, so
bytes win. The TS dumper converts at dump time:

```
byteOffset(i) = Buffer.byteLength(text.slice(0, i), "utf8")
```

implemented as one O(n) pass per file building a cumulative table (convert all
span endpoints sorted, never per-span slicing). Two mandatory guards: (a) if
`Buffer.byteLength(text) === text.length` the mapping is identity — assert
this fast path explicitly so ASCII files cost nothing; (b) a span endpoint
that lands inside a surrogate pair is a dumper bug — fail loud, never round.
The one place offsets do NOT leak into prompt content: code windows are
line-based, not offset-based (`src/rename/code-window.ts` splits on `"\n"`,
`MAX_CODE_LINES = 500` at :27), so prompt bytes are unaffected by this
conversion.

Every dump file names the `sha256` of each text it indexes into (in
`meta.json`); the comparer refuses to compare dumps whose anchors differ
(exit 2, section 4). A span without its anchored text is not a key.

## 2. The artifact catalog

One JSON file per stage, `schemaVersion` at the top of every file, all arrays
sorted by `(text, start, end)` of their primary key, all map-shaped data
emitted as sorted arrays of pairs (never object key order). **Twelve files per
pair, plus the anchored texts:**

| file                | stage (01 §2)  | primary key              | content sketch                                                                                                                                                                                                                                |
| ------------------- | -------------- | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `meta.json`         | —              | —                        | oracle commit, flags, input paths, sha256 of every anchored text, schemaVersion of the set                                                                                                                                                    |
| `text/`             | 6              | —                        | the beautified fresh text and prior text themselves (the shared input phases 1–5a consume)                                                                                                                                                    |
| `functions.json`    | 7              | fn span                  | per function: span, kind, name-binding span, `internalCallees` edges (as callee fn spans), `scopeParent` (fn span), per-fn binding list (binding span, name)                                                                                  |
| `partitions.json`   | 7–8            | member span              | three families (`structuralHash`, `statementHash`, `structuralSignature`): for each, member → opaque hash string. Compared as partitions, never as bytes (section 4)                                                                          |
| `matches.json`      | 8              | (prior span, fresh span) | every accepted pair with resolving tier; every rejection with tier + guard (`singletonRejected`, `injectivityDemoted`, `crossedContainerRevoked`, `stillAmbiguous`, `unmatched`); full `ResolutionStats` totals (`src/analysis/types.ts:515`) |
| `transfers.json`    | 9 (mechanical) | binding span             | every APPLIED and every REJECTED rename: old name, proposed name, tier, outcome, machine-readable reason code. Rejections are the guards and must match too (03 phase 3)                                                                      |
| `votes.json`        | 9              | target binding span      | per vote: voted name, witness spans, tally, accepted / below-floor / ladder outcome                                                                                                                                                           |
| `prompts.jsonl`     | 9 (LLM)        | (primary fn span, round) | wave number, round, cache key, byte-exact `systemPrompt` and rendered `userPrompt` — rendered for hits too (section 5)                                                                                                                        |
| `names.json`        | 9 (LLM)        | binding span             | final assignment per LLM-named binding + per-response-entry classification (valid / invalid / duplicate / unchanged / missing per `classifyRenameEntry`, `src/rename/processor.ts:2708-2737`)                                                 |
| `placement.json`    | 10             | statement span           | file + placing tier + `priorFile`/`priorFileFrom` + evidence votes (mirrors `PlacementTrailEntry`, `src/split/placement-trail.ts:116-151`, plus the span)                                                                                     |
| `emit.json`         | 11–12          | (file, slot index)       | per emitted file: ordered statement spans (the bundle-side identity behind the ledger's `emitIndexes`, `src/split/stable-split.ts:246-262`); alias decisions (module → alias, per `aliases`, stable-split.ts:266-272)                         |
| `tree/` or tree sha | 12 + post      | path                     | the final emitted tree (phase 5a compares it directly; earlier phases carry only its manifest of per-file sha256)                                                                                                                             |

**Amended 2026-09-19 by the WP0.2 author (the catalog as implemented):**

- **Thirteenth file, `regions.json`** (00-control §3's recorded decision, both
  ingestion questions): the library comment regions and the Bun CJS factory
  classifications, keyed by span in the MINIFIED original text — the third
  anchored text (`text/minified.js`), so a Rust leg never needs the
  pre-beautify bytes. Per-factory: the minified handle, the factory body's
  span, and the cross-version structural hash; the vendor NAME is not
  re-recorded (the written vendor manifest in the tree carries it).
- **File list as written:** `meta.json`, `text/{fresh,prior,minified,shipped}.js`
  (the fourth text per the §1 amendment above), `functions.json`,
  `partitions.json`, `matches.json`, `transfers.json`, `votes.json`,
  `prompts.jsonl`, `names.json`, `placement.json`, `emit.json`,
  `tree-manifest.json`, `regions.json`.
- **Prompt-row join key:** the dump rows carry both the dispatching
  sessionId (`functionId`) and the dispatch node's declaration spans
  (`targets`); the comparer joins by `(functionId, round)` — sessionIds are
  deterministic for a fixed input, and the folders/sweep/vendor dispatch
  sites have no span at all. `wave` is recorded when the dispatch is
  wave-scheduled and absent otherwise.
- **Serialization:** every JSON file compact (`JSON.stringify`, no indent);
  `prompts.jsonl` is one JSON object per line, in dispatch order (the row's
  `seq` is the stable tiebreak); rows carry raw UTF-16 spans and the writer
  converts to UTF-8 bytes once per anchored text (07 §1's table, with the
  identity fast path and the surrogate guard).

What REMAINS uncovered by this catalog is enumerated in section 11 (the rule-8
box); nothing else in the run is a decision.

Ordering rules are load-bearing: the TS side must not leak `Map` insertion
order or `Set` iteration order into dump bytes, and the Rust side must not
leak `HashMap` randomization (02 §5). Sorting by span at dump time makes both
sides canonical, so `humanify-parity` can byte-compare _keyed rows_ after the
join instead of solving graph isomorphism.

## 3. TS side: `--dump-artifacts <dir>` (03, phase 0 item 2)

Most of the catalog already exists as in-memory structures; the flag makes
them complete, span-keyed, and serialized. Inventory:

| artifact          | exists today as                                                                                                                                                               | gap to close                                                                                                                               |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| functions + graph | `FunctionNode` graph in memory (01 §3); never serialized                                                                                                                      | new dump walk                                                                                                                              |
| partitions        | `byStructuralHash: Map<string, string[]>` (`src/analysis/types.ts:490-491`); statement hashes in ledger `hashes` (`src/split/stable-split.ts:212-223`)                        | dump with member spans, all three families                                                                                                 |
| matches           | `ResolutionStats` counters only (`src/analysis/types.ts:515`); pair-level map exists only as debug-gated `prior-match-map.json` name→name (`src/commands/unified.ts:507-515`) | serialize PAIRS with tier + guard outcomes, span-keyed — the counters stay as totals cross-check                                           |
| transfers         | `StrategyTrailEntry` trails (`src/rename/strategy-trail.ts:59-76`), armed by `--diagnostics` (`src/commands/unified.ts:1127-1134`)                                            | keyed by `oldName` + `loc: "line:col"` today (:61-62); add the binding span (the recorder already keys on the declaration identifier node) |
| votes             | trail outcome `"vote"` entries (strategy-trail.ts:22); placement `evidence.votes` (`src/split/placement-trail.ts:109`)                                                        | dedicated record with witnesses + tally                                                                                                    |
| prompts           | built at call time in the provider (`src/llm/openai-compatible.ts:130-155`); never persisted; debug wrapper logs but is lossy                                                 | render + dump at dispatch for every request, hit or miss (section 5)                                                                       |
| names             | responses live in the processor; `--diagnostics` report has classification buckets (`src/rename/diagnostics.ts:367-390`)                                                      | span-keyed final table                                                                                                                     |
| placement         | `PlacementTrailReport` (`src/split/placement-trail.ts:153-158`), keyed by bundle statement index                                                                              | add the statement span next to `index`                                                                                                     |
| emit + aliases    | ledger `order`/`emitNames`/`emitHashes`/`emitIndexes`/`aliases` (`src/split/stable-split.ts:207-285`)                                                                         | re-emit span-keyed; ledger stays untouched                                                                                                 |
| stats totals      | `--stats-json` (`src/commands/unified.ts:452-501`)                                                                                                                            | none — copied into the dump as-is                                                                                                          |

Two implementation rules:

- **The dump flag must be inert.** Proven in two runs, both using the
  neutrality machinery, before any dump is trusted:
  1. `experiments/lib/neutrality.sh <pre-flag-commit>` with the flag OFF in
     both legs — the standard should-change-nothing case (03 phase 0 item 3),
     warm cache per the standing rule (CLAUDE.md: a cold neutrality run is
     invalid).
  2. Flag-ON inertness: same commit, one leg with `--dump-artifacts`, one
     without, warm shared cache, byte-identical trees. Plain `neutrality.sh`
     refuses same-commit comparisons (`experiments/lib/neutrality.sh:143-146`),
     so this run uses the per-leg-command extension of section 6 — which is
     needed for phase 5a anyway and pays for itself here first.
- **Dump like the diagnostics flag, not like a new pipeline.** `--diagnostics`
  already arms trails without changing behavior (unified.ts:1127-1134); the
  dump flag arms the same recorders plus the new ones and writes at the same
  boundaries the run already has (`writeSplitLedger`, `writeStageHashes`,
  `writeEvalStats` sites in unified.ts). The dump directory lives OUTSIDE the
  output tree so tree diffs never see it.

## 4. The comparer: `humanify-parity`

A Rust crate in the workspace (02 §2) — deliberately the first Rust code
written, exercised against TS dumps before any pipeline Rust exists.

```
humanify-parity compare <ts-dump> <rust-dump> [--stage <name>] [--max-divergences N] [--forks <ledger>]
humanify-parity compare-ledger <a>/split-ledger.json <b>/split-ledger.json
humanify-parity selftest
```

Per stage: verify `schemaVersion` equal and every `meta.json` text anchor
equal — a mismatch is exit 2 (not comparable), because spans into different
texts are not keys. Then:

- **Keyed rows** (`functions`, `matches`, `transfers`, `votes`, `names`,
  `placement`, `emit`): join by primary key; report rows missing left, missing
  right, and value mismatches, in key order.
- **Partitions** (`partitions.json`): hash BYTES differ between
  implementations by design (02 §4a — canonical serialization vs beautified
  text), so equality is over the partition, not the digest. Algorithm: within
  each dump, map every member span to its class representative (the smallest
  member span sharing its hash); compare the two `member → representative`
  mappings for exact equality. Equivalently: equivalence classes as sorted
  lists of sorted member-span lists, compared as nested lists with the hash
  strings ignored. Any member whose class-mates differ is a divergence.
- **Prompts** (`prompts.jsonl`): rows joined by `(primary span, round)`, then
  `systemPrompt`/`userPrompt` compared byte-exact; first differing byte offset
  reported with ±80 bytes of context.

Reporting: the first N divergences (default 20) print the key, both values,
and a source excerpt — the anchored text's bytes at the span, truncated —
so adjudication (section 9) starts from evidence, not from a count. Exit
codes: 0 identical, 1 divergences found, 2 not comparable.

Two standing obligations from `docs/measurement-pitfalls.md`:

- **Rule 3 (a zero from an instrument never seen producing a one is not a
  measurement):** `selftest` ships known-diverged fixture dump pairs — one per
  stage kind, including a partition split and a prompt differing in one byte —
  and asserts exit 1 on each. The check stage runs `selftest` before any
  green `compare` counts.
- **Where it runs:** on committed fixture-scale dumps as a stage in
  `npm run check` — adding a check is one entry in `STAGES` in
  `scripts/check.ts` (CLAUDE.md) — and on the four oracle pairs manually at
  each phase gate. Fixture dumps are small and live in-repo; oracle dumps do
  not (section 10).

`compare-ledger` exists for phase 5a's tree gate: `split-ledger.json` embeds
hash bytes (`hashes`, `emitHashes`, `hashVersion`,
`fossilModules[].hashes/tokens` — stable-split.ts:207-312), as does
`vendor/_bun-modules.json` (`structuralHash` per factory,
`src/unpack/adapters/bun.ts:322-330,358-361`). Those two files are compared
structurally — hash-bearing fields as partitions, everything else byte-exact
— while the rest of the tree is compared as raw bytes (section 6).

## 5. Prompt and cache-key parity

Two separate gates, because they catch different failures.

**Cache keys, bit-exact.** The Rust cache module must reproduce
`keyOf` (`src/llm/cached-provider.ts:82-106`) exactly: sha256 hex over
`canonicalJson({cacheVersion: 1, params, request})` where `params` is
`{model, temperature, maxTokens, reasoningEffort}` with temperature hardcoded
0 at the only construction site (`src/commands/unified.ts:1052-1058`), and
`request` is the sixteen listed fields. `canonicalJson`
(cached-provider.ts:51-69) is `JSON.stringify(sortValue(v))`: arrays keep
order; `Set` → members stringified and sorted by JS default sort (UTF-16
code-unit order); `Map` → object, then key-sorted; objects → keys sorted by
`Object.keys().sort()` (UTF-16 code-unit order again); `undefined` members
vanish. The Rust reproduction is a dedicated writer, NOT serde_json defaults:

- key and set ordering compares `str::encode_utf16()` sequences, not UTF-8
  bytes — the two orders differ for supplementary-plane characters, and
  identifier text comes from arbitrary bundles;
- string escaping matches `JSON.stringify`: `"`,`\`, code units below 0x20
  (short forms `\b \t \n \f \r`, else `\u00xx` lowercase hex; U+007F and the
  C1 range are emitted RAW, exactly as `JSON.stringify` does), non-ASCII
  emitted raw, lone surrogates escaped `\udxxx` (well-formed-stringify
  semantics) — a lone surrogate cannot live in a Rust `String`, so request
  text is carried as validated UTF-8 with an explicit decision recorded if a
  bundle ever presents one (open question 3);
- numbers format per ECMAScript `Number::toString`; every field in key
  material today is a non-negative integer (`cacheVersion` 1, temperature 0,
  `maxTokens`), so the writer asserts integrality and emits digits — if a
  non-integer ever enters key material, that assert fires before a wrong key
  is computed;
- sha256 lowercase hex; shard path `key[0:2]/key[2:]+".json"`
  (cached-provider.ts:108-110).

The test plan: `--dump-artifacts` writes `cache-keys.jsonl` — every request of
an oracle pair as a typed JSON object (with Set-valued fields marked) plus the
TS-computed key. A colocated Rust test reproduces **every** key from the typed
struct, plus a small committed adversarial vector set (non-ASCII identifier,
supplementary-plane character, empty Set, absent optionals, retry round with
`promptBody`). The operational proof then comes for free: phase 4's warm
replay writes zero cache entries only if every live key matched (rule 10's
"live-call evidence is cache writes").

**Prompts, byte-exact — and why key parity alone is insufficient.** The prompt
renders `usedNames` as the first 50 in **Set iteration order**
(`src/llm/prompts.ts:82-85`), while the cache key canonicalizes the same field
by sorting. A Rust implementation that preserves the set's membership but not
its insertion order would produce **identical cache keys and different prompt
bytes** — warm replay green, live behavior changed. The prompt corpus
comparison exists to catch exactly this class. Consequence for the dumper:
with the cache wrapper outermost (`src/commands/unified.ts:1032-1058`), hits
never reach the provider's renderer, so the dump hook renders every request's
prompt itself — the render is a pure function of the request and config
(openai-compatible.ts:130-155: `userPrompt` verbatim if set, else
`buildBatchRenamePrompt`/`buildBatchRenameRetryPrompt`) — hit or miss, in
dispatch order, keyed `(primary span, round)`. Under warm replay the whole
request sequence is deterministic (retry rounds depend only on cached
responses), so the corpus is well-defined and stable.

## 6. Cross-implementation neutrality: the phase-5a instrument

The verdict 03 phase 5a needs — byte-identical trees, warm replay, zero live
calls — is exactly what `experiments/lib/neutrality.sh` already computes, for
two commits of one implementation. Its mechanics today: one `run_leg` function
(neutrality.sh:161-196) whose pipeline line is hardcoded
`npx tsx "$SRCDIR/src/index.ts" ... --llm-cache "$CACHE" --prior-version ...`
(:170-175); candidate leg = the working tree (:199), baseline leg = a detached
worktree of the baseline ref with node_modules symlinked (:202-210); cache
writes counted per leg by `find "$CACHE" -type f | wc -l` deltas
(:148,198-216); verdict fails on nonzero diff lines OR nonzero baseline-leg
cache writes OR mismatched exit codes (:275-278);
`DIAGNOSTIC_ONLY=(placement-stats.json stage-hashes.json)` excluded from the
diff by basename, everything else load-bearing (:243-257).

**The smallest change: a per-leg command.** Add `--candidate-cmd "<argv...>"`:

- when set, the candidate leg replaces `NODE_OPTIONS=... npx tsx
$SRCDIR/src/index.ts` with the given argv, appending the identical pipeline
  flags; the baseline leg is untouched (still the worktree + tsx path);
- the same-commit fatal check (:143-146) is skipped when the flag is set —
  the candidate is no longer a git ref (its provenance is the argv, echoed
  into the run header and the stdout file);
- everything else — cache counting, `.rc` files, tree diff, verdict — is
  already implementation-agnostic: it counts files and compares bytes.

Phase 5a run: candidate = the Rust binary (with its parity-era
`--beautified-input` ingestion so it consumes the TS-beautified text — see
open question 1), baseline = the TS pipeline at the oracle commit, both
replaying the phase-0 warm cache. **Verdict, tightened for this use:**
byte-identical trees, matching exit codes, and **both legs +0 cache writes** —
not just the baseline's zero. In standard use the candidate legitimately
populates (CLAUDE.md: only the baseline leg's zero is load-bearing); here a
candidate write means the Rust leg asked a question the TS run never asked,
i.e. prompt parity (section 5) is broken, and any tree diff downstream of a
live call means nothing (rule 10's inverse failure: 601 differing files from
802 live calls read as catastrophic nondeterminism). The two hash-bearing
files (`split-ledger.json`, `vendor/_bun-modules.json`) are excluded from the
raw byte diff and compared by `humanify-parity compare-ledger` instead — a
visible exclusion, reported in the output the way `DIAGNOSTIC_ONLY` hits are,
because excluding something from a gate is only safe while the exclusion is
visible (neutrality.sh:263-267 makes that argument about its own list). The
provenance sidecar (02 §4b) is the third such surface: sidecar emission stays
OFF during parity legs — it is a Rust-only artifact with no TS counterpart,
so its mere presence in the tree would fail the byte gate — and it enters the
emitted tree only at phase 5b, alongside the formatter swap.

**Why the cache is legitimate here (rule 10).** Rule 10 forbids the cache for
a verdict about LLM-dependent behavior. This verdict is about determinism with
the model held fixed — the same carve-out the existing neutrality gate uses
(CLAUDE.md, "Using the cache here is the use rule 10 permits") — and the
zero-write counts are the proof the replay actually replayed, not an
assumption of warmth. **What the run cannot see, stated in advance per rule
11:** live-draw behavior (how the model would answer under serving-state
nondeterminism), cold-run drift, and multi-hop feedback (the output becoming
the next release's prior). Those are covered later and elsewhere: phase 5b's
cold eval, and the phase-6 walk segment.

## 7. Eval integration

Today the harness cannot point at a different pipeline command: every spawn
site hardcodes `npx tsx .../src/index.ts` —
`experiments/034-eval-harness/run.sh:198` (rebase leg), `run.sh:369`
(self-hop leg), `experiments/lib/run-pipeline.ts:187` (scored leg),
`experiments/lib/selfhop.sh:77`, `experiments/lib/gate.sh:109`,
`experiments/lib/neutrality.sh:170`, and
`experiments/076-statement-placement/walk.sh:76` (the mini-walk driver) —
seven sites (amended 2026-09-19: was six; 09 §1 wins). The change:

- `scripts/eval.ts` gains `--pipeline-cmd "<argv...>"` in `SCORE_FLAGS`
  (eval.ts:128-143), passed through to `run.sh` like the other flags
  (spawn at eval.ts:189-193);
- `run.sh` threads it to its three sites, and `RunConfig`
  (run-pipeline.ts:53-73) gains `pipelineCmd?: string[]` defaulting to
  `["npx", "tsx", "<repo>/src/index.ts"]`; `selfhop.sh` and `gate.sh` take
  the same variable. Existing plumbing survives unchanged: kill-switch
  recording parses child argv (`switchesFromArgv`, run-pipeline.ts:144-156),
  and peak-RSS sampling walks the process tree by PPID
  (run-pipeline.ts:105-131) — a native binary is just a shallower tree than
  the npx-grandchild case the code already documents (run-pipeline.ts:100-104);
- provenance: the manifest records `pipelineCmd` plus the binary's version
  and build commit (the binary must print them; the mixed-commit label guard
  at eval.ts:72-92 extends to refuse mixed-`pipelineCmd` labels the same way).

Expectations by phase:

- **5a:** byte-equal trees on warm replay means the KPIs are identical **by
  construction** — the scorer reads trees, and identical trees produce
  identical numbers. Running `npm run eval -- score` against the binary at
  this point is a contract smoke test (manifests, `-run-status.json`, stats
  shapes), not a statistical gate.
- **5b:** the ONLY statistical gate in the migration. The formatter swap moves
  all text at once, so it is judged the way formatting changes are judged
  today: score with the default REBASE_PRIOR-regenerated bases
  (`npm run eval -- score` regenerates priors by default; `--archive-prior`
  is the opt-out), re-measure the noise bands at the Rust commit with three
  same-commit cold repeats (`npm run eval -- bands <label> <label> <label>`,
  eval.ts:277-323), and judge every delta against those NEW bands — rule 11:
  a gate cannot resolve an effect below its own noise floor and will print a
  confident sign anyway, and the TS-era bands are bands of a different
  instrument once the text moved. `novel`/`realLn` must not move relative to
  the freshly rebased reference. The resulting label becomes the successor
  reference in CLAUDE.md's reference list; TS-era hold columns are not
  comparable across the 5b boundary and are marked historical, exactly as
  `archive-shipped` already is.

## 8. Self-hop and boot gates, unchanged in meaning

Both already drive the pipeline as a subprocess and transfer verbatim through
`--pipeline-cmd`:

- **Boot gate:** `bun run.cjs --version` (and optionally `-p`) against the
  emitted tree, verdict in `<v>-boot.json` (run.sh:329-347;
  `experiments/lib/invariants.ts:159-162`). The Rust binary's trees must boot
  from phase 5a onward.
- **Self-hop idempotence:** re-run over the pipeline's own output must be
  byte-identical, verdict in `<v>-self-hop.json` (run.sh:369-406;
  invariants.ts:164-236), ledger compared byte-wise (`cmp -s`, run.sh:400).
  The standing caveat carries over: the invariant is only reachable warm
  (measurement-pitfalls rule 10 — "the self-hop invariant is unreachable
  cold, for the control too"), so it is run cache-pinned as today. After 5b,
  self-hop = 0 is re-established as the NEW fixed point (03 phase 5b), at
  which point the ledger byte-compare is again exact within the Rust era.

## 9. Divergence adjudication

Every comparer divergence gets exactly one of three classifications, and the
default is "Rust bug" until proven otherwise — the TS side is the oracle:

| class          | action                                                                                                                                                                                                                                                                       |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust bug       | fix Rust, re-run the comparer. No re-dump.                                                                                                                                                                                                                                   |
| TS bug         | fix in TS FIRST, land through the normal gates (`npm run check`, plus neutrality or eval as the change class requires), then **re-dump the oracle at the new commit** (section 10). The oracle must stay truthful; parity against a known-buggy oracle is parity with a bug. |
| conscious fork | record in `docs/rust-port/PARITY-FORKS.md`, then re-dump/re-base downstream expectations.                                                                                                                                                                                    |

`PARITY-FORKS.md` is a ledger, one entry per fork: date, stage, affected keys,
TS behavior, Rust behavior, justification, and which downstream dumps were
re-derived. It also carries a machine-readable block (fenced JSON) listing
`(stage, key)` exemptions; `humanify-parity --forks` reads that block and
suppresses exactly those keys, each suppression printed in the report with its
ledger entry id. Exemptions are never encoded anywhere else — same rule as the
`DIAGNOSTIC_ONLY` list: an exclusion is only safe while it is visible. Expected
fork count at phase 2: zero; the cascade ports verbatim (02 §3).

Two hard rules:

- **No tolerance windows.** Every gate is exact set or byte equality; the
  comparer has no epsilon parameter by design. If a phase gate appears to need
  tolerance, stop and understand the mechanism — 03 lists exactly that need,
  at its phase-2 gate, as an early falsifier, and this document extends the
  rule to every parity gate: an inexact gate is how decision drift gets in.
- **Free-text is not compared; codes are.** Trail `reason` strings are prose
  for humans; the dump carries a machine reason CODE per rejection and THAT
  must match. Prose divergence is not a finding (section 11).

## 10. Oracle lifecycle

- **Pinning.** The oracle is a named main commit, recorded in every
  `meta.json` and tagged (`oracle-<date>`). Dumps from different commits never
  compare (the comparer's anchor check enforces the text level; `meta.json`
  commit equality is checked too). The migration plan's rule stands: `main`
  stays live, and when mainline decisions change, re-dump at the new commit —
  the flag makes that a re-run, not a re-design (03, sequencing).
- **Contents per oracle commit:** dumps for the four eval pairs + fixture-set
  dumps + the warm LLM cache captured at that commit (phase 0 item 4).
- **Size (estimates, labeled).** Total per oracle commit: **~1–2 GB
  uncompressed**; zstd-compressed likely a few hundred MB. The composition —
  anchored texts and trees are the bulk:
  inputs 11.7–19.5 MB, prior bundles 16.8–31.9 MB, largest output tree
  ~93 MB (01 §9, measured). Prompt corpus: 1,598–3,541 calls/pair
  (04, measured) at an estimated 10–50 KB rendered prompt → ~50–150 MB/pair.
  Trails: 200,425 bindings and 35,903 statements on the largest pair (01 §9,
  measured) → an estimated ~50–100 MB of span-keyed rows. Warm cache: entries
  store only responses (`{v, renames, finishReason}`,
  cached-provider.ts:42-48), historically ~24k entries (measurement-pitfalls
  rule 10) → an estimated 50–200 MB.
- **Storage.** Not in git. Canonical copy on the eval host under
  `~/humanify-oracle/<commit>/<pair>/`, mirrored to the laptop as needed.
  The fixture-scale dumps that the check stage consumes ARE in git (small,
  under `test/parity/`).
- **Re-dump procedure.** Warm re-dump (cache unchanged, decisions unchanged
  upstream of prompts) is one warm 4-pair sweep — cheap. If the TS change
  alters any prompt or request field, the cache must be re-warmed first: one
  cold sweep (~81 min today, 04) before the dump, because a warm oracle whose
  cache predates its prompts replays nothing (rule 10).
- **Retention.** Keep the current oracle and its predecessor; delete older
  sets once every phase gate that consumed them has passed at a newer oracle.

## 11. What parity does NOT compare (rule 8 box)

Enumerated in advance, each either covered by another gate or consciously
accepted. Rule 8's lesson is that the unscored surface is where the story
hides.

| not compared                                                                      | disposition                                                                                                                                                                |
| --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| hash BYTES (all three families)                                                   | by design (02 §4a); the partition comparison covers what hashes are FOR                                                                                                    |
| log/stdout prose                                                                  | accepted — EXCEPT `ERROR:` lines, which `-run-status.json` extraction consumes (invariants.ts:49,69-84); their format is part of the pipeline contract (03 phase 0 item 1) |
| trail/rejection prose (`reason` strings)                                          | accepted; machine reason CODES are compared (section 9)                                                                                                                    |
| timing, token, and rate metrics (`avgResponseTimeMs`, tokens/sec, `coverage.llm`) | accepted — hits report zero spend by design (cached-provider.ts:141-149); wall-clock claims belong to `04-performance-model.md`, a different instrument                    |
| peak RSS, provenance fields (`node`, `bun` versions) in manifests                 | accepted; manifest SHAPE is contract, values differ                                                                                                                        |
| `placement-stats.json`, `stage-hashes.json` bytes                                 | diagnostic-only today (neutrality.sh:243); placement DECISIONS are covered by `placement.json`                                                                             |
| live-draw LLM behavior, cold-run drift                                            | covered by phase 5b (cold eval, new bands) — rule 10 forbids covering it warm                                                                                              |
| multi-hop feedback (output as next prior)                                         | covered by the phase-6 walk segment                                                                                                                                        |
| webcrack unpack path (webpack/browserify inputs)                                  | not exercised by the four oracle pairs; stays a subprocess wrapping the same JS library (02 §8) — parity there is by construction of a shared dependency; fixture-only     |
| Electron directory input (`feat/electron-unpacking`)                              | not on the oracle commit's main; enters the oracle when merged, via re-dump                                                                                                |
| `.humanify/humanified.js` and emitted trees                                       | NOT exempt — compared byte-wise by the phase-5a gate (they are load-bearing, neutrality.sh:240-242); listed here only to say so explicitly                                 |

## Open questions

1. **How the Rust binary ingests TS-beautified text in phases 1–5a.** Proposed:
   a parity-era `--beautified-input` flag that skips stage 6 and treats the
   input as already formatted (the dump's `text/` provides it), deleted at the
   5b formatter swap. Alternative: shell out to a TS beautify-only command.
   Needs a decision before phase 1 scaffolding.
2. **Emitted bytes derived from AST hashes beyond the two known files.**
   `split-ledger.json` and `vendor/_bun-modules.json` are handled
   (section 6); `src/split/cjs-emit.ts:233` derives an 8-hex alias suffix via
   sha256 — confirm its input is path/name TEXT (byte-stable across
   implementations) and not AST serialization; if the latter, alias names in
   emitted code differ at 5a and the alias comparison must move from byte diff
   to `emit.json`.
3. **Lone surrogates in bundle text.** JS strings tolerate them and
   well-formed `JSON.stringify` escapes them; Rust `String` cannot hold them.
   No oracle bundle is known to contain one; decide (and record in
   `PARITY-FORKS.md` if it ever bites) whether the Rust side carries request
   text as WTF-8 or fails loud at parse.
4. **Dump determinism of `prompts.jsonl` beyond warm replay.** Dispatch order
   is deterministic under warm replay; a cold dump's retry rounds depend on
   live responses. The oracle is dumped warm by definition — confirm no phase
   needs a cold prompt corpus (none is expected to).
5. **Where fixture-scale parity dumps come from.** Proposed: generate from the
   existing e2e fixture set at oracle re-dump time, committed under
   `test/parity/`; confirm size stays in the low single-digit MB so
   `npm run check` stays ~25 s.
