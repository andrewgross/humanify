# 081 — noise sources: handover for deeper review

> **STATUS 2026-08-20.** Written as a handover. Every number here is measured on
> real trees and every claim carries the command that produced it. Nothing in
> this document is a hypothesis unless it says so.
>
> **Read `docs/measurement-pitfalls.md` before sizing anything.** Six first
> counts were wrong in the two days before this was written, every one a filter
> whose false positives went unread. The pattern that keeps working: state what
> your filter would WRONGLY include, look at a sample of what it caught, and
> only then quote the number.

## The trees to work on

```
/work/exp080-letwalk/          current main, 4 consecutive releases, COLD
  2.1.213  cold start (no prior)
  2.1.214  prior = 213
  2.1.215  prior = 214      <- the CALM pair is 214 -> 215
  2.1.216  prior = 215      <- the BUSY pair is 215 -> 216
```

Regenerate with `experiments/076-statement-placement/walk.sh <workdir>`
(~36 min, no cache, every prompt live).

**Diff them like this.** `index.js` is the whole bundle re-emitted as one file,
so diffing the tree ROOT double-counts everything (253k lines). Diff `src/` and
`vendor/` separately. `vendor/` is 344 lines on the busy hop and is NOT a factor.

```bash
# the build stamp is 82% of a calm release and is pure repetition — exclude it
git diff --no-index \
  -I'VERSION: "2\.1\.' -I'BUILD_TIME:' -I'GIT_SHA:' \
  /work/exp080-letwalk/2.1.214/src /work/exp080-letwalk/2.1.215/src
```

## Where the lines are, measured

| busy hop (2.1.215 -> 2.1.216)               |     lines | tool                          |
| ------------------------------------------- | --------: | ----------------------------- |
| total changed in `src/`                     |    27,022 | scorecard `layout.churnLines` |
| **renaming — same code, different name**    | **5,132** | `layout.nameOnlyLines`        |
| **unchanged code moved between files**      | **1,480** | `080/moved-lines.py`          |
| inlined build stamp                         |     1,296 | `034/build-constant-churn.ts` |
| modules still displaced from their filename |       244 | `080/displaced-names.py`      |
| genuine change                              |   ~19,000 | remainder                     |

| calm hop (2.1.214 -> 2.1.215) | lines |
| ----------------------------- | ----: |
| total changed in `src/`       | 1,571 |
| **build stamp**               | 1,296 |
| everything else               |   275 |
| ...of which renaming          |    46 |

**The calm release is essentially solved.** All the remaining work is the busy
one.

### Two KPIs, and which to trust

- `layout.naming` (statement-level) reads **952** on the busy hop.
- `layout.nameOnlyLines` (line-level) reads **5,132**.

Both are correct. `naming` only counts renames in statements whose hash did NOT
flip; a statement carrying both an edit and a rename is charged entirely to
`real`. **Quote `nameOnlyLines`.** `churnLinesExBuild` is the headline total.

---

# Source 1 — renaming (5,132 lines). THE BIG ONE.

## What it looks like

Provably name-only: identical once local identifiers are masked. Reproduce with
`080/name-churn-where.py`.

```
src/normalize-path.js
  OLD  return nodePath.join(resolvedLinkTarget, ...missingSegments);
  NEW  return nodePath.join(resolvedLinkTargetPath, ...missingSegments);

src/parse-tool-rule/create-http-proxy-server/is-tls-handshake.js
  OLD  onHeaders?.(filteredHeaders, upstreamConfig.hostname);
  NEW  onHeaders?.(filteredHeaders, upstreamEndpoint.hostname);

src/handle-model-consent-flow/is-zero-flag.js
  OLD  children: ["Resets ", resetDateStringDisplay, " · ", ...]
  NEW  children: ["Resets ", resetDateStringValue,   " · ", ...]
```

And one that is actively WORSE, not merely different:

```
src/is-message-valid/is-message-valid.js
  OLD  let toolUseMeta = ...(chatMessage.message.content, tools);
  NEW  let formattedToolUseMessages = ...(lastProgressTimestamp.message.content, tools);
```

`chatMessage` -> `lastProgressTimestamp`. It holds a message, not a timestamp.

## Concentration

Top 25 files hold 57%, top 100 hold 83%. 42% of the mass is in files of
200-999 lines — **not** only the compiler-generated megafiles.

## Why it happens

The model is asked about a name only when we cannot prove what it was called
before. Everything provable is now transferred automatically. Then the model
answers inconsistently: **exp052 measured two cold runs disagreeing on 33.4% of
model-decided bindings by a different word.**

