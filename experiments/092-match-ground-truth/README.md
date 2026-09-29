# exp092 — the match ground-truth harness

## STATUS

- **State: SHIPPED (2026-09-29), first baseline recorded.** This is a
  safety-net instrument, not a lever: it exists so a regression in the
  matcher (pipeline stage 8) is caught DIRECTLY, before it can hide
  inside eval noise (rule 11: the eval cannot resolve a small matching
  regression inside its ±band and will print a confident sign anyway).
- **Instrument:** the `humanify match` verb (fully cold — no LLM path
  exists in it) + the scorer in
  [`experiments/lib/match-truth/`](../lib/match-truth/).
- **First baseline:** `baseline-f8b87490.json` — matching
  machinery of main @ `f8b87490` (the verb is purely additive; it drives
  the same `match_prior_version` stage a pipeline run drives). Numbers
  below.
- **Findings on day one** (from the fixed fixture alone, before any real
  package ran): the two misses below are REAL matcher behavior, recorded
  in `score.test.ts` as exact values — treat them as the first leads:
  1. **A wrapper-spelling change defeats matching.** A function whose
     body is byte-identical but spelled `(a, b) => {}` in the prior and
     `function (a, b) {}` in the new version is NOT matched (fixture:
     `keepWrapper`, recall 2/3). The statement twin machinery misses the
     same statement too (statement recall 2/3).
  2. **A literal-only change defeats matching when the literals change
     magnitude.** `n * 2 + 50` → `n * 3 + 100` is unmatched: `2` and `3`
     blur (same magnitude class) but `50` and `100` do not, so the
     structural hash differs and nothing recovers the pair (fixture:
     `changedSmall`, should-match 0/1). This is the should-match tier's
     reason to exist.
- No claims retracted yet; nothing here supersedes another experiment.

## The brief (why this is the right check)

When humanify processes a new program version against the old, its most
important step is recognizing "this function in the new version IS that
function from the old one" so old names carry over. Until now that
recognition was only ever measured by its downstream effects on the
cross-version diff — where a small regression could hide inside normal
variation forever. This experiment builds the direct check: known ground
truth in, matching decisions out, recall/precision-like numbers back.

## Method

For each pinned package pair:

1. Download the two published tarballs (`npm pack`, content-addressed
   per version) and take the package's OWN shipped single-file build at
   the same path in both — no rebuild step, so nothing the harness does
   can perturb ground truth; what differs between the versions is what
   the package's toolchain actually shipped.
2. Produce the two input forms the pipeline expects: run the OLD build
   through the stage-6 formatter (`humanify format`) — the closest
   no-LLM equivalent of a humanified prior, since this instrument is
   pre-naming by design — and pass the NEW build as the input.
3. Run `humanify match` (detect → unpack → format → graph → matching,
   stopped before naming; deterministic; no LLM) and score its dump.

### The ground-truth rule (kept simple and defensible)

A function (or statement) whose **canonical form** is byte-identical
between the two versions — where the canonical form normalizes ONLY the
wrapper spelling (`function(a,b){}` ≡ `(a,b)=>{}`) and erases identifier
renaming (a consistent bijection inside the slice, via first-occurrence
rewrite; string/template contents opaque) — **MUST be matched**. Two
tiers:

- **Must-match (hard)**: canonical-identical, occurring exactly once on
  each side. Duplicated canonical forms (identical helper functions) are
  excluded from the metric — the matcher may pair those in any order —
  and counted as `duplicateClass`.
- **Should-match (advisory)**: identical under the LOOSE form (literals
  additionally blurred, mirroring the structural hash's string-length +
  number-magnitude policy) but not strictly. Reported separately, never
  hard-fails: two genuinely different functions that differ only in
  literals also read loose-equal.

### The numbers

- **`recall`** — of the must-match pairs, the fraction the matcher
  reported. THE metric. Misses are listed by name, per package.
- **`shouldRecall`** — the same over the advisory tier.
- **`reportedClasses`** — every reported pair classified identical /
  near / far by content. **A `far` pair is a LEAD, not proof of a wrong
  match** — matching genuinely changed code is the matcher's job, so
  exact precision is only knowable where ground truth exists by
  construction (the committed fixture, where both tiers' misses are
  exact). `farPairs` are listed with slices for reading.
- **`statements.recall`** — of must-match statements, the fraction
  proposed as a twin (any gate outcome; `abstained:no-candidacy` is by
  design — nothing to bridge — so proposals, not bridging, count).

Blind spots (conservative direction — miss ground truth, never invent
it): identifier-shaped contents inside template literals are opaque;
template interpolations are not canonicalized; a regex literal
containing a quote character can confuse the string tracker; a named
function expression head (`var f = function g(){}`) is not normalized.

## The corpus (pinned; `experiments/lib/match-truth/corpus.ts`)

13 real npm packages, CJS/UMD single-file builds, one realistic release
distance each: underscore 1.12.1→1.13.4, lodash 4.17.20→4.17.21, moment
2.29.1→2.29.4, dayjs 1.10.7→1.11.10 (minified), axios 1.6.0→1.7.9,
bluebird 3.7.0→3.7.2, q 1.5.0→1.5.1, ramda 0.27.2→0.29.1, immutable
4.0.0→4.3.7, async 3.2.0→3.2.6, minimist 1.2.5→1.2.8, ms 2.1.2→2.1.3,
jquery 3.6.0→3.7.1. Re-run: `npm run match-truth` (downloads once into
`.corpus/`, gitignored). bluebird uses the plain `js/release/` build —
its `js/browser/` build is a browserify bundle and would need the
webcrack shim, out of this instrument's single-file scope.

