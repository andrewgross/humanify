# 082 — export-heir veto: content tiers were letting neighbors steal misnamed modules

> **STATUS 2026-08-20: shipped on branch `exp082-export-heir-veto`, cold walk
> validation in flight.** Offline replay numbers below are final (matching is
> deterministic); tree-level numbers pending the walk.

## What was found (task: explain the 4 remaining `-2` displacements, 244 lines)

The 080 handover treated the four remaining displacements as "below the
export-set floor or blocked by mutual-best — unmeasured why". Replaying the
matcher offline on the walk ledgers (`fossil-match.ts` is pure; the two split
ledgers carry the exact signatures it saw) showed something worse than a
missed match: **three of the four were CHAINS of thefts.**

The pattern, using `handle-post-tool-use-hook` as the example:

1. The prior module has 2 statements. Its content changed almost entirely in
   2.1.216 (statement overlap with its true heir: 0.33).
2. A NEIGHBORING 1-statement module happens to share one boilerplate statement
   hash — overlap exactly 0.5, which is tier B's floor — and its import edges
   agree (tiny init modules import the same common helpers). Tier B
   (edge-corroborated) hands it the prior module's identity, and with it the
   FILENAME `handle-post-tool-use-hook.js`.
3. The true heir — whose export names the function matcher already carried
   over, so its declared set overlaps the prior module 1.00 — reaches the
   export-set tier with its prior already taken. It mints fresh, collides,
   and becomes `handle-post-tool-use-hook-2.js`.
4. The neighbor was itself a real prior module (`initialize-plugins-and-
utilities`), so ITS prior is now free to be stolen the same way — the
   shipped 2.1.216 tree has `initializePluginsAndUtilities` living in
   `handle-post-tool-use-hook.js` and `initializeServerVal` living in
   `initialize-plugins-and-utilities.js`, and `initialize-server-val.js`
   deleted. Three misfiled files per chain.

Three chains on the busy hop (heads: `handle-post-tool-use-hook`,
`build-system-prompt-array`, `resume-stale-prompt-cancel`). Measured cost in
byte-identical moved lines alone: **298 git lines (19.8% of all cross-file
moves on the hop)** — plus wrong-content-under-right-name churn and importer
require-path churn that the move counter never sees.

The `displaced-names.py` count (244 lines) only saw the `-2` tips of the
chains. The misfiled middles masqueraded as content churn in files that kept
their names.

## The fix

`exportHeirVeto` in `src/split/fossil-match.ts`: a content-tier match (edge,
graded, graph-position) is refused when the pair's export sets flatly
contradict — both declare names, zero overlap — while some OTHER unmatched
fresh module clears the export-set floor (0.6) for that prior. The heir then
claims it at the export-set tier.

Two designs produced IDENTICAL final match sets on all three walk hops:
running the export-set tier first, and the veto. The veto was chosen because
it keeps the matcher's content-first philosophy: names never nominate a
match, they only block one that a rightful heir contradicts. (Name-first
ordering could cross-pair duplicated-export instances that only edges can
distinguish — the i36/Pd8 class. No measured hop hits this, but the veto
cannot by construction.)

## Offline replay (deterministic — no band needed)

`matchFossilModules` on the exp080-letwalk ledgers, shipped matcher vs veto:

| hop     | differing pairs | matches     | note                                          |
| ------- | --------------- | ----------- | --------------------------------------------- |
| 213→214 | **0**           | 4819 = 4819 | tier attribution shifts, sets identical       |
| 214→215 | **0**           | 4819 = 4819 | same                                          |
| 215→216 | **8**           | 4818 → 4820 | exactly the three chains + event-emitter-data |

The 8 busy-hop changes: the three heirs match their priors (export-set); the
three neighbors match their OWN priors (edge/graded, one honest mint); the
old wrong `event-emitter-data` graph-position match (createForkSessionInfo
module under eventEmitterData's name, 0 name overlap) becomes an honest mint.

Matcher preflight: 4 fixtures unchanged. `npm run check`: 8/8.

## Pre-registered predictions for the cold walk (written before reading it)

- busy hop: cross-file moved lines drop by ≈298 git lines (chain share);
  displaced modules 4 → 1 (`create-vqs-component` stays: heir export overlap
  0.43 < 0.6 floor); `filesAdded` ≈ −3, `filesRemoved` ≈ −1.
- calm hops: match sets identical offline, so tree deltas must sit inside the
  walk noise band (35 busy / 32 calm, `experiments/076-statement-placement/
walk-noise-band.json`).
- matcher counters: compare digit-for-digit; only the busy hop may differ.

## Residual (known, deliberate)

- `create-vqs-component`: the heir gained 3 export names (4 declared → 6,
  Jaccard 0.43). A containment metric (|inter|/|smaller|=0.75) would catch it
  but is laxer — a big module containing a tiny module's one common export
  name scores 1.0. Not shipped without sampling that false-positive class.
  ~16 moved lines + one `-2` file on the busy hop.

## The rest of the moved lines: upstream regrouping, not the pipeline

The handover's first task — diagnose the 1,480 moved lines — pointed at the
placement trail. **The trail cannot answer it: a busy-hop run WITH
`--diagnostics` produced ZERO placement-trail entries** (`placementTrails:
{tiers: {}, trails: []}` in a 102 MB diagnostics file whose naming-side
sections are all populated). The trail records the stable-split statement
tiers, and a fossil-split tree bypasses them — every src file except
`index.js` IS one fossil module. The all-zero `placement-stats.json` in every
walk tree is the same gap. Instrument fix (wire fossil assignment into the
trail) is a follow-up, not done here.

The ledgers answered instead. Pairing every statement-hash occurrence across
the two releases (excluding many-to-many collisions):

```
stayed in their module                       34,554
moved between two PERSISTING modules            123   <- upstream regrouping
moved into a minted module                       11
ambiguous (boilerplate twins etc.)               59
```

The 123 match the eval card's `relocatedStatements: 104` and its `topMoves`
list pair-for-pair (12x sanitize-html-val -> analyze-features, 7x
is-in-process-teammate -> is-local-observer, 5x load-image-processor ->
calculate-tile-count, ...). Spot-check of the biggest pair: the minified
2.1.216 bundle really does carry `detectImageMimeType`/`detectDocumentType`/
`getImageDimensions` inside the `calculate-tile-count` factory segment while
2.1.215 carried them in `load-image-processor` — the bundler regrouped, both
modules are correctly matched, and the functions kept their names (the lines
are byte-identical; only the file changed).

**So the moved-lines item decomposes as: ~20% matcher chain-theft (fixed
here), nearly all the rest upstream regrouping between correctly-matched
modules.** The latter is input-side change; keeping such a function's diff
clean would mean emitting it in its OLD file and importing it across the
module boundary — the exp070 "fossil relayout" lever, a real design, not a
patch. Recorded as the standing next lever, not attempted here.

## Traps hit (recorded so the next reader skips them)

- The placement trail does NOT cover fossil-split trees (see above) — reading
  `--diagnostics` output for placement questions on the current pipeline
  returns an empty instrument, not an answer.
- The fresh side's `fossilModules` in a tree's OWN split-ledger.json are
  exactly the signatures the matcher saw (fossil-assign writes them from
  `extract.modules`), so cross-release matching replays offline from two
  ledgers alone. Stems must be re-derived (basename, strip `-N` on fresh).
