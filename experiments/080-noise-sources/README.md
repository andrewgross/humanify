# 080 — where the diff lines actually come from

> **MEASUREMENT, 2026-08-19.** Read off the real emitted trees at
> `/work/p2-walk` (current main). No hypotheses acted on yet.

## How to look at it yourself

```bash
# calm release — small enough to read end to end
git diff --no-index /work/p2-walk/2.1.214/src /work/p2-walk/2.1.215/src

# busy release — the 27,500-line one
git diff --no-index --stat /work/p2-walk/2.1.215/src /work/p2-walk/2.1.216/src | tail -40
```

`index.js` is the whole bundle re-emitted as one file, so diffing the tree ROOT
double-counts everything (253k lines). Diff `src/` and `vendor/` separately.

**Scope check first (pitfalls rule 8).** `vendor/` is 344 changed lines on the
busy hop — the exp046/047 work held, and it is no longer a factor. `src/` is
the whole story now.

## The breakdown

| busy hop (2.1.215 -> 2.1.216)             |   lines | share |
| ----------------------------------------- | ------: | ----: |
| total changed lines in `src/`             |  27,543 |  100% |
| **name-only** — same code, different name |   6,020 | 21.9% |
| **moved between files** — byte-identical  |   4,320 | 15.7% |
| **inlined build stamp** — one fact x217   |   1,296 |  4.7% |
| moved within a file — byte-identical      |     192 |  0.7% |
| remaining, genuine edits                  | ~15,700 |  ~57% |

| calm hop (2.1.214 -> 2.1.215) | lines |   share |
| ----------------------------- | ----: | ------: |
| total changed lines in `src/` | 1,581 |    100% |
| **inlined build stamp**       | 1,296 | **82%** |
| name-only                     |    58 |    3.7% |
| moved between files           |    12 |    0.8% |

**The analyzer's own `naming` metric reports 962 for the busy hop. The real
figure is ~6,020 — 6x.** Consistent with exp055's finding that name-only lines
hide inside "real"; larger than that estimate.

## Four mechanisms, in order of what they cost

### 1. An inlined constant object, copied 217 times

The bundler inlined a build-metadata literal at every use site:

```js
{ ISSUES_EXPLAINER: "...", PACKAGE_URL: "...", VERSION: "2.1.214",
  BUILD_TIME: "2026-07-17T23:24:50Z", GIT_SHA: "e158e55...", ... }.VERSION
```

**217 occurrences across 83 files.** Three fields change every release, so
every release pays ~1,300 diff lines for ONE logical fact. It is 82% of a calm
release and a fixed tax on every busy one.

It is invisible to every noise metric because the values genuinely changed —
this is classified as REAL change, correctly, and is still the largest single
line source in the tree.

**Idea:** detect literal objects that appear byte-identical at N sites and hoist
them to one generated module, imported everywhere. One `build-metadata.js`
instead of 217 copies turns ~1,300 lines/release into ~3. This is un-inlining —
the same move the split stage already makes for code, applied to data.

Sizing needed first: how many DISTINCT repeated literal blocks exist, and how
many sites each has. If build metadata is the only one with a big N, a narrow
fix is honest; if there is a tail, a general rule is worth more.

### 2. A file's name changes, and every importer churns

```
-const srcStripAnsi = require("../strip-ansi.js");
+const stripAnsi2   = require("../strip-ansi-2.js");
```

**411 distinct modules changed their local alias; 1,068 require lines moved.**
And the alias is not just the import line — it is every use site inside every
importing file, which is a large share of the 6,020 name-only lines.

Two sub-causes visible in the samples:

- **collision suffixes.** `strip-ansi.js` became `strip-ansi-2.js` because
  something else claimed `strip-ansi.js`. A NEW module displacing an existing
  one renames a file that did not change at all.
- **vendor path churn.** `vendor/color-name/lib_6687d946.js` ->
  `vendor/color-scheme/lib_3be10b08.js` — hash-derived filename AND folder both
  moved.

**Idea:** an existing file must never yield its name to a newcomer. When two
modules want one name, the one that HELD it last release keeps it and the
newcomer takes the suffix. That is a pure tie-break rule with an obvious
correct answer, and it is cheap.

