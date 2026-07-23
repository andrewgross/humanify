# Vocabulary — cross-version eval & naming pipeline

Plain-language definitions for the jargon used across this experiment's
docs, results, and reports. Link here on first use: `[twin](VOCABULARY.md#twin)`.

## The two hashes

### statement hash

The split system's hash of one top-level statement: **all identifiers
and property names are masked**, literals and structure are kept. It is
rename-invariant — renaming can never change it — which is what lets us
tell "same code, different names" (noise) from "different code" (real
change). Coarse on purpose.

### structural hash (fingerprint)

The matcher's per-function hash: bound identifiers become slots keyed by
their resolved binding, but **property names, free identifiers, and
literals stay verbatim** — the discriminating content. Finer than the
statement hash; two statements can share a statement hash while their
functions have different structural hashes.

## Comparing two versions

### twin

A statement whose statement hash appears on **both** sides of a version
pair. A **unique twin** appears exactly once on each side — strong
identity. Twin statements with differing text are _noise_; byte-equal
twins are _clean_.

### family bucket

Several statements sharing one statement hash (16 identical identity
functions, 536 `var a, b, c;` lists). The hash says nothing about which
member corresponds to which — pairing them needs outside evidence, and
members nothing can distinguish are called **interchangeable**.

### pool

Inside the matcher: the set of candidate functions a still-unmatched
function is tied with after every evidence tier ran (the "ambiguous"
leftovers of a bucket).

### clean / noise / novel

Per fresh statement: **clean** = twin with byte-identical text;
**noise** = twin whose text differs (naming churn — the thing we drive
to zero); **novel** = hash absent on the prior side (real new/changed
code). Prior-side statements with no fresh twin are **removed**.

### statement mass vs diff-visible lines

`noiseLn`/`realLn` count **whole statements'** lines (mass): one edited
line inside a 5,000-line function flips the whole statement. The
**diff-visible** numbers count only lines a `diff` would actually print.
Mass is what the eval KPIs gate on (safe direction); diff-visible is the
human-facing truth. See the diff ledger in `trail-report.ts`.

### echo / root

When one binding's name flips, every statement that _references_ it
changes text too. The declaring statement is the **root**; the
referencing statements are **echoes**. Fixing a root clears its echoes.

### reloc

A name that exists on both sides but sits in a different split file —
same-name relocation. Tree churn measures the file-assignment version.

## Matching machinery

### the cascade

The ordered match tiers, strongest evidence first: unique structural
hash → binding identity → member key → enclosing statement → callee
shapes → caller shapes → callee hashes → two-hop shapes → shingle
similarity → ordinal. Later tiers only see what earlier ones left.

### evidence key

The bundle of distinguishing features (member key, callee/caller shapes,
callee hashes) used to tell bucket members apart. Members with identical
evidence keys are interchangeable as far as we can measure.

### exact match / close match

**Exact**: byte-identical modulo names — the whole slot table of names
transfers mechanically. **Close**: similar but changed content — locals
transfer only where statements align; the head name goes to the LLM
with the prior as a suggestion.

### statement-twin tier

Transfers names wholesale inside unique twin statements (applies before
everything else — whole-statement identity outranks finer guesses).

### votes / pins

Matched functions that _reference_ an unnamed binding testify to its
prior name (**votes**; applied at ≥2 agreeing). Below that floor, a
single exact-grade vote can still **pin** the name if content evidence
corroborates (the single-vote ladder).

### binding cascade

Module-level variables run through the same cascade as functions,
alternating rounds so each side's matches crack the other's buckets.

## Names and the floor

### mint / minted leftover

A name that still looks minifier-made (`q7x`, `__m`, `k_3`). The
**census** counts them in the final output; the goal is zero.

### naming floor / below-floor

The deterministic passes that name minted survivors, and the classifier
(`isBunToken`) deciding whether a name is acceptable. A **below-floor**
name is one the floor would reject.

### below-floor guard

Merged rule: a _prior_ name that is below-floor never settles or
transfers onto module-level bindings or function heads — minted names
are naming gaps, not names. The predicate is `isBelowFloorName`
(census shape minus decorated-descriptive): collision-decorated good
names (`fsPromises_`) transfer like any name, which closed the guard's
draw-dependent re-roll channel (exp035 task E).

### uniquify decoration

`name_2`, `k_3` — underscore-ordinal suffixes added to dodge collisions.
Below-floor when the stem is minted.

## Runs, references, invariants

### rebased prior / reference label

`REBASE_PRIOR=1` regenerates each pair's _from_ version with the current
pipeline before scoring, so the diff measures the pipeline against its
own output, not an old archive. Result folders like
`floor-guard-rebased` are committed **references** to compare against.

### healing hop

The first run after a naming improvement: fresh names differ from the
_old_ prior's names, so noise spikes once, then quiets. Expected cost,
not a regression — judged against a same-generation reference.

### self-hop / idempotence invariant

Re-humanify a version using **its own output** as the prior: every
statement is a twin, everything should inherit, and the output must be
**byte-identical**. Any diff line is nondeterminism or a phase-ordering
bug. `run.sh` runs this after every sweep.

### same-session A/B, control run, drift

LLM serving drifts across sessions (±thousands of lines on identical
code), so changes are graded by probes run in the same session with the
shared response cache; a **control** (unchanged code, same session)
separates a feature's effect from drift — a byte-identical control means
the whole delta is the feature.

### corpus gate

The text-diff reconcile pass refuses to run when too few prior lines
survive unchanged (a bundle-wide reorder breaks line alignment) — why
reconcile-based fixes cannot reach shuffle pairs like 85→86.

## Instrumentation

### strategy trail / funnel

`--diagnostics` records every naming tier's attempt per identifier
(applied / rejected / abstained / vote-routed + reason) until one lands;
the **funnel** is the per-tier rollup. `trail-report.ts` renders both.

### identifier ledger / diff ledger

Totals-first accountings: every binding from TOTAL down to REMAINING
still-minted; every diff line from TOTAL lines down to per-bucket
attribution.

### ambiguity probe

`HUMANIFY_AMBIGUITY_PROBE=<path>` dumps every ambiguous pool with both
sides' caller/callee evidence for offline ceiling measurements.

### ceiling

The measured upper bound of an idea's possible win, computed _before_
building it. Ideas whose ceiling is too small are recorded as **failed**
without being built.