## First baseline — matching machinery of main @ f8b87490

`baseline-f8b87490.json` (run 2026-09-29, release binary built
from this branch — the verb is additive; the matching machinery is
main's). **TOTAL first**:

| metric                         | value                                                                    |
| ------------------------------ | ------------------------------------------------------------------------ |
| must-match recall (THE metric) | **3082 / 3086 = 0.9987**                                                 |
| should-match (advisory tier)   | 0 / 0 (nothing loose-unique on this corpus)                              |
| reported pairs (all packages)  | 3595 — 3581 identical, 3 near, **11 far**                                |
| statement-twin recall          | 20 / 20 = 1.0 (20 must; most bundled builds are ONE top-level statement) |

Per package (must matched/must, dup-class rows excluded from the
metric):

| package    | must    | recall    | reported (identical/near/far) |
| ---------- | ------- | --------- | ----------------------------- |
| underscore | 174/174 | 1.000     | 182 (181/1/0)                 |
| lodash     | 586/586 | 1.000     | 678 (678/0/0)                 |
| moment     | 306/306 | 1.000     | 349 (349/0/0)                 |
| dayjs      | 38/39   | **0.974** | 44 (41/1/2)                   |
| axios      | 205/205 | 1.000     | 215 (212/0/3)                 |
| bluebird   | 1/1     | 1.000     | 1 (1/0/0)                     |
| q          | 184/184 | 1.000     | 219 (219/0/0)                 |
| ramda      | 354/354 | 1.000     | 395 (392/0/3)                 |
| immutable  | 485/485 | 1.000     | 662 (661/0/1)                 |
| async      | 248/248 | 1.000     | 311 (310/0/1)                 |
| minimist   | 16/16   | 1.000     | 16 (16/0/0)                   |
| ms         | 5/5     | 1.000     | 5 (5/0/0)                     |
| jquery     | 480/483 | **0.994** | 518 (516/1/1)                 |

**What the first run FOUND — the leads:**

- **11 of 13 packages at recall 1.000.** The matcher is very good on
  this corpus; the instrument's value is that a future regression now
  has a number to move.
- **dayjs (the only minified-shipped input) is the weakest**: the miss
  is `function (t, n, i, s) { return t && (t[n] || t(e, r)) || i[n].substr(0, s); }`
  — a helper whose body references FREE module-scope identifiers (`e`,
  `r`). Canonically identical modulo minified renaming, unmatched by the
  cascade. Suspect: free-identifier references under minified renaming
  — the exact "no minified leftovers" regime.
- **jquery: 3 misses.** One large function (`return !rhtml…` — again a
  free-scope-identifier body), one tiny `function () { return true; }`
  that is unique on both sides yet unmatched (worth reading — a
  unique-both-sides trivial shape missing is a clean cascade gap), and a
  Deferred callback whose body contains a free `mightThrow` reference.
- **The 11 `far` reported pairs** (axios 3, dayjs 2, ramda 3, immutable
  1, async 1, jquery 1) are listed with slices in the baseline file —
  leads to READ, not errors (matching changed code is the matcher's
  job).
- Common shape of the misses: **free identifiers in the body** — the
  same suspect twice. That aligns with the known-gaps file (single-letter
  minified identifiers) and is where the noise work should look first.

**Regenerating the fixture** (committed under
`experiments/lib/match-truth/fixtures/`, asserted exactly in
`score.test.ts`):

```bash
cargo build -p humanify-cli
./target/debug/humanify format experiments/lib/match-truth/fixtures/old.js \
  -o experiments/lib/match-truth/fixtures/prior.formatted.js
./target/debug/humanify match experiments/lib/match-truth/fixtures/new.js \
  --prior-version experiments/lib/match-truth/fixtures/prior.formatted.js \
  -o experiments/lib/match-truth/fixtures/dump.json --work-dir /tmp/match-fixture-work
```

**Re-running the baseline**: `npm run match-truth -- --out
experiments/092-match-ground-truth/baseline-<sha>.json` (needs
the release binary and, once, the npm registry; `.corpus/` caches the
tarballs). Like `noise-bands.json`, the committed scorecard is the
reference future runs compare against.

## What this test CANNOT see

- **Naming and placement — nothing downstream of matching.** A match can
  be correct and the name still churn (the naming waves re-ask); a match
  can be missed and the output still look fine because the LLM drew the
  same name. The eval (034) owns that end-to-end verdict.
- **The prior-with-human-names regime.** The prior here is the
  formatter's output, not a fully humanified tree (no LLM is available
  and none is needed — matching reads structure, not names). If the
  matcher ever reads a name where it should read structure, this harness
  will NOT catch it; the eval would.
- **Bundled multi-file inputs** (bun/webcrack unpacks): the scorer
  refuses multi-file dumps loudly. Single-file builds only.
- **Precision against real packages.** Without author-maintained
  correspondences, a reported pair of two different-looking functions
  cannot be called wrong — hence the lead list, not a false-positive
  count.
- **Duplicate-heavy libraries.** Identical helper functions repeated on
  a side are excluded from the must set (any pairing order is legal).