### 3. Names re-roll around an insertion in compiler-generated code

In React-compiler memo functions, adding one feature shifts every cache index
after it and inserts a variable into a long `let` list. The surrounding names
then re-roll:

```
-      isDeferredMcpRequestPresent = categories.some(isDeferredMcpRequest);
+      containerBox = categories.some(isDeferredMcpRequest);
-      columnCache = getColor.CacheStore;
+      hasAnyToolsFlag = getColor.CacheStore;
```

Note `containerBox` is also a WORSE name — it holds a boolean. The insertion
did not change what these variables do; it changed their surroundings enough
that matching lost them.

**Idea:** these functions are huge, highly repetitive, and machine-generated.
Their variables are anchored by the `cache[n]` slot they read and write, which
survives renumbering as a RELATIVE order. Matching within one of these
functions by cache-slot order rather than by surrounding context is a narrow,
checkable rule. Size it first — count the files with this shape and the names
that re-roll in them.

### 4. Unchanged code moving between files

**4,320 lines are byte-identical and still appear as a delete plus an add,
because they moved to a different file.** Nothing about them changed. This is
placement, not naming, and it is 15.7% of the busy diff — the second largest
source.

Worth noting against the fossil work: module identity is inherited well, but
these are STATEMENTS moving between modules, which the module-level match does
not constrain.

**Idea:** the placement trail already records why each statement landed where
it did. Point it at these 4,320 lines specifically and read the tier that
placed them, rather than reasoning about placement in general.

## What I would do next, and why in this order