## STRATEGIES ALREADY TRIED — and why each is at its ceiling

Do not re-propose these without new evidence; each was measured this week.

### 1. Transfer names for exactly-matched functions — WORKS, already carrying 91%

59,230 of 64,493 functions (91.8%) inherit wholesale and never reach the model.

### 2. Transfer body locals in CLOSE-matched functions — WORKS, at ceiling

`computeBodyLocalTransfers` (`src/prior-version/statement-align.ts`) aligns
statements on rename-invariant content and bridges placeholder slots.

```
705 close-match pairs
  statement coverage        38,038 / 48,751   78%
  body-local names APPLIED          54,100
  pairs aligning ZERO statements       147   (mostly 1-2 statement functions)
```

**Trap:** `computePartialTransfer`'s comment says body locals "are never
transferred for close matches". That is true of THAT FUNCTION and false of the
pipeline — the call site composes it with `alignment.transfers` two lines below
(`prior-version.ts` ~line 905). Reading it as a statement about the system cost
half a day.

### 3. Post-hoc restore of prior names ("snap") — WORKS, refusals are CORRECT

`src/rename/diff-reconcile.ts`. On the busy hop: **264 restored, 326 refused.**
Refusals broken down:

```
consumer-single-hunk-occ1        152   one lone reference — genuinely too little
decl-not-clean                    91
consumer-to-name-live             25
occurrence-outside-diff           20
rename-rejected:target-in-scope   16
...rest single digits
```

**95% of the largest bucket is a binding with a single mention.** The
two-witness rule is well calibrated. Loosening it trades precision for 8 cases
tree-wide. **CLOSED — do not reopen without new evidence.**

### 4. Prior-name HINTS in the prompt — work better than expected, and that is why the last fix was small

The anchoring fix moved 4,893 bindings from hint to guaranteed transfer and
bought only **~90 diff lines**. The prior name was already in the prompt and the
model was already choosing it. **Converting hints to transfers buys determinism,
not diff reduction.** Any future proposal of the form "transfer instead of hint"
should expect the same, and must size the churn actually happening rather than
the occurrence arithmetic. (I predicted ~5,000 lines from
563 names x ~5 occurrences x 2 lines. That reasoning was wrong.)

### 5. Ideas that DIED on inspection — do not rebuild

- **relax the equal-count guard inside a matched container** — already
  satisfied. Every match lives in one structural-hash bucket, so a matched
  parent is structurally IDENTICAL to its counterpart; its same-hash children
  are equal in number by construction.
- **structural address instead of source ordinal inside a container** — same
  reason: identical structure means address and ordinal agree.
- **import-alias churn** — 28 lines on the busy hop, not the 411 modules a
  first count suggested. The 411 conflated 829 genuine dependency changes.
- **minified free variables** (`bLn` -> `ELn`) — real, ~6 sites tree-wide.
  Now COUNTED (`Unreachable (free refs)` in the coverage summary) and
  deliberately not renamed: rewriting a free reference changes a global lookup,
  and `typeof Bun !== "undefined"` is a feature check whose answer flips.

## WHAT IS ACTUALLY LEFT HERE — the open question

The residual is the model naming **genuinely changed** code differently from
last time. Neither matching nor the safety net is the constraint; both are
measured and near ceiling. Two unsized levers:

1. **Make the model's answer reproducible** for the same question.
2. **Keep the prior name for changed code when it is still defensible**, rather
   than asking at all.

(2) is closer to the stated goal — _"no code shared between versions needs the
LLM to pick names"_ — and carries a different risk: a stale name on repurposed
code. `chatMessage` -> `lastProgressTimestamp` above shows the model sometimes
produces a WORSE name than the stale one would be, which argues for (2); a
genuinely repurposed binding argues against. **Neither is sized. Decide the
risk posture before building.**

---

# Source 2 — unchanged code moving between files (1,480 lines)

Byte-identical lines that still cost a deletion and an addition because they
landed in a different file. Nothing about the code changed.

```bash
python3 experiments/080-noise-sources/moved-where.py \
  /work/exp080-letwalk/2.1.215/src /work/exp080-letwalk/2.1.216/src
```

Concentrated: top 10 file pairs hold ~60%, top 25 ~80%, and **67% move within
the same folder**. Examples:

```
 54 ln  is-plain-object-check-result.js  ->  get-file-info/get-tool-usage-notice.js
 48 ln  is-in-process-teammate.js        ->  exit-process/is-local-observer.js
 44 ln  load-image-processor.js          ->  wsl-path-converter.js
 42 ln  parse-tool-rule/.../process-env-vars.js -> .../process-env-vars-2.js
```

