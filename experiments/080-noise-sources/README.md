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
