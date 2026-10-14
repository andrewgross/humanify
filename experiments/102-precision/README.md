# exp102: precision guards for library matching

STATUS (2026-10-14): EXECUTED on branch `exp102-precision`. Three generic guards, all default OFF
(default `humanify discover` reports byte-identical to main: 69/69 reports, label `exp102-ctl2-structural`).
Every number below is the exp096 scorer v3 on the fixed corpus with index `/work/exp097-data/ix/step4`
(band 0: every delta is real). Thresholds were picked on the scored sets (see "Tuning risk"): the frozen
TEST split (campaign lane T) is the judge before any default flip.

## Totals first

Held-out wrong-family bytes, structural rule: **388,512 -> 184,508 (-204,004, -52.5%)** for **-985 named
bytes** (recall 23.91% -> 23.90%). Precision, before -> after all three guards:

| set                      | structural P       | structural R   | structural F1  | agreement P        | agreement R    | agreement F1   |
| ------------------------ | ------------------ | -------------- | -------------- | ------------------ | -------------- | -------------- |
| held-out 67              | 96.08 -> **98.10** | 23.91 -> 23.90 | 38.29 -> 38.44 | 96.13 -> **97.12** | 18.51 -> 18.51 | 31.05 -> 31.10 |
| task-master-ai (esbuild) | 97.90 -> **98.91** | 40.47 -> 40.47 | 57.27 -> 57.44 | 98.97 -> **99.02** | 28.34 -> 28.34 | 44.06 -> 44.06 |
| wrangler                 | 95.05 -> **96.26** | 14.56 -> 14.56 | 25.26 -> 25.30 | 100.0 -> 100.0     | 10.38 -> 10.38 | 18.80 -> 18.80 |
| Claude Code 2.1.216      | 100.0 -> 100.0     | 70.64 -> 70.64 | 82.80 -> 82.80 | 100.0 -> 100.0     | 61.87 -> 61.87 | 76.44 -> 76.44 |

App code claimed: 0 bytes on every set, every label. Labels: `exp102-ctl2-structural` / `step4f-agreement`
(before), `exp102-final-structural` / `exp102-final-agreement` (after: all three guards,
`contested_cover = 0.05`).

## 1. Where the wrong bytes came from (before)

`wrongfam.py` decomposes a label's wrong-family claims by mechanism. Held-out, structural, 388.5 KB:

| cause                                                                  | bytes   |
| ---------------------------------------------------------------------- | ------- |
| propagation from seeds that were themselves wrong                      | 179,859 |
| propagation overreaching from a right seed region                      | 126,133 |
| index seed, whole print, one family holds it                           | 39,209  |
| index seed, function cover, one family                                 | 19,390  |
| declared / banner seeds (no index)                                     | 13,790  |
| index seed shared by several families, queue seeds, small nested cover | 10,131  |

Propagation (DOWN / MODULE / GLUE) made 79% of the wrong bytes. Read per unit, it fell into three
shapes, each a generic gap in the guards:

- **A unit's own index evidence points to another family** and containment let it through, because
  containment only asks "does P's family hold any print of this unit?" (one shared nested helper is
  enough).
- **A table the unit print cannot see**: entities' decode tables are one long string, a handful of
  nodes, under the index's 20-node row floor; parse5 calls them, so DOWN took them as parse5.
- **A huge unit taken through one thin shared helper**: a 49 KB terser unit whose only index evidence
  is a ~500-node helper uglify-js and terser share; an 18 KB MCP schema module taken as
  `@hono/node-server` through two small functions both families hold.

## 2. The guards (all `[library.discovery]`, default off)

Each refuses a claim; none names anything. A refused propagated unit also stops propagation through it.

