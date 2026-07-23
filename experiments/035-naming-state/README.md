# 035 — Deterministic naming state (kill the classifier, close the census)

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
research-log entries read _Idea → Evidence (table + linked page) →
Conclusion_; outcomes are **landed** or **failed** with numbers;
totals-first tables (TOTAL at top, REMAINING at bottom).

## Why this experiment exists (2026-07-24 findings)

The minted-leftover census said 165 bindings across the eval pairs were
"never named". Per-binding classification (54 on pair 216, via the
strategy trail + diagnostics of the guard probe) showed that is THREE
populations, none of which is "the LLM failed":

| n   | population                                | example                                                   | real cause                                                                                            |
| --- | ----------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| 37  | never entered any naming path             | `do7Function`, `T7Class`, `sm6Factory`, `h1Regex`         | some deterministic pass BUILT a name from a minted stem; the result no longer qualifies for the sweep |
| 9   | statement-twin inherited `_` names        | `_`                                                       | underscore-convention bindings, faithfully inherited                                                  |
| 8   | the LLM's answer, decorated or misflagged | `fsPromises_`, `pathModule_`, `w3cTraceContextPropagator` | collision decoration appends `_` to a GOOD name; `w3c` is a domain stem                               |

Consequence: the guard-idempotence fork (034 RESULTS sub-experiment 8)
was posed against an inflated population. Fix the producers and the
classifier, and the genuinely-cycling set likely shrinks to single
digits per pair — possibly dissolving the fork.

## The work, in order

### A. Census classifier: decorated names are not mints — LANDED (35b304f)

`isBunToken` flags any trailing `_` before `isDecoratedDescriptive`
gets a say. Fix landed at the census-summary layer: `summarizeCensus`
counts collision-decorated names in a separate `decorated` field,
excluded from the minted total (`fsPromises_` decorated, `M2_` still
minted); `isBunToken` itself stays loose because `decoration-retry`
feeds on its candidates (its test caught the first attempt). `w3c`
added to `DOMAIN_STEMS`. Behavior byte-neutral; full check green.

### B. Collision-decoration history — ANSWERED (2026-07-23): the LLM IS retried; `_` is the ladder's last-resort tier

**Idea.** When the LLM's name collides (`fsPromises` taken), the output
shows `fsPromises_` — did we skip an LLM re-ask and decorate instead?

**Evidence** (probe artifacts `/tmp/floor-216-diag.json` + shared LLM
cache + archive bisect; every step verified in code):

