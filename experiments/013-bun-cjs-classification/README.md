# Experiment 013 — Bun CJS classification

## Goal

Validate end-to-end that humanifying two adjacent Bun-bundled versions of
Claude Code produces a clean `diff -r`:

- **Library files**: stable filenames across versions, minimal LLM cost,
  user can tell at a glance "which libraries did this release add / remove /
  bump."
- **Application code (`runtime.js`)**: humanified with `--prior-version`
  carry-over so structurally unchanged functions get identical names in
  both runs — the cross-version diff shows only real source changes.

## Hypothesis

With the changes in this branch (CJS classification + structural-hash
filenames + library marking via manifest), running humanify on v119 and
then v120 with `--prior-version` will:

1. Skip ~99.9% of unpacked files from LLM (only runtime.js needs renaming)
2. Reuse v119's humanified names for most of v120's runtime.js functions
3. Produce a `diff -r` whose noise is bounded by genuine source changes
   plus the predictable library-body byte deltas (different minifier rolls)

## Pre-experiment confirmation (cheap, already done)

| Check                                   | Source                      | Result                              |
| --------------------------------------- | --------------------------- | ----------------------------------- |
| Bun CJS factories detected              | `cross-version-baseline.ts` | v119: 1,493, v120: 1,493            |
| Structural-hash overlap across versions | `cross-version-baseline.ts` | 1,277 / 1,283 unique hashes (99.5%) |
| Filename overlap after stable naming    | `unpack-real.ts` + `comm`   | 1,450 / 1,454                       |
| Files marked library (skipped from LLM) | `library-skip-smoke.ts`     | 1,452 / 1,453 (99.9%)               |

All cheap checks point toward the hypothesis. The expensive end-to-end
runs validate the actual user-visible outcome.

## Inputs

- v119 bundle: `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.119/binary-decompiled/src/entrypoints/index.js` (13.7 MB)
- v120 bundle: `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.120/binary-decompiled/src/entrypoints/index.js` (13.8 MB)
- LLM endpoint: `http://192.168.1.234:8000/v1` (local vLLM, `openai/gpt-oss-20b`)
- Branch: `experiment/cross-version-caching` (this branch)

## Procedure

### Step 1: Humanify v119 (Run A)

```bash
HUMANIFY_ENDPOINT=http://192.168.1.234:8000/v1 \
HUMANIFY_API_KEY=local \
HUMANIFY_MODEL=openai/gpt-oss-20b \
node --max-old-space-size=8192 --import tsx/esm src/index.ts \
  /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.119/binary-decompiled/src/entrypoints/index.js \
  -o /tmp/exp013/cc-119 \
  --diagnostics /tmp/exp013/cc-119-diag.json \
  --bundler bun \
  --minifier bun \
  -vv > /tmp/exp013/cc-119.log 2>&1
```

Expected:

- Wall-clock ~1-3 hours (only runtime.js → LLM)
- Output dir contains ~1,454 files
- Diagnostics shows ~1,452 library-skipped, 1 novel processed
- `_bun-modules.json` lists every factory by stable name

### Step 2: Humanify v120 with prior-version (Run B)

Only after Run A completes successfully:

```bash
HUMANIFY_ENDPOINT=http://192.168.1.234:8000/v1 \
HUMANIFY_API_KEY=local \
HUMANIFY_MODEL=openai/gpt-oss-20b \
node --max-old-space-size=8192 --import tsx/esm src/index.ts \
  /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.120/binary-decompiled/src/entrypoints/index.js \
  -o /tmp/exp013/cc-120 \
  --diagnostics /tmp/exp013/cc-120-diag.json \
  --bundler bun \
  --minifier bun \
  --prior-version /tmp/exp013/cc-119/runtime.js \
  -vv > /tmp/exp013/cc-120.log 2>&1
```

Expected:

- Wall-clock significantly faster than Run A (cache hits)
- Diagnostics shows ≥80% of runtime.js functions matched from prior version

### Step 3: Diff the outputs