| key                         | rule                                                                                                                                                                                                                                                                                                                                                          | what it reads           |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| `evidence_conflict = true`  | an index seed or propagated claim of P is refused when another family's index prints cover **strictly more** of the unit than P's (per family: the whole print if held, else its largest non-overlapping function prints; prints at or over the row floor; no specificity or package-count filter). A tie (a fork holding the same prints) is not a conflict. | the index unit table    |
| `literal_conflict = true`   | a propagated claim of P is refused when the unit carries **long literals** (values over the extractor's 120-unit cap, kept as `S#`/`P#`/`R#` hashes: generated tables, long messages) that the index's literal table holds, held by at most 3 families, and none by P's family.                                                                               | the index literal table |
| `contested_cover = <share>` | a propagated claim of P is refused when the unit is **contested** (another family's prints cover part of it) and P's prints cover less than `<share>` of its nodes. Uncontested thin cover (version drift of the right package) is kept.                                                                                                                      | the index unit table    |

Tests (red first): `libdetect/discovery_test.rs`
`evidence_conflict_refuses_a_propagated_unit_another_family_covers_more_of`,
`evidence_conflict_drops_an_index_seed_another_family_covers_more_of`,
`literal_conflict_refuses_a_propagated_unit_whose_long_literal_is_another_family_s`,
`contested_cover_refuses_a_big_unit_claimed_through_a_thin_shared_print`; config keys in
`humanify-cli/src/library_config_test.rs`.

## 3. Each guard alone (bytes vs the before labels)

| label                                             | set            | wrong family removed | named removed | P before -> after |
| ------------------------------------------------- | -------------- | -------------------- | ------------- | ----------------- |
| `exp102-ec2-structural` (evidence_conflict)       | held-out       | -71,119              | -727          | 96.08 -> 96.77    |
|                                                   | task-master-ai | -772                 | 0             | 97.90 -> 97.94    |
|                                                   | wrangler       | -1,433               | 0             | 95.05 -> 96.26    |
| `exp102-ec2-agreement`                            | held-out       | -21,532              | -802          | 96.13 -> 96.40    |
|                                                   | task-master-ai | -772                 | 0             | 98.97 -> 99.02    |
| `exp102-lc-structural` (literal_conflict)         | held-out       | -56,616              | -88           | 96.08 -> 96.63    |
|                                                   | task-master-ai | -313                 | 0             | 97.90 -> 97.92    |
| `exp102-lc-agreement`                             | held-out       | -56,616              | 0             | 96.13 -> 96.85    |
| `exp102-cconly-structural` (contested_cover 0.05) | held-out       | -135,990             | -170          | 96.08 -> 97.41    |
|                                                   | task-master-ai | -19,721              | -15           | 97.90 -> 98.86    |

CC is byte-unchanged under every label; wrangler moves only through the evidence-conflict seed check.
Agreement is untouched by `contested_cover` (its seeds never reach those units).

## 4. What each removed, read claim by claim (`claimdiff.py`)

All three guards, structural, held-out: removed 204,004 B wrong, 1,055 B right (plus 70 B newly right);
task-master-ai 20,806 B wrong, 15 B right; wrangler 1,433 B wrong, 0 right.

| removed claim (claimed <- truth)                                                                                         | bytes   | guard                                | verdict        |
| ------------------------------------------------------------------------------------------------------------------------ | ------- | ------------------------------------ | -------------- |
| uglify-js <- terser (html-minifier-terser; terser 5.51 is newer than any indexed terser)                                 | 117,028 | evidence (59 KB) + contested (56 KB) | wrong, removed |
| parse5 <- entities (decode tables, 3 apps)                                                                               | 55,677  | literal                              | wrong, removed |
| @hono/node-server <- MCP (task-master-ai x2)                                                                             | 38,683  | contested                            | wrong, removed |
| css-select <- node-html-parser                                                                                           | 10,235  | evidence                             | wrong, removed |
| confbox <- jsonc-parser (wrangler seed)                                                                                  | 1,426   | evidence (seed)                      | wrong, removed |
| zod-to-json-schema, eventsource-parser, micromark\*, raw-body, stubborn-fs                                               | ~3,000  | mixed                                | wrong, removed |
| micromark seed (466) + module (26), rxjs (origin, 305), @hono/node-server (origin 170, right 15), parse5 / entities (88) | ~1,070  | mixed                                | RIGHT, lost    |

The right claims lost are 0.01% of held-out named bytes. Agreement: removed 78,148 B wrong (parse5 /
entities 55.7 KB, d3-delaunay <- delaunator 10.6 KB, css-select <- node-html-parser 10.6 KB, ...),
1,283 B right lost (rxjs inside puppeteer-core, origin-right), 481 B newly right.

## 5. What is left (after, structural held-out 184.5 KB)

Read per (claimed, truth) pair; most of it is a TRUTH question, not a matcher one:

- **Vendored copies the origin layer does not mark** (the claim names the code's real origin; the shipper
  is not indexed): tweetnacl / fflate / noble-curves inside openpgp's bundled dist (32 KB),
  zod-to-json-schema inside @ai-sdk/provider-utils (18.7 KB), lodash-es / Runge-Kutta inside cytoscape
  (10.5 KB), quickjs-wasi inside puppeteer-core (11.6 KB), ora's and marked-terminal's dependencies
  (~10 KB). Candidates for the campaign's truth audit (hand-check, then fix the truth honestly).
- **Family-table gaps**: mimic-fn <- mimic-function (a rename), @isaacs/balanced-match <- balanced-match
  (~5 KB). Data, not a rule.
- **Generated template code, true package not indexed**: @anthropic-ai/sdk <- openai (Stainless, 32 KB).
  The index's `stoplists.json` already describes the Stainless `lookupFilePatterns`, but no Rust code reads
  them, and applying them blindly would cost Claude Code's right @anthropic-ai/sdk claims. No other
  Stainless SDK is indexed, so an index-derived commonness rule cannot see it.
- **got <- @sindresorhus/is** (9.4 KB, MODULE), **sharp <- semver** (6.8 KB, a declared seed),
  **prettier <- @angular/compiler** (agreement 75.8 KB: prettier 2.8.8's index entry bundles the
  compiler, which is not indexed).

## 6. Rejected, with numbers

- **Fork / sibling margin over regions** (refuse a family's whole region when a sibling ties it on >= 30%
  of its evidence-bearing units and has more bundle-exclusive evidence): held-out -131 KB wrong but -127 KB
  right (yargs-parser under meow, css-select / csso under svgo, d3 pieces). A tie cannot tell a fork from a
  sponge that bundles the package. Index-side overlap does not separate them either (uglify-js/terser
  share 2%/9% of prints; css-select/svgo 10%/2%).
- **Cover rule without contest** (refuse a propagated unit P covers < 10% of): held-out -133 KB wrong,
  -31 KB right, wrangler -26 KB right.
- **"No P-exclusive print"** on propagated units: -66 KB wrong, -83 KB right held-out.

## Tuning risk

`contested_cover` is a share picked on these sets: at 0.05 it is clean; at 0.10 it also removes
node-fetch in task-master-ai (17.4 KB right, label `exp102-cc10-structural`, built at `70c698e7`). The
two guards without a tuned number (`evidence_conflict`, `literal_conflict`; the literal guard's
3-family cap mirrors `lookup.max_packages`) carry no such risk. Judge all three on the frozen TEST split
before flipping a default.

## Recommendation

- `evidence_conflict` and `literal_conflict`: clean on held-out, task-master-ai and wrangler under both
  rules (precision up everywhere, recall within 0.002 points, CC unchanged). Candidates for default ON
  after the TEST split agrees and a warm neutrality run (they only run with `library.matching = "report"`
  and an index; a default pipeline run never runs discovery).
- `contested_cover = 0.05`: the largest single lever (-136 KB held-out alone), clean here, but its
  threshold was picked on the scored sets: TEST split first.

## Instruments

- `wrongfam.py <label>`: wrong-family bytes by mechanism (seed kind / propagation from wrong seeds /
  overreach).
- `claimdiff.py <base> <label>`: every claim a configuration removed, added or changed, judged by the
  truth.
- `compare.py <base> <label>...`: exact byte deltas per set.
- `npm run eval -- library <label> ... --pipeline-set library.<key>=value` (new): a label's `[library]`
  settings, validated before any build, written by the one config writer, recorded in `pipeline.json`.