**Status: attributed, NOT diagnosed.** I measured where lines move; I never
established why placement shifts. This is the largest untouched item.

Starting point: `--diagnostics` records a placement trail per statement (see
`docs/pipeline-stages.md` and the `project_placement_trail` notes). Point it at
these specific file pairs and read which tier placed them, rather than reasoning
about placement in general.

**Caveat that bit me:** a first measure put this at 4,320 by pairing identical
deleted and added lines POSITIONALLY, so every `}` and `});` deleted anywhere
and added anywhere counted as a move. Requiring lines to be non-trivial and >=25
chars halved it. Keep that filter.

---

# Source 3 — modules displaced from their filename (244 lines)

A module loses its filename to a newcomer and moves to `X-2.js`, so the whole
file reads as deleted and re-added, plus every importer's path changes.

```bash
python3 experiments/080-noise-sources/displaced-names.py \
  /work/exp080-letwalk/2.1.215/src /work/exp080-letwalk/2.1.216/src
```

Remaining cases, each at **100% export overlap with the prior file and 0% for
the new occupant** — no ambiguity about which module is which:

```
69 ln  handle-post-tool-use-hook-2.js    was handle-post-tool-use-hook.js
53 ln  build-system-prompt-array-2.js    was build-system-prompt-array.js
44 ln  resume-stale-prompt-cancel-2.js   was resume-stale-prompt-cancel.js
```

**Mostly fixed already.** `tierExportSet` in `src/split/fossil-match.ts` matches
modules by export-set overlap (floor 0.6, mutual best) and took this from 1,250
lines to 244 — the largest single win of the week (-1,290 diff lines).

These four are below the floor or blocked by mutual-best. **Unmeasured why.**
Cheap next step: log the near-misses and their scores.

**Do not "fix" `claimPath`.** Inherited paths already claim before fresh ones,
correctly. The defect was always module MATCHING, not name assignment.

---

# Source 4 — the build stamp (1,296 lines, excluded from scoring)

One 8-field build-metadata literal the bundler inlined at **216 byte-identical
sites across 83 files**; three fields change every release. It is correctly
classified as REAL change (the values did change), which is exactly why no noise
KPI saw it.

Excluded from the headline via `churnLinesExBuild`. **The deobfuscator is
deliberately untouched** — Andrew, 2026-08-19: _"we don't need to special case
the version, package url and build time in our code, we can just make sure we
account for it when looking at noise in our experiments."_

Still in the emitted tree. A general "hoist repeated constant blocks" pass would
remove ~3,264 tree lines (101 distinct repeated blocks measured), of which the
build stamp is ~85%. **Not proposed — it changes output, and output stability is
worth more than tree size right now.**

---

# How to validate anything you build

| change type                | instrument                                                                 | pass condition                                 |
| -------------------------- | -------------------------------------------------------------------------- | ---------------------------------------------- |
| counters, refactors        | `experiments/lib/neutrality.sh main <pair> --cache /work/neutrality-cache` | 0 files, 0 lines, **baseline +0 cache writes** |
| anything touching matching | cold walk vs the band                                                      | delta outside the spread, right sign           |
| every change               | `npm run check`                                                            | 8/8                                            |
| matching changes           | `experiments/lib/matcher-preflight.sh`                                     | 4 fixtures unchanged                           |
| **every change**           | `novel` / `realLines`                                                      | **exact — any movement refutes**               |

**The measured noise floor** is
`experiments/076-statement-placement/walk-noise-band.json`: two same-commit cold
walks differ by **35 lines on the busy hop, 32 on the calm**. A delta inside
that is unresolvable. Two repeats give a spread, not a band — treat it as a
lower bound.

**The matcher needs no band at all.** Every matcher counter was identical
digit-for-digit across both repeats, because matching completes before the first
prompt is built. Matcher-level changes can be compared directly. This makes them
far cheaper to evaluate than anything line-level.

**A cached run cannot validate a behaviour change** (pitfalls rule 10). The
diagnostic hops in `079/abstain-hop.sh` use a cache legitimately because they
count decisions made before any prompt; they also turned out to be ~cold anyway
(1,941 live calls against a cold walk's 1,976).

# Suggested order for a fresh pair of eyes

1. **Diagnose the 1,480 moved lines.** Largest untouched item, and a placement
   trail already exists to read. Do not propose a mechanism before reading it.
2. **Size the two renaming levers** (reproducible answers vs keeping the prior
   name for changed code). The risk posture question above is a real decision,
   not a technicality.
3. **The four remaining displacements.** Small, and probably one logged number
   away from being understood.