1. **The collision-suffix rule (#2).** Smallest, most obviously correct, and
   a matcher-level change — which the band runs proved needs no statistical
   allowance, so it is cheap to judge.
2. **Size the repeated-literal hoist (#1).** Biggest single number in the tree
   and it dominates calm releases outright. Sizing is a census, not a build.
3. **Attribute the 4,320 moved lines (#4).** Read the existing trail before
   proposing anything.
4. **The compiler-generated-function idea (#3) last** — most speculative, and
   the population needs counting before it earns code.

---

## The two lines Andrew picked out of the calm release — diagnosed

Both were real noise. They turn out to be **different mechanisms, and neither
is a lever.** Worth writing down precisely because the instinct was right and
the sizing says don't build.

### `cr_2` -> `unusedParameterPlaceholder` — LLM nondeterminism

```
-  let finalFallbackModel = cr_2 ?? silentRearmFallback;
+  let finalFallbackModel = unusedParameterPlaceholder ?? silentRearmFallback;
```

The logs show **both runs asked the model about this identifier, with a
byte-identical prompt** — same identifier, same 50-name avoid-list. In 2.1.214
it came back unnamed and stayed `cr_2`; in 2.1.215 it came back
`unusedParameterPlaceholder`.

So this is not a matching failure, a coverage gap, or a rule. It is the known
re-roll floor: exp052 measured two cold legs disagreeing on **33.4% of
LLM-decided bindings by a different word**. Nothing in the matcher can fix it,
and it is the one class where the churn is genuinely an improvement — a
leftover got named.

Two observations, the second speculative and flagged as such:

- the emitted name is BAD: `unusedParameterPlaceholder` holds
  `resolvedFallbackModel.model ?? fallbackModelCandidate`, which is not unused
  and not a placeholder.
- the avoid-list handed to the model contains `unusedParam1`, `unusedParam2`,
  `unusedParam3`. It is plausible the list primed the word. NOT MEASURED —
  worth a probe before anyone believes it.

### `bLn` -> `ELn` — a free variable the renamer cannot reach

```
-  childModule = moduleObjectVal && typeof bLn == "object" && bLn && !bLn.nodeType && bLn;
+  childModule = moduleObjectVal && typeof ELn == "object" && ELn && !ELn.nodeType && ELn;
```

`bLn` is declared nowhere in that file. It is a FREE reference — lodash's
`freeModule` idiom, where it was originally `module`. Renaming operates on
resolved BINDINGS, so a reference with no binding is invisible to it; and
`mintedCensus` counts bindings too, so it never showed up as a leftover either.
The minifier redraws the letters each release and the line churns forever.

**Sized, and it is tiny** (`free-variable-census.ts`, 1,500 files of 2.1.215):

| unresolved names that are not real globals | 9 distinct |
| ------------------------------------------ | ---------: |
| ...that look minified                      |      **2** |

`ELn` and `_0n`. The other seven are `DOMException`, `SuppressedError`,
`TextDecoderStream`, `WebSocketPair` and friends — legitimate globals.
Extrapolated tree-wide: roughly 6 sites.

**Verdict: real, correctly identified, ~6 sites. Do not build for it.** Worth
recording so nobody re-derives it; the census script stays so the number can be
re-checked if the bundler changes.

The first pass of this census reported 16 files, mostly `Bun`, `btoa`,
`crypto`, `Blob` — a globals list that was not complete enough. Widening the
list took it to 2. A filter's false positives have to be read before its count
is believed.

---

## Why `cr_2` got a bad name: the model could not see it

`unusedParameterPlaceholder` holds `resolvedFallbackModel.model ??
fallbackModelCandidate` — neither unused nor a placeholder. The avoid-list
priming theory was wrong. The actual cause is worse and checkable:

**The identifier is not in the code the model was shown.** In that prompt the
fenced block is exactly 500 lines — `MAX_CODE_LINES` — with no elision markers,
and `cr_2` (declared at line 2764 of its file) appears nowhere in it. The model
was asked to name a symbol it could not see, so it invented one. That name then
churns against whatever the previous release invented.

exp015 replaced flat truncation with declaration-anchored windows precisely to
stop this. `prompt-blindspot.py` checks whether the guarantee holds:

| cold-start hop (2.1.213), 50,897 prompts |                  |
| ---------------------------------------- | ---------------: |
| identifiers asked about                  |          173,342 |
| **not present in the code shown**        | **1,590 (0.9%)** |
| ...of those, at the 500-line cap         |                4 |

So the cap is NOT the main cause — `cr_2` is in the rare 4. The other 1,586 are
identifiers absent from a block well under the cap, which is a second and
currently unexplained gap. Both produce a name invented blind.

**This number was 1.5% on the first run, and the top "missing" names were `$e`,
`$t`, `$u`.** They were never missing: `\b` is not an identifier boundary in
JavaScript — `$` is an identifier character but a NON-word character to the
regex engine, so `\b$e\b` cannot match a real `$e`. Fixed to an explicit
`[A-Za-z0-9_$]` lookaround. Fourth time in two days a first count came out too
high; the pattern is always a filter whose false positives were not read.

---

## CORRECTIONS, 2026-08-19 — two of the four mechanisms were mis-sized

Both were caught by checking a claim before building on it. Recording the wrong
numbers next to the right ones, because the wrong ones were published here.

### Idea 2 (import-alias churn) is 28 lines, not 411 modules

The evidence for it was one diff line:

```
-const srcStripAnsi = require("../strip-ansi.js");
+const stripAnsi2   = require("../strip-ansi-2.js");
```

read as "an existing file yielded its name to a newcomer". **It did not.**
2.1.216 keeps `strip-ansi.js` for the same module (same exports plus one new
one) and gives `strip-ansi-2.js` to a genuinely different module. The rule under
suspicion — inherited paths claim their names before any fresh module — was
already correct, and `claimPath`'s comment already said so.

Measured properly (`require-churn.py`), splitting changed requires by whether
the PATH moved or only the local ALIAS:

| busy hop                         |  count |                   |
| -------------------------------- | -----: | ----------------- |
| same path, different local alias | **28** | noise, ours       |
| requires removed (path gone)     |    341 | dependency change |
| requires added (path new)        |    488 | dependency change |

The 411 figure counted every changed require line, conflating 829 genuine
dependency changes with 28 lines of alias churn. **Not a lever.**

The 28 do show a real mechanism — an alias gains a path-derived prefix when a
second import would collide with it (`matchesCommandName` ->
`srcMatchesCommandName`) — so the "existing yields to newcomer" shape is real at
the ALIAS level. It is just tiny.

### Idea 4 (cross-file movement) is 2,416 lines, not 4,320

The first measure paired identical deleted and added lines positionally, which
counts every `}`, `});` and `return;` deleted anywhere and added anywhere as a
move. Requiring a line to be non-trivial and at least 25 characters before it
can be evidence of movement:

| busy hop                                            | git lines |
| --------------------------------------------------- | --------: |
| substantial identical lines moved **between files** | **2,416** |
| within a file                                       |         0 |

Still the largest verified placement cost, and still worth attributing — just
1.8x smaller than reported.

### Corrected ranking

| busy hop, 27,543 changed lines | lines | share |
| ------------------------------ | ----: | ----: |
| name-only renames              | 6,020 | 21.9% |
| moved between files            | 2,416 |  8.8% |
| inlined build stamp            | 1,296 |  4.7% |
| import-alias churn             |    28 |  0.1% |

**Six first-counts have now been too high in two days, every one a filter whose
false positives went unread.** The rule that keeps working: state what the
filter would wrongly include, then look at samples of what it caught, BEFORE
quoting the number.

---

## Idea 2, RE-OPENED and properly sized: modules displaced from their filename

I closed this an hour ago on one sample. The sample was real and the conclusion
was wrong — `strip-ansi` happened to be a case where the rule works. The
largest cross-file move in the tree is a case where it does not.

### The mechanism, proven by export sets

| file                                       | exports                                                                |
| ------------------------------------------ | ---------------------------------------------------------------------- |
| 2.1.215 `pr-review-artifact-template.js`   | `loadTemplateModule`, `skillRegistryRef`                               |
| 2.1.216 `pr-review-artifact-template-2.js` | **the same two** — the module, displaced                               |
| 2.1.216 `pr-review-artifact-template.js`   | `loadWorkshopTemplateModule`, `workshopTemplates` — a different module |

The prior module was pushed to `-2` and a newcomer took its name.

### Sized (`displaced-names.py`, busy hop)

|                                                         |       |
| ------------------------------------------------------- | ----: |
| suffixed files in the fresh tree                        |    10 |
| **of those, DISPLACEMENTS** (prior module pushed aside) | **5** |
| lines held by displaced modules                         | 1,250 |

Every one at **100% export overlap with the prior file, 0% for the new
occupant**. There is no ambiguity about which module is which.

Cost is roughly double those lines in git terms (the whole file reads as
deleted and re-added) plus every importer's require path — and it accounts for
the top cross-file move at 916 lines on its own.

### Why it happens, and where the fix belongs

Not in `claimPath`. Inherited paths already claim before fresh ones, correctly.
The prior module **failed to MATCH**, so it had no inherited path to claim and
fell back to deriving a name — by which time a newcomer held it.

And it failed to match for a knowable reason: `FossilSignature` is
**rename-blind by design**, keyed on sorted statement hashes. This module grew
653 -> 1,006 lines, so its statement overlap is weak. Its EXPORT NAMES, however,
are a perfect match — because splitting runs AFTER naming, so exports on the
fresh side already carry names inherited by the function matcher.

**The lever: an export-set tier in the module matcher.** Function-level matching
already succeeded here (that is why the names carried); module-level matching
did not, and the evidence it needed was sitting in the emitted output.

Open question before building: the prior side's exports must be available at
match time. Fresh-side exports are; whether the ledger carries prior exports, or
they must be derived from the prior bundle, decides the size of the change.

### Cross-file movement, attributed

| busy hop                |   git lines |
| ----------------------- | ----------: |
| cross-file moves, total |       2,446 |
| top 10 file pairs       | 1,470 (60%) |
| top 25 file pairs       | 1,966 (80%) |
| same folder             | 1,640 (67%) |

Highly concentrated, and the largest single pair is the displacement above. So
ideas 2 and 4 are substantially the SAME defect seen from two directions.

---

## Experiment 3 — why prior names are not reused: the skip breakdown

The prior-name snap pass reported "250 snapped, 357 skipped" with no breakdown.
Instrumented, on a real hop:

```
264 snapped, 326 skipped:
  consumer-single-hunk             160   49%
  decl-not-clean                    91   28%
  consumer-to-name-live             25
  occurrence-outside-diff           20
  rename-rejected:target-in-scope   16
  consumer-from-not-novel            4
  consumer-name-conflict             3
  disagreement / name-downgrade /
  rename-rejected:shadows-child /
  reroll                             7
```

**Two reasons are 77% of it.** Whether they are sound refusals or a reachable
lever is the next question, and it is now a specific one about two named rules
rather than a number with no explanation.

Note the reason strings are not literals in `diff-reconcile.ts` — they are
composed elsewhere. Finding where is the first step.

## Experiment 2 — the export-set tier, BUILT

`FossilSignature` gains an optional `declared` field (the module's export
names) and the matcher a `tierExportSet` rescue tier: mutual-best on export-set
Jaccard with a 0.6 floor, running before the weaker content tiers.

**Why name-bearing evidence is legitimate here**, when every other field in that
signature is deliberately rename-blind: splitting runs AFTER naming, so a module
whose functions the FUNCTION matcher already paired carries its prior export
names on the fresh side. That is the only condition under which the floor can be
cleared. Churned names score low and the tier stays silent — it cannot invent a
match, only rescue one where identity is already evidenced.

Ledger carries `declared` per module. Optional, so a ledger written before this
gets the pre-exp080 behaviour and never a wrong answer — the `tokens` contract.
Cheap too: a handful of short names per module, against `tokens`' 8 MB.

Red/green on the real case: a prior module with `["loadTemplateModule",
"skillRegistryRef"]` grown from 3 to 8 statements must keep its identity, and
the newcomer holding the base name must not inherit it.

| gate                   | result                           |
| ---------------------- | -------------------------------- |
| `npm run check`        | GREEN 8/8                        |
| `matcher-preflight.sh` | 4/4 fixtures unchanged           |
| cold walk vs band      | **OWED** — this changes matching |

Expected effect, sized before the run: 5 displaced modules, 1,250 lines of
module, ~2,500 git lines plus importer paths. If the walk shows less, the tier
is not firing on the cases it was built for and the floor or the mutual-best
rule is wrong.

### Experiment 3 verdict: the refusals are sound. NO LEVER.

The hypothesis was that `consumer-single-hunk` (160 of 326 skips, the largest
refusal) under-counts evidence, because a hunk is a contiguous diff REGION and
several distinct references can sit inside one.

Measured by bucketing the refusal on actual occurrence count:

| bucket                                           |   count |
| ------------------------------------------------ | ------: |
| `consumer-single-hunk-occ1` — one lone reference | **152** |
| `consumer-single-hunk-occ2`                      |       5 |
| `consumer-single-hunk-occ3plus`                  |       3 |

**95% are genuinely a single reference.** The proxy is not hiding evidence; the
two-witness rule is well-calibrated, and relaxing it would trade precision for
almost nothing. Only 8 cases tree-wide have more evidence than the hunk count
implies.

So the snap pass is not the problem: 264 snapped, and the bindings it refuses
mostly cannot be identified from one witness. **Closed.**

That leaves `decl-not-clean` at 91 as the second bucket — small, and untested.

---

# RESULTS — experiment 2 (export-set tier): SHIPPED

Cold walk at `b6340fb` against the measured band
(`experiments/076-statement-placement/walk-noise-band.json`, two same-commit
cold repeats).

## Gate table

| gate                   | result                                     |
| ---------------------- | ------------------------------------------ |
| `npm run check`        | GREEN 8/8                                  |
| `matcher-preflight.sh` | mitt / nanoid / preact / zustand unchanged |
| cold walk, 4 hops      | all exit 0                                 |
| `novel` / `realLines`  | **exact on both hops** — nothing real lost |

## Diff lines, against the measured spread

| hop  | metric        |      band range |  candidate |      delta |
| ---- | ------------- | --------------: | ---------: | ---------: |
| calm | churnLines    |   1,557 – 1,589 |      1,561 |     inside |
| busy | churnLines    | 28,403 – 28,438 | **27,113** | **−1,290** |
| busy | real          | 22,873 – 22,930 |     21,281 |     −1,592 |
| busy | fileAddRemove |           4,494 |      4,824 |       +330 |
| busy | noise         |   1,014 – 1,036 |      1,008 |     inside |

**−1,290 against a spread of 35 — 37x the noise floor.** The largest single
improvement measured in this arc, and `novel`/`realLines` byte-identical, so no
real change was traded for it.

## The mechanism fired on the case it was built for

|                                 | before |   after |
| ------------------------------- | -----: | ------: |
| displaced modules               |      5 |       4 |
| lines held by displaced modules |  1,250 | **244** |

`pr-review-artifact-template.js` — the 1,006-line module that grew 653→1,006,
lost its filename to a newcomer and moved to `-2`, costing 916 git lines as the
largest cross-file move in the tree — now keeps its identity.

Four small displacements remain (78, 69 lines and two smaller). They are the
same shape and presumably below the 0.6 export-overlap floor or blocked by the
mutual-best rule; unmeasured, and the obvious next thread.

## Prediction vs outcome, stated in advance

Predicted ~2,500 git lines (1,250 lines of module, roughly double in git terms,
plus importer paths). Actual −1,290 — same order, about half. The prediction
double-counted: a displaced module's lines are not all charged twice, because
the destination file's content still matches statement-wise in places.

---

# Why shared code still reaches the model — the close-match gap

Andrew's goal, 2026-08-19: _"no code that is shared between the versions needs
the LLM to pick the names."_ This is the measurement of how far from that we
are, and exactly what blocks it.

## Where the model is used on a busy hop

| functions                         |  64,493 |                                                      |
| --------------------------------- | ------: | ---------------------------------------------------- |
| cached (inherited whole)          |  59,230 | 91.8% — never touched the model                      |
| **close match**                   | **705** | **1.1% — paired with a prior function, but CHANGED** |
| LLM (genuinely new)               |   1,109 | 1.7%                                                 |
| already named / nothing to rename |   4,153 | 6.4%                                                 |

| identifiers renamed | 5,121 | of which **5,083 chosen by the model** |

705 close-matched functions, ~7 bindings each, ≈ the 5,083. **Close matches are
essentially all the naming noise**, and they are all shared code: we know
exactly which prior function each one is.

## What a close match transfers today

`computePartialTransfer` carries over exactly two things:

1. the function's own name, and
2. parameters, **by index**.

Body locals are transferred never, and the comment says why: _"Body locals can
shift when statements are added/removed, so they are never transferred for
close matches."_

Measured over 9,215 functions of a real tree:

| bindings a close match CAN transfer      |           |
| ---------------------------------------- | --------: |
| function names                           |     1,614 |
| parameters                               |     5,448 |
| **bindings it CANNOT — go to the model** |           |
| **body locals**                          | **6,238** |

**Body locals are 46.9% of all function-scoped bindings.** Per function: p50 0,
p90 2, p99 11 — so they concentrate in the large functions, which is precisely
where the churn concentrates too.

## What else was tried, and how well it works

- **hints** — the prior name is put in the prompt, never applied. The model may
  reuse it or pick a synonym; exp052 measured 33.4% disagreement between two
  cold runs on model-decided names.
- **snaps** — a post-pass restores prior names where the definition still
  corroborates. Measured: **264 restored, 326 refused.** The refusals are sound:
  152 of the largest bucket are bindings with a single lone reference, which is
  genuinely too little evidence (experiment 3, closed above).

So the post-hoc safety net is working near its limit. The headroom is not in
relaxing it — it is in never needing it.

## The fix that follows from this

Body locals are refused because the only alignment method tried is POSITIONAL,
and position shifts when statements move. But position is not the only
name-free way to align two bindings.

**Align body locals by their DECLARATION STATEMENT's rename-blind hash.** Inside
a pair of functions we have already matched, a statement whose rename-blind hash
is identical on both sides, and unique on both sides, declares the same binding
— regardless of how far it moved. That is the same trick the function matcher
uses at the top level, applied one level down, and it is immune to the exact
failure the current comment cites.

Ceiling: 46.9% of function-scoped bindings become transferable rather than
re-picked. What it cannot reach: locals whose declaring statement genuinely
changed — for those the model is the right answer, and a hint is the right tool.

Refutation to run first: of the body locals in close-matched functions, what
share have a declaring statement whose rename-blind hash is unchanged? If that
share is small, statements change more than assumed and this dies like the last
three ideas.
