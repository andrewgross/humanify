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
