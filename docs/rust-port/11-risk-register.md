# Risk register with early probes

`03-migration-plan.md` closes with a risk table; this document expands it into
an operating instrument, grounded in the target design
(`02-rust-target-architecture.md`) and the parity machine
(`07-differential-validation.md`) that this register cites throughout as
`02 §N` / `07 §N`. The house rule for levers — measure the ceiling before
building (`docs/measurement-pitfalls.md`, rule 11's mechanism-bound) —
applies to risks too: **every known risk gets a cheap probe that sizes it
before any phase can be hurt by it**, each runnable in under a day, stating
its output and the result that would change the plan. Probe results edit
their row IN PLACE, dated, never a newer file (rule 9 — the wrong number
stays in the older document; this one must not become it).

## 1. Summary

**TOTAL: 14 risks.** Eight probes are runnable today, each under a day (R1–R6,
R9, R14). Three attach to the first day of their phase (R8 lands with the
first parallel component, R11 and R12 with phase 1). One is covered by
standing gates and needs no probe (R10). Two are schedule-shaped and their
probe is a milestone (R7 re-probes at every upgrade, R13 at M1).

| #   | risk                                             | likelihood                          | impact                                 | early probe (≤1 day)                                                                 | mitigation                                                                                                                                                                                                                                         | trigger to revisit                                                                                                                   |
| --- | ------------------------------------------------ | ----------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| R1  | Babel↔oxc parse divergence on real bundles      | low                                 | high (blocks phase 1)                  | parse all 8 corpus bundles with oxc today; compare error + top-level counts vs Babel | phase-1 partition gate localizes node-shape drift (03)                                                                                                                                                                                             | any oxc upgrade; new input format                                                                                                    |
| R2  | comment/trivia reliance vs oxc's trivia model    | medium                              | medium (vendor naming, lib detection)  | banner/attachment census over the corpus via the existing TS functions               | regexes port over raw text unchanged; attachment rule becomes a span query; dump regions                                                                                                                                                           | census finds attachment-dependent decisions > 0                                                                                      |
| R3  | UTF-16 vs UTF-8 offsets                          | high — non-ASCII measured in corpus | high, silent (spans slice garbage)     | delta scan per anchored text (run on the corpus for this doc; results in §4)         | 07 §1: dump spans are UTF-8 bytes, converted at dump time, surrogate = fail loud                                                                                                                                                                   | any new artifact gains offsets                                                                                                       |
| R4  | cache-key / canonical-JSON bit-parity            | medium                              | high (warm replay goes cold, rule 10)  | port `keyOf` to a scratch Rust bin; replay real + adversarial vectors                | dedicated JSON writer, not serde defaults (07 §5); zero-write proof at phase 4                                                                                                                                                                     | any new keyed request field; params change                                                                                           |
| R5  | formatter-swap semantics at 5b                   | high (bytes will differ)            | medium one-time — unless semantic      | oxc parse→codegen round-trip on one beautified file; categorized diff + idempotence  | transform inventory: reproduce or consciously drop; cost through rebased priors                                                                                                                                                                    | any transform decision changes; oxc_codegen upgrade                                                                                  |
| R6  | hash porting / ledger continuity                 | medium                              | high at transition (silent mass churn) | census of persisted-hash READERS (write sites already enumerated)                    | re-derivation of the prior's tables from the prior tree is PERMANENT infrastructure, run on every `tableVersion` bump (12 §2); a stale record fails loud, never matches nothing (amended 2026-09-19: was "re-derive ... once (03 5b)"; 12 §2 wins) | any new persisted hash site                                                                                                          |
| R7  | oxc version churn pre-1.0                        | high                                | medium                                 | R1 harness doubles as API canary, re-run per candidate upgrade                       | exact `=x.y.z` pins + `Cargo.lock`; upgrades are scheduled events (05 §5)                                                                                                                                                                          | every upgrade; a needed API missing at the pin                                                                                       |
| R8  | rayon order leaking into decisions               | medium                              | high (draw-dependence returns)         | double-run byte-equality unit gate per parallelized component, planted-red first     | evidence-only parallelism (02 §6); HashMap ban (05 §4); self-hop + warm rerun                                                                                                                                                                      | any new `par_iter` on a decision path                                                                                                |
| R9  | LLM client behavior differences (reqwest vs SDK) | medium                              | low warm / medium cold                 | byte-capture both clients against a local sink; dead-port + 500-stub envelope test   | hand-rolled client (05 §5); typed retry classification; warm gates bypass the client                                                                                                                                                               | endpoint or vLLM upgrade; retry policy edits                                                                                         |
| R10 | webcrack boundary + using-desugar forgotten      | low                                 | low                                    | none needed — R1 confirms `using` parses; fixture + boot gate cover the rest         | subprocess adapter (02 §8); desugar decision at build time; boot gate decides                                                                                                                                                                      | a webpack-format input enters the corpus                                                                                             |
| R11 | plausible-but-wrong agent translation at volume  | high                                | contained by gates                     | port one leaf module via an agent against dumped vectors; count caught divergences   | decision-parity gates (07); PORTING.md gate citations (10 §4); mutants (06 §5)                                                                                                                                                                     | phase-1 divergence rate exceeds review capacity                                                                                      |
| R12 | performance projection wrong                     | low-medium                          | high (business case only)              | R1 yields parse throughput today; phase-1 criterion bench replaces the projection    | 04 labels its soft factors; M1 weighs measured numbers                                                                                                                                                                                             | bench lands >2x off the component projection                                                                                         |
| R13 | two-stacks limbo / morale                        | medium                              | medium                                 | M1 itself, at 25.5% of the surface (10 §5)                                           | phase-2 go/no-go (README); PORTING.md progress line + REMAINING count (10 §4)                                                                                                                                                                      | M1 slips past the 10 §7 forecast band                                                                                                |
| R14 | LLM endpoint dependence during parity            | low                                 | medium (blocks cold events only)       | one warm replay pair with the endpoint at a dead port; success proves server-free    | warm oracle cache; schedule cold events (phase 0, re-warms, 5b, 6) around the endpoint                                                                                                                                                             | endpoint or model change (cache invalidates) (probe run 2026-09-19: PASSED, both legs +0 writes with the endpoint dead — RUNBOOK §5) |

**What REMAINS un-probeable in advance**, by design: live-draw behavior under
the Rust client (only 5b's cold eval sees it), multi-hop feedback where
output becomes the next prior (only the phase-6 walk sees it), and
agent-porting throughput (only phase 1 measures it — 10 §7 re-forecasts at
M1). These are the items `07-differential-validation.md` §11 enumerates as
uncovered by parity; nothing earlier should claim to have sized them.

## 2. R1 — Babel↔oxc parse divergence. Probe: parse the corpus today

Both parsers implement the same spec and the bundles are vanilla minified JS
(03's risk table rates this low), but the whole phase-1 gate sits on the
assumption — so convert it to measurement. **This is a day-one task.** The
corpus is the eval pair inputs — 8 bundles (both sides of 85/86, 118/119,
197/198, 215/216) at
`<inputsBase>/claude-code-<v>/binary-decompiled/src/entrypoints/index.js`
(`experiments/034-eval-harness/run.sh:177,193`; `inputsBase` from
`experiments/034-eval-harness/pairs.json`); verified for this document
(2026-08-28): all eight present, 12–20 MB each.

**Probe.** A throwaway Node script (scratchpad, not committed) using the npm
`oxc-parser` package (`parseSync` — the official napi binding to the same
Rust parser) beside `@babel/parser` from `node_modules`; a throwaway cargo
bin on `oxc_parser` + `oxc_allocator` is the fallback. Per bundle: oxc error
count, Babel vs oxc top-level statement count, parse wall time (the first
hard datum for R12). Two normalizations so the probe does not cry wolf:
**directives** (Babel keeps `program.directives` separate from `body`;
ESTree-style serializations fold them into `body` — compare
`body + directives` on the Babel side) and **`using` declarations** (present
in the corpus — the desugar exists because emitted trees hit them,
`src/split/using-desugar.ts:33-47` — a parse error here is R10 surfacing
early, not noise). Also parse one TS-BEAUTIFIED text (a
`.humanify/humanified.js` under `priorsBase`): that, not the raw input, is
what oxc consumes in phases 1–5a.

**Output:** an 8-row table (errors, count delta, MB/s). **Plan-changing
result:** any parse error or normalized count mismatch is investigated before
phase 1 scaffolding — a day now versus a mysterious partition diff later.
Zero deltas retire the risk to the R7 upgrade canary.

## 3. R2 — comment and trivia reliance

Three distinct dependencies, two cheap and one load-bearing:

- **Raw-text regexes port unchanged.** Banner detection is four regexes over
  raw source (`src/library-detection/banner-patterns.ts:11-23`); region
  scanning runs them `g`-flagged, keyed by `match.index`
  (`src/library-detection/comment-regions.ts:30-42`); the header scan slices
  `code.slice(0, 1024)` (`src/library-detection/adapters/default.ts:37,120-125`).
  No AST comments involved.
- **The offset mix stays consistent.** `classifyFunctionsByRegion` compares
  Babel `node.start` against regex string indices (`comment-regions.ts:93-99`)
  — both UTF-16 units into one string today, both UTF-8 bytes into one string
  in Rust; the hazard lives only in the transition dumps (R3).
- **The one AST-comment dependency is load-bearing.**
  `src/analysis/bun-module-classification.ts:478-551` reads Babel
  `leadingComments`/`innerComments`/`trailingComments` and depends on Babel's
  attachment rule — a comment between two statements attaches to the prior
  one's `trailingComments` (lines 533-535, 544). oxc keeps trivia in a flat
  span-keyed comment table, not attached to nodes: the port must reimplement
  attachment as a span query reproducing Babel's rule exactly, or vendor
  banners classify differently and vendor naming churns.

One harness consequence found writing this register: regions come from the
ORIGINAL pre-beautify text (`src/unminify.ts:110,120-124`) and beautify
STRIPS comments (`src/babel-utils.ts:217-245`, `comments: false`), so a
parity-era Rust leg cannot recompute them — 07 §2's catalog must carry them
(Open questions).

**Probe.** A `tsx` scratch script running the EXISTING TS functions
(`findCommentRegions`, the bun banner classifier) over the 8 bundles.
**Output:** per bundle, region count and how many classifications consulted a
comment attachment. **Plan-changing result:** attachment-dependent decisions
above zero (expected — these bundles go through the bun adapter) confirm the
span-query port is load-bearing and promote banner classifications into the
dump catalog; zero everywhere demotes this risk to the regex port.

## 4. R3 — UTF-16 vs UTF-8 offsets: measured, not hypothetical

What carries JS string indices (UTF-16 code units) today:

| carrier                                                       | where                                                                                                                                                                  | persisted?     |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| `rename-ledger.json` `Span = [number, number]` per occurrence | `src/rename/rename-ledger.ts:29-45,65-66,93`; applied via `String.prototype.slice` (`src/commands/unified.ts:255-281`); its doc comment says "byte range" and is wrong | yes            |
| statement slicing at split                                    | `src/split/stable-split.ts:637-638,1382-1388,1527-1530` (error strings say "byte offsets" — code units)                                                                | no (in-memory) |
| offset splices at CJS emit (~20 sites)                        | `src/split/cjs-emit.ts` (`offsetOf` at 133-136)                                                                                                                        | no             |
| line/column text edits                                        | `src/split/bundle-carry.ts:224-238`, `src/split/post-split-reconcile.ts:137` (0-based UTF-16 columns)                                                                  | no             |
| region classification mixing regex + node offsets             | `src/library-detection/comment-regions.ts:93-99`                                                                                                                       | no             |

Prompts are immune: code windows are line-based with no character offsets
(`src/rename/code-window.ts:215`; 07 §1), so this risk never reaches cache
keys or prompt bytes. The conversion decision is made — 07 §1: all dump spans
are UTF-8 byte offsets, the TS dumper converts with a cumulative table, a
span endpoint inside a surrogate pair fails loud. What this register adds is
the measurement that the identity fast path is already false on real data
(measured 2026-08-28 for this document, `Buffer.byteLength` vs
`String.length` per file): **all 8 minified inputs are pure ASCII**, but
**the prior carry bundle is NOT** —
`claude-code-2.1.215/.humanify/humanified.js` (31.9 MB) runs +69 bytes over
15 non-ASCII lines, real content property keys (`español`, `français`,
`日本語`), zero surrogate pairs. Property names are verbatim hash content
(`01-current-architecture.md` §3), and the carry text is one of the two texts
every dump span indexes (07 §1).

**What breaks silently if the rule is missed:** a span interpreted in the
wrong index space is shifted by the cumulative multi-byte delta at that
point. Slicing does not throw — it ships a shifted identifier or a corrupt
splice, latent on any ASCII prefix.

**Probe (generalize the scan):** one script over every text the dumps anchor
— fresh beautified, prior carry, emitted files carrying `rename-ledger` spans
— reporting the byte/unit delta per file; wire the same check in as 07 §1's
identity fast-path assert. **Output:** a per-text delta table.
**Plan-changing result:** none for the decision (bytes already won); the
nonzero delta makes the conversion table mandatory from day one. One
obligation created: corpus non-ASCII is BMP-only, so the surrogate fail-loud
branch never fires on corpus data — per rule 3 (a guard never seen firing
certifies nothing), a planted supplementary-plane fixture must see it red once.

## 5. R4 — cache-key and canonical-JSON bit-parity

The key is sha256 over `canonicalJson({cacheVersion: 1, params, request})`
(`src/llm/cached-provider.ts:82-106`), where `canonicalJson`
(cached-provider.ts:51-69) is `JSON.stringify` after recursive key-sorting —
`Object.keys().sort()` and `Set` members sorted by JS default sort, both
UTF-16 code-unit order; arrays keep order; `undefined` fields vanish. Three
properties a serde-default port gets wrong: sort order for non-BMP strings
(UTF-8 byte order differs), ECMAScript number formatting, and
`JSON.stringify` escaping rules. 07 §5 specifies the fix — a dedicated
writer, `encode_utf16()` comparisons, an integrality assert on numbers — and
the committed test-vector plan.

**What a silent mismatch does:** every Rust "warm" replay silently goes cold
— misses instead of hits, live calls instead of replays — and the tree diffs
that follow read as decision divergence when they are draw variance: rule
10's inverse failure verbatim (601 differing files from 802 live calls,
`docs/measurement-pitfalls.md` rule 10), aimed at the port's own gates. The
absolute detector is cheap: the replay leg's cache-WRITE count must be zero
(07 §6 tightens phase 5a to both legs +0). A second failure the key cannot
catch: the prompt renders `usedNames` as the first 50 in Set ITERATION order
(`src/llm/prompts.ts:82-85`) while the key sorts the same field — a port
preserving membership but not insertion order produces identical keys and
different prompt bytes; the prompt byte-corpus (07 §5) exists for that class.

**Probe (runnable today, before phase 0):** copy `canonicalJson` + `keyOf`
into a scratch tsx script; emit a JSONL of `(typed request, expected key)`
for real-shaped requests plus 07 §5's adversarial set (non-ASCII identifier,
supplementary-plane character, empty Set, absent optionals, retry round with
`promptBody`); port `keyOf` to a ~100-line scratch Rust bin (`sha2` plus a
hand-rolled writer); assert every key equal. **Output:** N/N equal, or the
first divergent vector with the differing byte position. **Plan-changing
result:** a divergence class not representable from validated UTF-8 (a lone
surrogate in real request text) escalates 07's open question 3 from
theoretical to blocking; otherwise the vectors seed the committed suite and
the risk retires to the phase-4 zero-write proof.

## 6. R5 — formatter-swap semantics at 5b

The inventory to disposition, from `src/plugins/babel/babel.ts:130-143` and
the wrapped `babel-plugin-transform-beautifier` v0.1.1:

| transform                                                                                                                                                | source                                                | disposition default |
| -------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | ------------------- |
| `void <num>` → `undefined`; literal-on-left comparison flipping (`<`→`>` etc.); exponent literals re-emitted long (`5e3` → `5000` via Babel `extra.raw`) | babel.ts:8-72 (57-72 is the only `.extra` use in src) | reproduce           |
| SequenceExpression extraction, EXCEPT for-init/update/test positions                                                                                     | babel.ts:80-128 (patched visitor)                     | reproduce           |
| multi-declarator split, for-init extraction, export splitting                                                                                            | base plugin 16-86                                     | reproduce           |
| block-wrapping for/if bodies (three if carve-outs)                                                                                                       | base plugin 95-99, 284-348                            | reproduce           |
| `a && b;` → `if (a) b;`, `a \|\| b;` → `if (!a) b;`; `!0`/`!1`/`void 0` canonicalization (scope-guarded); return-void split                              | base plugin 125-200                                   | reproduce           |
| conditional expression → if/else at statement positions; `'s'.concat(x)` → template literal + concat folding                                             | base plugin 202-275, 350-423                          | reproduce           |

The default is "reproduce" because several of these are SEMANTIC for the
pipeline, not cosmetic: the multi-declarator split changes the top-level
statement POPULATION — the unit of `statementHash`, placement, and order
(`01-current-architecture.md` §6) — and block-wrapping interacts with the
hash's single-statement unwrapping (`src/analysis/structural-hash.ts:770-788`).
Dropping one changes decisions, and the 5b gate would report it only as
unattributed KPI movement; any conscious drop is recorded (PARITY-FORKS
discipline, 07 §9) with its one-time text cost routed through rebased priors
(regeneration is the score default; `--archive-prior` opts out). Number and
string formatting are the sharp edges: the exponent transform reads Babel
`extra.raw` (oxc's equivalent is the source slice via spans), and anywhere
the Rust emitter prints a NUMBER it must implement ECMAScript
`Number::toString` semantics or copy raw text — Rust float `Display` differs
in exponent thresholds. Self-hop = 0 after 5b (03) requires the new form be a
fixed point: `format(parse(format(x))) == format(x)`.

**Probe (runnable today):** a throwaway cargo bin: oxc parse → `oxc_codegen`
(non-minified) over one TS-beautified corpus file; diff against the input,
categorize divergences (quotes, semicolons, parens, number forms,
indentation); feed the output through again and assert byte-equality
(idempotence). **Output:** categorized divergence counts plus an idempotence
verdict. **Plan-changing result:** if `oxc_codegen` cannot produce a stable
readable form — idempotence fails structurally, or a needed formatting class
is not controllable — 5b's design changes from "codegen + small normalization
pass" (02 §3) to writing our own printer for the beautified form, a scope
change to surface before phase 5 planning, not during it.

## 7. R6 — hash porting and ledger continuity

Today every pipeline hash is `node:crypto`: sha256/16-hex for
`structuralHash` (`src/analysis/structural-hash.ts:921-924`) and
`statementHash` (`src/split/statement-hash.ts:90-114`, `STATEMENT_HASH_VERSION
= 1` persisted as `hashVersion`), sha1/8 for fossil tokens
(`src/split/fossil-assign.ts:172-174`), sha256/8 for the emitted cjs alias
suffix (`src/split/cjs-emit.ts:233`), full sha256 for the cache key
(`src/llm/cached-provider.ts:105`). The 02 §4a decision — canonical
serialization, hash function free (05 §5 picks XXH3) — is safe precisely
because phase gates compare partitions, never hash bytes. Two carve-outs:
the cache key stays sha256 bit-exact (02 §7; R4), and the alias suffix is
emitted INTO code text (07 open question 2 tracks whether its input is
byte-stable path text).

The transition risk is a TS-era persisted hash read by a Rust-era run:
`split-ledger.json` `hashes`/`emitHashes`/`fossilModules[].hashes/.tokens`
(`src/split/stable-split.ts:207-312`) and the bun vendor manifest
`structuralHash` (`src/unpack/adapters/bun.ts:322-330,358-361`, consumed via
`src/unpack/vendor-namer.ts:226`). Re-keyed hashes match NOTHING silently —
the tree looks fresh, layout inheritance dies, the diff explodes as churn.
The designed mitigation is `12-layout-and-diff.md` §2's standing mechanism:
re-derivation of the prior's tables from the prior TREE is permanent
infrastructure, run whenever `tableVersion` bumps and exercised by a test on
every bump, so a stale record fails loud instead of matching nothing — and
under the compatibility posture (02 §9) no TS-era ledger or hash is read in
production at all, because the post-cutover walk starts fresh (amended 2026-09-19:
was "03 phase 5b's one-time transition"; 12 §2 wins).

**Probe:** a half-day reader census — the write sites are enumerated above;
grep out every READER of persisted hash fields (prior-ledger load, vendor
manifest consumers, fossil matching) and add the table to the phase-0
pipeline-contract doc. **Output:** the definitive reader list.
**Plan-changing result:** a reader whose input cannot be re-derived from the
prior tree forces hash-compat mode for that field — the weakened-4a fallback
03 lists among its early falsifiers — far cheaper known in phase 0 than at 5b.

## 8. R7 — oxc version churn pre-1.0

oxc sits under every span and every hash and moves fast pre-1.0. Policy is
set in 05 §5: exact `=x.y.z` pins, committed `Cargo.lock`, and **an upgrade
is a scheduled event** — its own branch, `npm run check` including fixture
parity, the current phase's full parity gate, and post-port a warm neutrality
run plus an eval score, because a parser/codegen bump is exactly the class of
change that can move decisions while looking like a chore.

**Probe:** the R1 harness is the standing canary — built against the exact
pin on day one, re-run against each candidate upgrade to count compile breaks
and output diffs before any workspace crate moves. **Output:** a
breaks-per-upgrade datum measuring churn empirically. **Plan-changing
result:** a needed capability (e.g., codegen formatting control for R5)
missing at the pin moves the pin ONCE, deliberately, before the phase-1 exit
gate — never mid-phase.

## 9. R8 — rayon nondeterminism

The rule is 02 §6: **parallelism generates evidence; a deterministic single
thread spends it.** The failure shape: a parallel reduce whose combine order
depends on work splitting (`fold`/`reduce`), `find_any` instead of
`find_first`, collecting into a `HashMap` and iterating, or float
accumulation order — any of which, on a decision path, re-introduces the
draw-dependence class this project spent incidents hunting down
(`01-current-architecture.md` §7). Standing detectors: the clippy
`HashMap`/`HashSet` ban (05 §4), the self-hop invariant, warm-rerun byte
identity (07 §8).

**Probe (continuous — lands with the first parallelized component):** a
unit-level double-run gate: run the component twice in-process over the
fixture corpus, assert byte-equal serialized output; per rule 3, plant one
nondeterministic reduce and watch it fail once before the green is believed.
**Output:** red/green per component, forever after in `cargo nextest`.
**Plan-changing result:** none — this guards a rule already made; the answer
to "we need an order-free parallel decision" is a sorted merge, never
tolerance.

## 10. R9 — LLM client behavior: reqwest/tokio vs the openai npm SDK

Today's client: the `openai` npm SDK (`src/llm/openai-compatible.ts:1,111-118`),
`POST {baseURL}/chat/completions`, `response_format: json_object`, temperature
0, `max_tokens` 6000 default, timeout 300 s default
(`src/commands/default-args.ts:16`), no streaming, no tools, no seed. Retries
live in the wrapper (`src/llm/rate-limiter.ts:140-177`): up to
`retryAttempts + 1` calls, backoff `1000 * 2^attempt`, retryable decided by
SUBSTRING match on the lowercased error message
(network/timeout/econnreset/econnrefused/"rate limit"/429/500/502/503/504) or
`.status` 429/5xx — and beneath that the SDK applies its own internal retries
(`maxRetries` defaults to 2, `node_modules/openai/core.js:138`), not disabled
at the construction site (openai-compatible.ts:117-122): the effective
envelope today is layered. Port hazards, in decision-relevance order:
**error-string reclassification** (reqwest error text differs — "connection
reset by peer", not "econnreset" — so a verbatim substring port silently
turns retryable failures fatal; classification must move to typed reqwest
error kinds plus status codes); **the hidden inner retry** (dropping the
SDK's layer changes how many attempts a flaky COLD call gets, and a failed
call halves `adaptiveBatchSize`, `src/rename/processor.ts:1146-1148` —
different draws follow); **connection pooling / HTTP version** against vLLM
(throughput only). The exposure is bounded by architecture: the cache wrapper
is OUTERMOST (`src/commands/unified.ts:1032-1058`), so hits never reach the
client and every warm parity gate (phases 1–5a) is immune; only cold events —
5b, the phase-6 walk, re-warms — see client behavior, and the measured bands
absorb draw variance there.

**Probe (runnable today):** (1) byte-capture: point both clients at a 10-line
local HTTP sink that prints request bodies; diff the JSON payloads and
headers for a fixed request set. (2) failure envelope: point both at a dead
port, then at a stub returning 500-then-200; log the attempt timeline.
**Output:** a payload diff plus an attempts/timing table per client.
**Plan-changing result:** if the SDK's inner retry is observed doing real
work against vLLM today, the Rust client reproduces the layered envelope
rather than the wrapper's alone — a behavior decision recorded in the
contract doc before phase 4, instead of discovered inside 5b noise.

## 11. R10 — small boundaries: webcrack and using-desugar

Both are decided (02 §8); the risk is only that they are forgotten, so they
get register rows rather than probes. **webcrack** stays a subprocess
wrapping the npm library (`src/plugins/webcrack.ts:31-32`), selected only for
webpack/browserify inputs (`src/unpack/adapters/webcrack.ts:8-11`); not
exercised by the oracle pairs (07 §11) — parity there is by construction of a
shared dependency, held by one pinned Node helper plus a fixture test.
**using-desugar** today wraps
`@babel/plugin-transform-explicit-resource-management` over emitted tree
files behind a `\busing\b` prefilter
(`src/split/using-desugar.ts:33-47,60-67`); the build-time decision — port
the small desugar or keep the Node scaffold step — is adjudicated by the boot
gate, and the R1 probe confirms on day one that oxc parses the corpus's
`using` declarations at all.

## 12. R11 — Claude-assisted porting: plausible-but-wrong at volume

The port is 44,393 LOC of non-test TypeScript (10 §1) translated largely by
agents. The failure mode: translation that compiles, reads idiomatically, and
decides differently in a corner — at that volume review cannot catch it by
reading, and "the tests pass" is no defense when the tests were translated too.

**Why the decision-parity gates specifically neutralize it:** they compare
DECISIONS, not code — partitions, match sets with tiers, applied AND rejected
renames, prompt bytes, placements, trees (03 phases 1–5a; 07 §§2-6) — under
exact set or byte equality with no tolerance parameter, adjudication
defaulting to "Rust bug" (07 §9). A plausible-but-wrong translation either
changes a decision, which the differ localizes to a stage and a span key, or
changes none — in which case it is, for the product being ported, correct;
PORTING.md enforces the loop socially (no status change without a gate-run
citation, 10 §4). **What the gates do NOT cover**, per rule 8: performance
(R12's bench owns it), panics on inputs OUTSIDE the corpus (fuzzing is a
background instrument, 06 §7 — the corpus is four pairs plus fixtures), code
quality (parity passes on unmaintainable Rust; review still owns that), and
the rest of 07 §11's enumerated box.

**Probe (day one of phase 1):** have an agent port one self-contained leaf
module — the `statement-hash.ts` serialization walk is the natural pick —
against dumped vectors; count divergences caught by the gate versus by
review. **Output:** a divergence-per-module datum plus time-to-green — the
first real velocity measurement. **Plan-changing result:** a high rate means
smaller work packages and per-module review; either way the datum feeds the
M1 re-forecast (10 §7).

## 13. Schedule-shaped risks: R12 performance, R13 limbo, R14 endpoint

**R12 — performance projection.** `04-performance-model.md` labels its own
soft factors: the 5–20×/core native factor and the rayon scaling factor are
mechanism-derived estimates; if wrong, the business case shrinks — nothing
else breaks. **Probe:** the R1 harness measures oxc parse throughput on real
bundles today — the first projected number replaced by a measured one; the
full replacement is the phase-1 criterion bench
(`09-experiment-methodology.md` §5) — parse + semantic + hash at real scale
(64,493 functions, 200,425 bindings on the largest pair,
`01-current-architecture.md` §9), noise floor measured by back-to-back runs
before any delta is read (rule 11). **Output:** measured component times
overwriting 04's projected column. **Plan-changing result:** if the measured
phase-1 stack recomputes the hop projection to under ~2× total, the case
rests on memory, the dev loop, and LLM saturation alone — and M1 weighs that
explicitly (README, "The decision this asks for").

**R13 — two-stacks limbo and morale.** A months-scale port (10 §7: 3–6
months, all estimates) with two stacks alive invites drift and
demoralization if progress is invisible or the end state never forced. The
mitigations are structural: the **phase-2 go/no-go** — phases 0–2 are 11,302
of 44,393 LOC, 25.5% of the surface, M1's exact-match gate the explicit
decision point (10 §5) — and **PORTING.md** as the single visible ledger:
TOTAL/parity-green progress at top, REMAINING at bottom, no status without a
gate citation (10 §4). TS is never maintained alongside — read-only oracle,
then deleted (03 phase 6). The probe IS the milestone: M1 arrives roughly one
to two months in, early enough to abandon cheaply. **Plan-changing result:**
M1 slipping past its forecast band, or the REMAINING line stalling across a
re-forecast period, triggers the honest conversation the go/no-go exists for.

**R14 — LLM endpoint dependence during parity.** The parity era is
deliberately server-independent: cache hits are disk reads returning
zero-spend usage (`src/llm/cached-provider.ts:141-149`) and the cache wrapper
is outermost, so a fully warm replay never contacts
`http://192.168.1.234:8000/v1` (`experiments/034-eval-harness/pairs.json`) —
phases 1–5a gates run through endpoint outages. Cold events DO need the
server and get scheduled around it: the phase-0 cache capture, re-warms after
prompt-changing oracle re-dumps (07 §10 — a warm oracle whose cache predates
its prompts replays nothing), the 5b cold eval with fresh bands, the phase-6
walk, R9's live probes. **Probe (runnable today, ~10 min class — 04's warm
figure):** run one warm replay pair with the endpoint pointed at a dead port.
Success with +0 cache writes proves the parity loop is server-free; any miss
fails loud and enumerates exactly what is not warm. **Output:** a pass, or
the list of unwarm requests. **Plan-changing result:** misses mean the warm
cache is incomplete for the oracle commit — re-warm before parity work relies
on it. The probe doubles as standing discipline: every parity session starts
by checking the write count, never assuming warmth (rule 10's inverse failure).

## Open questions

1. **Comment regions in the dump catalog (from R2).** Regions come from the
   original pre-beautify text and beautify strips comments, so the parity-era
   Rust leg cannot recompute them from the shared beautified text. Add
   `commentRegions` (and bun banner classifications) to 07 §2's catalog, or
   have the binary ingest the original text — ties into 07 open question 1.
2. **Supplementary-plane coverage (from R3).** Corpus non-ASCII is BMP-only
   (zero surrogate pairs measured in the carry text), so the surrogate
   fail-loud branch never fires on real data; the planted fixture must exist
   before the converter's green means anything (rule 3).
3. **The SDK's inner retry (from R9).** Reproduce the layered envelope or
   consciously drop it — decided by the probe's observation against vLLM,
   recorded in the pipeline contract doc before phase 4.
4. **Register upkeep.** Proposed: each phase kickoff re-reads this table;
   probe results edit their row, dated, the day they land — this file stays
   the newest document about its own claims (rule 9).