```bash
diff -r --brief /tmp/exp013/cc-119 /tmp/exp013/cc-120 > /tmp/exp013/diff-brief.txt
diff -r /tmp/exp013/cc-119/runtime.js /tmp/exp013/cc-120/runtime.js > /tmp/exp013/runtime-diff.txt
wc -l /tmp/exp013/*.txt
```

## Success criteria

| Criterion                                            | Target                                                     |
| ---------------------------------------------------- | ---------------------------------------------------------- |
| Run A completes without crash                        | yes                                                        |
| Run B completes without crash                        | yes                                                        |
| Run B's `priorVersionApplied` count (from diag.json) | ≥80% of runtime.js functions                               |
| Files only in cc-119 (library inventory removed)     | <20                                                        |
| Files only in cc-120 (library inventory added)       | <20                                                        |
| Runtime.js diff length                               | reflects real code changes, not rename noise               |
| Total tokens spent (sum of both runs)                | a small fraction of the equivalent two-fresh-runs baseline |

## Status log

Update this as the experiment progresses.

| Phase                  | Status          | Notes                                                                  |
| ---------------------- | --------------- | ---------------------------------------------------------------------- |
| Pre-experiment checks  | done            | All metrics above confirm hypothesis is plausible                      |
| Run A (v119, no prior) | done            | First attempt failed: regex extractor produced unparseable runtime.js. |
|                        |                 | Fix: switched body extraction to AST positions (commit f1b8aab).       |
|                        |                 | Second attempt: 9 hr 22 min wall-clock, exit 0.                        |
|                        |                 | 1,494 files emitted, 1,493 library-skipped, runtime.js parses OK.      |
|                        |                 | Coverage: 96.7% of 107,399 identifiers LLM-renamed.                    |
| Run B (v120 + prior)   | done-with-bugs  | First attempt OOM'd at graph-build (8 GB heap insufficient).           |
|                        |                 | Second attempt: --max-old-space-size=16384. 15 hr 31 min wall clock.   |
|                        |                 | Cache hits: 99.4% of functions, 74% of module bindings reused.         |
|                        |                 | transferStats: 41,069 exact-match + 21,776 close-match applied.        |
|                        |                 | BUG: output runtime.js fails to parse. Two distinct validation gaps:   |
|                        |                 | 1. Duplicate identifier collision (`NH` declared twice @350563).       |
|                        |                 | 2. Reserved keyword as parameter name (`delete` @416094).              |
|                        |                 | Humanify reported "Done!" but the output is broken.                    |
| Diff analysis          | blocked-on-bugs | Can't `diff -r` cleanly until v120 output parses.                      |
| Writeup                | pending         | Cache mechanics validated, validation pipeline gaps identified.        |

## Findings

### What worked

- CJS classification + structural-hash filenames: 1,491 / 1,495 cross-version filename overlap (vs 0 before).
- Library skip: 1,493 / 1,494 files marked library and skipped from LLM rename — saved ~99% of LLM cost on those files.
- Prior-version cache: 99.4% of v120's functions reused v119's renames. transferStats show 62,845 successful name applications.
- Memory stable at 3-5 GB throughout the run (under both 8 GB and 16 GB caps; 16 GB needed only for prior-version setup).

### What broke

The cross-version transfer path does not validate the names it applies:

- **Duplicate names**: a name carried over from v119 collides with a different binding in v120's scope. The transfer applied the name anyway, producing two `var NH = ...` declarations in the same scope.
- **Reserved keywords**: a prior-version name like `delete` (presumably an `object.delete()` method) was applied to a parameter position where keywords aren't allowed.

Fresh LLM choices in Run A passed these checks (96.7% coverage with 0 module-binding failures). The transfer code path needs the same identifier-validity + collision checks the LLM-rename path runs.

### What to fix next (out of scope for exp013)

- Add reserved-word check to prior-version transfer in src/cache/prior-version.ts
- Add scope-collision check before applying a transferred name
- Consider: re-run Run B after the fix, then attempt the `diff -r` analysis