| #   | fact                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | where                                   |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| 1   | The bundle holds **276** `require("fs/promises")` sites — Bun flattens every bundled module into one scope, so their module-level lazy-import vars all compete in ONE name domain. **209 distinct `fsPromises*` variants** exist in the final output.                                                                                                                                                                                                                                                                       | probe bundle greps                      |
| 2   | The LLM **was re-asked**: the diag records `Yde → fsPromises_` at round 3, and the cached retry responses (rounds 2 and 3, taken-name context in the prompt) show the model answered `fsPromises` **every time**. No cached response among all 12,219 entries contains a decorated name — the model never produced the `_`.                                                                                                                                                                                                 | `renamed[]` in diag; llm-cache          |
| 3   | After retries exhaust, the deterministic ladder `resolveConflict` allocates, in order: `Val/Var/Ref/Item/Data/Result/Value` suffixes → numeric `2..100` → `_name` → **`name_`** → `local_/inner_` prefixes → numeric `101..999` → `_name_N`.                                                                                                                                                                                                                                                                                | `src/llm/validation.ts:212`             |
| 4   | Under 276 same-stem claimants every early candidate was already taken: `fsPromisesVal`✗ … `fsPromisesValue`✗, `fsPromises2..100` **all** ✗, `_fsPromises`✗ — `fsPromises_` was literally the first free candidate. Batch-mates prove the ladder ran normally: same batch yielded `mimeTypesVal` (tier 1), `fsModule33` (tier 2), `fsPromises_`/`pathModule_` (tier 4).                                                                                                                                                      | probe bundle greps                      |
| 5   | `initialize_` took the OTHER route: its function close-matched and the A2 slot snap reused the **prior version's** decorated name verbatim (content-corroborated `priorNameSnaps` overrides the suggestion). The decoration itself was born generations ago — the `fsPromises_` family entered the archive at the 2.1.133→136 hop — and prior reuse has correctly carried it since (zero diff noise).                                                                                                                       | `prior-name-snap.ts` A2; archive bisect |
| 6   | The [below-floor guard](../034-eval-harness/VOCABULARY.md#below-floor-guard) uses raw `isBunToken`, which flags any trailing `_` — so it REFUSES to transfer these good decorated prior names each hop and sends the binding back to the LLM for a fresh draw. This is the concrete, named root of the guard's draw-dependent self-hop channel (034 sub-exp 8's `initialize_ → noop13`). The A2 snap channel bypasses the guard, which is why `initialize_` survives on close-matched hops but dies on guard-refused paths. | `prior-transfer.ts` `refuseBelowFloor`  |

**Conclusion.** There is no missing retry: the model is re-asked twice
with taken-name context and keeps returning the correct-but-taken stem,
because `fsPromises` IS the right name and the namespace already holds
100+ variants of it. More re-asks cannot mint new information — the `_`
is tier 4 of a 7-tier deterministic ladder doing its job under extreme
same-stem pressure. The real defects are (a) the census counted these
descriptive names as mints — fixed in A — and (b) the GUARD still
treats them as mints and re-rolls them every hop — open. Design
choice posed:

- (i) more LLM re-asks — **failed by evidence** (row 2): the model
  repeats the same stem; extra rounds add tokens and draw-dependence,
  not names.
- (ii) smarter deterministic suffixes (derive from enclosing module,
  e.g. `fsPromisesForCache`) — possible but a new naming surface, and
  allocation would still be order-dependent across versions.
- (iii) **proposed: decorated names are names.** One-line guard
  exemption — `refuseBelowFloor` accepts the prior name when
  `isDecoratedDescriptive(name)` — so the prior's `fsPromises_`
  transfers like any good name and the binding stops re-rolling. This
  directly shrinks the cycling population before E re-poses the fork.
  Build rides task E (it changes naming determinism → needs the probe
  A/B + self-hop gates), red test first.

### C. The `<mintedStem><Word>` producer — ANSWERED (2026-07-23): archive fossils kept alive by the reconcile's weaker gate

**Idea.** Find who manufactures `do7Function`/`T7Class`/`sm6Factory`/
`h1Regex` — suspected our own floor/derivation passes.

**Evidence** (archive bisect + probe trails + code reading):

| #   | fact                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | where                                           |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| 1   | None of the current passes builds them: `class-id-floor` copies outer names verbatim, the split namer and grammar tiers never touch module vars, and no cached LLM response contains any of the names.                                                                                                                                                                                                                                                                | code + llm-cache greps                          |
| 2   | All three true fossils exist in the EARLIEST archived version (2.1.69) — the producer predates the archive (an early-walk LLM draw or a since-deleted heuristic). They are inheritance riders, not products of current code.                                                                                                                                                                                                                                          | archive bisect                                  |
| 3   | This run's [guard](../034-eval-harness/VOCABULARY.md#below-floor-guard) REFUSED them everywhere instrumented: the trail shows exact-match votes carrying `do7Function`/`T7Class`/… and `binding-cascade`/`module-vote`/`module-pin` all abstaining `below-floor-prior-name`. Yet the names are in the output — some pass applied them OFF the record.                                                                                                                 | probe trails                                    |
| 4   | The unguarded door: the **reconcile asymmetric tier**. Its local `isMinifiedName` gate clears any name with a 3-letter lowercase run — `do7Function` ("unction"), `T7Class` ("lass") pass as "deliberate" — so the fossil re-transfers onto the minted fresh binding every hop. Reconcile and the floor passes were the only naming passes NOT recording to the strategy trail, which is exactly why the earlier classification read "never entered any naming path". | `diff-reconcile.ts` `isMinifiedName`; trail gap |
| 5   | Blocking the restore alone would be strictly WORSE: the fresh minified names here (`iIn`, `ACc`, `nng`, `Val` — no digits) are invisible to `isBunToken`, so the binding would keep a raw mint that neither census nor sweep can even see.                                                                                                                                                                                                                            | classifier probe                                |
| 6   | `h1Regex` is not a fossil — it is the LLM's name for the `<h1>` regex (`titleRegex, h1Regex` siblings). Census false positive, same class as `w3c`. The real census dump also surfaced `k8sNamespaces`, `b64Flag`, `u2fModule`, `X509CertificateClass`, `it2ExecutablePath` (iTerm2), `v1PluginData`, `x0Coord` as false positives, and `h06Result`, `j3lResult`, `p1tComponent`-shaped fossils the first regex missed.                                               | census dump on reference 216                    |

**Conclusion.** The fossils are archive inheritance with no living
producer; the reconcile keeps them alive through a weaker mint gate than
the census; and the naming ledger had a blind spot over exactly the
passes involved. Build (branch `exp035-c-reconcile-gate`, commits
e447ad6 + 27bf52b):

- **Sweep the fossils instead of blocking the restore**: `isHalfMintHead`
  (evidenced stem shapes only) makes them [coverage-sweep](../034-eval-harness/VOCABULARY.md#naming-floor--below-floor)
  targets — the reconcile still restores (zero mid-run noise), then the
  deferred sweep LLM-names them once (cache-pinned); the good name
  transfers normally on later hops. One healing hop, then stable;
  self-hop unaffected (a swept output has no fossils left to sweep).
- **Census precision round 3**: domain stems `k8s`/`b64`/`u2f`/`x509`;
  suffix-required stems `h1`-`h6`/`it2`/`v1`/`x0` (word tail required,
  bare stem stays flagged).
- **Post-pass trail instrumentation**: `strategyTrail.recordPostPass` +
  `terminalBy` — reconcile tiers, class-id floor, decoration retry, and
  the coverage sweep now record applies/abstains without tripping the
  post-settle clobber detector. This closes the blind spot and is the
  data feed task D's terminal-state ledger needs.

**Probe numbers** (216, same-session A/B vs main control, shared cache;
[evidence page](pages/probe-c216b-report.html)). The first probe's trail
caught two follow-up defects the ground truth demanded fixing (commit
c4a5c28): reconcile-descriptive was restoring fossils OVER good fresh
LLM names (all 4: `isValidRef→do7Function`, `InputInstance→T7Class`, …),
and the sweep's LLM echoes mint stems (`h06Result→h06CommandResult`)
which would re-flag and re-roll every hop. Final leg with both gates:

| metric                     | control (main) | probe (branch)                                          | read                                                                                                                   |
| -------------------------- | -------------- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| census total (fixed meter) | 33             | **23**                                                  | −30%; every survivor is a named family                                                                                 |
| half-mints in census       | 16             | **5**                                                   | the 5 = refused stem echoes, honest rows                                                                               |
| fossil names in output     | all present    | **0** (h1Regex correctly kept)                          | descriptive fresh names survive                                                                                        |
| noiseLn                    | 8,133          | 10,024                                                  | +1.9k one-time [healing](../034-eval-harness/VOCABULARY.md#healing-hop) (16 renamed bindings' echoes vs archive prior) |
| novel / realLn             | 986 / 122,066  | **986 / 122,066**                                       | byte-frozen ✓                                                                                                          |
| gates in funnel            | —              | 4 half-mint-restore skips, 5 still-below-floor refusals | working as designed                                                                                                    |
| boots                      | —              | `--version` + live `-p` ✓                               |                                                                                                                        |

Remaining census families after this change (23): 9× `_` convention
(twin-inherited, deliberate), 6× `qi_N` mint-stem uniquify tails
(future idea), 5 refused stem echoes (`h06Result`, `j3lResult`,
`p2cValue`, `p2sBytes`), misc smalls (`$root`, `O`, `m`,
`is1hCacheTTL`). The 10 decorated names are task E's population.

### D. Deterministic naming-state ledger (the architecture ask)

Replace shape-classification with ground truth: we ALREADY track
lifecycle per node (pending/transferred/skipped) and per-tier attempts
(strategy trail). Extend to a complete per-binding terminal state:
`named-by-<tier> | inherited | decorated | marked-fine | untouched`,
where **marked-fine** covers deliberate keeps (SHORT*WORDS locals, `*`convention, vendor-skipped) and **untouched** is exact, not inferred.
The census then =`untouched`(+ optionally`decorated`), computed from
bookkeeping, no `isBunToken`needed. Likely shape: the identifierLedger
gains a`terminalState` rollup fed by the trail + floor instrumentation
from C; self-hop + eval gates unchanged. This also gives B its history
for free.

### E. Re-pose the guard fork with honest numbers

After A–C, re-run the guard probe + duo-rebased eval; measure the
actually-cycling population. Then decide (options in 034 RESULTS
sub-experiment 8; standing recommendation: prior-name fallback (b) IF
the set is still material, else keep (a)). Unblocks branches
`feat/catch-and-swaps` (catch params + private swaps — features
exonerated, verdicts contaminated) and re-judges `feat/below-floor-guard`
ordinal tier separately.

## How to run everything (copy-paste)

Probe one pair (same-session A/B; ~15 min cached):
cd /Users/andrewgross/Development/humanify-lever1v2 # branch worktree
NODE_OPTIONS="--max-old-space-size=14336" npx tsx src/index.ts \
 /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js \
 --split --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
 --api-key local --reasoning-effort low -c 32 -o /tmp/probe-X \
 --llm-cache /tmp/eval-work/llm-cache \
 --prior-version /tmp/eval-work/floor-guard-rebased/2.1.215-rebased/.humanify/humanified.js \
 --diagnostics /tmp/X-diag.json
npx tsx experiments/034-eval-harness/analyze.ts /tmp/probe-X/.humanify/humanified.js \
 <prior> /tmp/probe-X/.humanify/split-ledger.json <priorLedger> /tmp/stub-stats.json "2.1.215->2.1.216" # ALWAYS also run a same-session CONTROL from main when judging deltas.

Full eval (new reference; auto evidence pages + self-hop invariant):
REBASE_PRIOR=1 experiments/034-eval-harness/run.sh <label> [workdir]

Reports: `npx tsx experiments/034-eval-harness/trail-report.ts <diag.json> out.html <fresh> <prior> [ledgers]`, then `open out.html`.
Mint census by name: adapt the list-minted snippet (see 034 RESULTS sub-exp 7/8 shell history) or extend trail-report.

Gates before any merge: `npm run check` (READ the verdict, never chain
push), `npx biome check <file>` per touched file, novel/realLn
byte-frozen, boots (`cd /tmp/probe-X && bun run.cjs --version` and a
live `-p`), self-hop for anything touching naming determinism. Work on
branches in ../humanify-lever1v2; never edit main mid-eval/walk;
fixture symlinks are worktree-local (real copies: ../humanify-percache).

## Current state pointers (2026-07-24)

- main green @ 35b304f (task A merged); reference = `floor-guard-rebased`
  (mints 165, self-hop pass was CACHE LUCK — see 034 sub-exp 8).
- Task B answered (see above); its proposed guard exemption builds
  during task E.
- Parked branches: `feat/catch-and-swaps` (blocked on E),
  `feat/below-floor-guard` (ordinal tier, failed separately).
- Evidence pages: ../034-eval-harness/results/family-rotation-ceiling/pages/
- Artifacts that informed this doc (in /tmp, may not survive):
  probe-floor-216 + floor-216-diag.json (the 54-binding classification).
