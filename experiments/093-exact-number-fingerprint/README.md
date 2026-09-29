# exp093 — MatchKey keeps EXACT numbers (the magnitude buckets die)

## STATUS

- **State: MEASURED (2026-09-29), verdict LANDED-ON-BRANCH (recommendation:
  LAND).** One commit ahead of main @ `0c5860d9` on
  `exp093-exact-number-fingerprint` (15 files, +435/−150). Result against
  the study's bar — zero real pairs lost: **PASS**; ambiguity strictly
  down: **PASS**; asks identical: **NOT PASSED, and the deltas are the
  experiment's own signal** (33 ask rows of 2,342, every one enumerated and
  attributed below; the largest single cause is a pair the change made
  MORE correct). All four measurements (M1–M4) below, before/after.
- **Vendor-inclusive M2 is PENDING the coordinator**: the
  `fix/match-instrument-scale` branch (match verb reading raw-bundle
  vendor files) had not merged to origin/main at measurement time
  (checked at finish: origin/main still `0c5860d9`), so M2 ran
  runtime-only exactly as the number-blur study did. One number below
  (the vendor-side matching deltas inside `vendor/`) will need the same
  A/B once that verb capability lands; nothing in the runtime-side
  numbers can change by extending coverage, but the vendor side is
  currently UNMEASURED between eras (M3's vendor tree diff shows only the
  predicted hash-identifier class moved there, which bounds any vendor
  matching delta tightly — see M3).
- Evidence base: `/work/number-blur-study/README.md` (the number-blur
  study, 2026-09-29). Measurement artifacts: `/work/exp093/`
  (`m1/`, `m2/`, `m3/` with the dumps, compacted stats, comparisons).

## The change

The MatchKey fingerprint family keeps EXACT number (and bigint) values;
`numeric_magnitude` — the number-magnitude buckets — is deleted. String
rules untouched (volatile semver/ISO/hex classes + `__STR_<len>__`
markers stay). `LiteralPolicy::Blurred` → `LiteralPolicy::MatchKey`; the
enum's doc block names every consumer family on each policy, and every
policy keeps numbers exact now — the one policy choice left is the
strings. `matching/statement_align` (its own walk, kept in sync with the
serializer's arms) made the same change. `FACTORY_HASH_VERSION` 2 → 3:
a manifest stamped with any other version — TS-written (no stamp) or an
older Rust hash era — routes to the WP5.6e content re-key
(`stale_era`, renamed from `ts_era`, whose name now told the truth only
for TS manifests), never silently hash-joined. Red/green TDD throughout:
six tests went red under the old policy first (four serializer tests, two
era-gate tests), then green.

## Why (the study's evidence, summarized)

Across 64,061 real matched pairs (60,466 function pairs on the
2.1.215→216 claude-code walk, 3,595 on the exp092 corpus, 0 statement-twin
proposals), `numValueDiffer` was ZERO — no real pair's numbers differ, so
the number blur enabled 0 matches. All 52 literal-only walk pairs survive
ExactNumbers (they differ in STRINGS — 44 in the volatile classes). The
only pair the bucket blur decided was the SYNTHETIC exp092 fixture miss
(`n * 2 + 50` vs `n * 3 + 100`: 2/3 share bucket N=0; 50/100 straddle a
bucket edge) — a manufactured miss, bought without anything. The buckets
also merged real distinct functions (420 prior / 447 fresh class merges
on the walk pair, 26 scanner-attributed blur ambiguity events).

## M1 — ground truth (exp092 harness, `npm run match-truth`)

Before = the current-main binary (main @ `0c5860d9`), after = this
branch's binary. Both scored the 13-package corpus cold (identical
`TOTAL` line; scorecards at `/work/exp093/m1/`).

| metric                         | before                | after                 |
| ------------------------------ | --------------------- | --------------------- |
| must-match recall (THE metric) | **3082/3086 = .9987** | **3082/3086 = .9987** |
| per-package breakdown          | unchanged             | unchanged             |
| statement-twin recall          | 20/20                 | 20/20                 |

**Recall byte-stable — zero pairs lost.** What moved is TIER
ATTRIBUTION, in the good direction (the same pairs resolve EARLIER in the
cascade — previously-blurred twins are distinct at the unique-hash tier
before the weak tiers are consulted): lodash
`structuralHashUnique` 578→**579** (`calleeShapes` 2→1), moment
304→**306** (`enclosingStatement` 2→0), q 330→**332**
(`enclosingStatement` 20→18), jquery 474→**476** (`ordinal` 1→0).

**The synthetic fixture** (`changedSmall`, `n * 2 + 50` → `n * 3 + 100`):
STILL MISSES under exact numbers — the values genuinely differ, nothing
at the strict tier rejoins them, and the should-tier is scorer-side
advisory only. Scorecard byte-equal to the committed one (`shouldRecall
0/1`, `missedShould: changedSmall`). Per the study, no real-world
counterpart exists among 64,061 matched pairs — this miss costs nothing
measured.

## M2 — walk regime (`humanify match`, 2.1.215 → 2.1.216 runtime)

Prior = the study's `prior-reformatted.js` (byte-identical to
`/work/exp050-cold/2.1.215-rebased/.humanify/humanified.js`), fresh = the
unpacked 216 runtime.js (the study's `run/runtime.js` — on current main
the RAW bundle fails at the format stage in BOTH binaries identically
(`parse error: Expected function name`), so the two binaries are
behavior-identical at the verb and nothing regressed; the raw-bundle
vendor-inclusive match is the pending coverage noted above). Before = the
study's `dump-runtime.json` (compacted numbers verified against the
study's 60,466); after = a fresh dump from this branch's binary. Both
matches: **60 s wall**. Comparison: `/work/exp093/m2/compare.txt`.

| metric             | before             | after                                                              |
| ------------------ | ------------------ | ------------------------------------------------------------------ |
| prior / fresh rows | 63,723 / 64,493    | identical                                                          |
| matched pairs      | 60,466             | **60,467** (60,465 common; +2, −1 reassigned)                      |
| unmatched prior    | 2,334              | **2,334** (unchanged)                                              |
| ambiguous rows     | 923                | **922** (2 resolved outright, 1 new)                               |
| binding cascade    | stillAmbiguous 455 | **445** (identityResolved 7220→7223, enclosingStatement 2180→2187) |

**Zero real pairs lost.** The one "lost" pair (`getFeatureActiveData`
↔ `Ids`, both `function X(){return <freeId>;}` getters) was an
INTERCHANGEABLE-tier pairing — a family where any assignment is legal by
construction (exp092's ground truth excludes them as `duplicateClass`);
it now sits in a 7-candidate ambiguous row, and the unmatched-prior count
did not move.

**Ambiguity resolved, in exactly the class the study predicted:**
`getSecondTerminalSymbol` (`return fetchTerminalSymbols()[4]`) and
`getSecondTerminalSymbolVal` (`...[1]`) were a blur-merged pair before —
`[4]` and `[1]` both bucket to `N=0`, so each sat in a 2-candidate
AMBIGUOUS row, matched to nothing, at risk of an arbitrary swap. Under
exact numbers each is uniquely hashed; both resolved at
`structuralHashUnique` — with the CORRECT twin ([4]↔`[4]`, [1]↔`[1]`).
These are 2 of the study's 26 blur-attributed events — the only two that
were resolvable by matching alone; the rest are same-shape helper
families (the 33/65/241/630-candidate histogram untouched).

**Distinctness improved:** 61 pairs moved UP into
`structuralHashUnique` (34 from `enclosingStatement`, 19
`propagation`, 4 `calleeShapes`, 1 `ordinal`, 1 `memberKey`;
unique-tier 37,269 → 37,330); a handful of lateral shifts among weak
tiers (3 `interchangeable`→`ordinal`, 2 `enclosingStatement`→`memberKey`,
1 `interchangeable`→`propagation`) — same decisions, different rung.

The era change's input-side cost, measured for the record: the fresh
bundle's unpacked `runtime.js` embeds hash-fallback vendor names, which
differ between binaries (`lib_bc193b64` → `lib_74796e4b`; 3,974 byte
differences, identical file size) — the expected one-time `lib_<hash>`
re-derivation, confined to hash-derived identifiers.

## M3 — end-to-end with the stub (full pipeline, production regime)

Both binaries ran the full pipeline (`--split`, the eval's launch shape)
over the raw 2.1.216 bundle against the real 2.1.215-rebased prior tree
(TS-era manifest on both legs — both re-key by content:
“1624 of 1624 prior entries readable; 1621 factories in 1371 structural
groups joined a prior group (0 ambiguous)” on both).

Wall time / peak RSS, effectively identical:
before **3m26s / 28,661 MiB**, after **3m27s / 28,471 MiB**.

### (a) asks — NOT identical, and the delta is the signal

`scripts/diff-asks.ts`: before 2,340 asks, after **2,342** — 15 rows
only-in-A, 17 only-in-B, 7 changed (count drifts of ±2 in
`usedNamesCount`). Full enumeration in `/work/exp093/m3/asks-delta.txt`.
Every row is one of three causes, each verified:

1. **The matching-population changes M2 measured** — at ~7 scopes the
   member lists of `prior-hinted`/`shadowed`/`module-lane` asks shift by
   the functions whose matches changed (e.g. `a0_` enters a
   module-binding ask in B; `Kbr` flips prior-hinted→shadowed;
   `qLp` moves lane). One extra NameTaken retry (`oia`) and 2 extra
   asks total.
2. **The era's `lib_` renames inside the vendor ask** — the one
   `vendor-namer` ask is the same ask, but four target identifiers carry
   their new-era names (`lib_72ea3321`→`lib_8e0348d4` etc.). Not a new
   question.
3. **Downstream used-names counters** (7 rows, ±2 each).

The largest single perturbation is cause 1's best case: the two
terminal-symbol getters that GAINED correct matches — more carries, so a
couple of scopes ask fewer/different members.

### (b) trees — 1,765 of 1,778 moved entries are the predicted class

`diff -r` of the two output trees: 1,778 entries (classification in
`/work/exp093/m3/finalclass.txt`):

| class                                                                                              | entries                  | predicted?                                                                                                                                                                                                               |
| -------------------------------------------------------------------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| renamed `lib_<hash>` vendor files (504 modules × both legs)                                        | 1,008                    | YES — hashVersion re-derivation; do not keep old names                                                                                                                                                                   |
| files whose diffs are EXCLUSIVELY hash-era identifiers (`lib_` bindings/paths in same-named files) | 756                      | YES — same class, one level down                                                                                                                                                                                         |
| the vendor manifest itself (`hashVersion` 2→3 + `structuralHash` values)                           | 1                        | YES                                                                                                                                                                                                                      |
| `humanified.js` (the whole-program mirror: 1,052 lib-lines + the naming lines below)               | 1                        | mirror                                                                                                                                                                                                                   |
| src files with NON-hash content differences                                                        | **12** (≈550 diff lines) | naming cascade — see below                                                                                                                                                                                               |
| `split-ledger.json` + `stage-hashes.json`                                                          | 2                        | name lists only: `files`/`hashes`/`aliases`/`order`/`hashVersion` byte-identical; ledger differs only in `fossilModules`/`emitNames`/`nameToFiles` NAME text; stage-hashes differ only in `afterNaming`/`afterPlacement` |

The 12 src naming diffs are the stub-naming cascade off cause 1 — and one
real cost case, read to the bottom:

- **`src/c0-renamed/c0-renamed.js`**: the fresh module constant
  `var a0_ = 25000`. In 215 the same module carried
  `var DEFAULT_TIMEOUT_MS = 60000`; the release CHANGED the value
  60000→25000 (25000 and 60000 share bucket `N=4`). Before: the name
  carried across the value change (`DEFAULT_TIMEOUT_MS5 = 25000`). After:
  exact numbers refuse the content match, the binding is re-asked, the
  stub mints `a0_Renamed`. This is the changed-code naming frontier
  (exp062–065): one more minted name whose old name described a
  DIFFERENT value — arguably the more honest output, but it is a real
  cost, and it is the one place the blur was doing real (if lucky) work.
  Population: small — module-lane ask counts are identical (877/877),
  total asks +2.
- The other 11 files: the same few scopes as the ask delta, compounded by
  the stub's byte-derived answers (a shifted prompt yields a different
  draw, whose name feeds `usedNames` and shifts later draws). Under the
  real model this class exists too, plus model variance — the eval (034)
  owns that verdict; M1/M2 show the perturbation seeds here are strictly
  fewer and correct-ward.

## M4 — regression sanity

- `cargo test --workspace`: **all green** (humanify-core 841 + cli 74 +
  model 82 + the rest; the six red-first tests pass).
- `npm run check`: **ALL 12 STAGES PASSED** (typecheck, lint, rust:fmt,
  rust:clippy, knip, knip:prod, census:clones, unit, rust:unit,
  rust:build, rust:format-golden, e2e — the e2e runs fresh AND
  `--prior-version` legs against the stub with byte-determinism
  double-run).
- **Kept parity files: ZERO byte edits.** `test/parity` pins
  structuralHash as PRESENCE + PARTITION (`matching_test`:
  “bytes are serializer artifacts”), decision equality
  (`wp21-cascade-synthetic.json`), the statement-hash walk's own vectors
  (that walk always kept numbers exact — untouched by exp093), and
  formatter goldens (no hash bytes). The frozen cascade probe needed no
  change once the fixtures kept their intent; no kept file pins blurred
  number bytes.
- **Deliberate spec edits** (annotated `fixed:{was,why}` in-source, all
  in `cascade_test.rs`): `SINGLETON_V2` and the one-sided-signal variant
  change `return 2` → `return 1` (the differing number no longer
  blur-joins, so the singleton gate under test would never have been
  reached); the demote-reparks twins differentiate by same-length strings
  (`"a"`/`"b"`) instead of numbers (`1`/`2`) so the class collision the
  test needs survives via the string-length rule.

## The era gate (hashVersion 2 → 3)

Traced at three levels. Unit: the two red-first tests
(`an_older_hashversion_prior_is_rekeyed_by_content_never_by_hash`,
`an_older_hashversion_prior_without_content_mints_everything`) prove a
manifest stamped 2 whose bytes WOULD join still carries by CONTENT only —
the version gate is the only protection when a number-free factory's
bytes coincide across the era; without content, everything mints.
Runtime, M3 both legs: the real TS-era manifest re-keyed 1624/1624
entries, 0 groups refused, on both binaries. The ledger re-derivation
path is untouched by design (`STATEMENT_HASH_VERSION` stays 2 — the
statement-hash walk always kept numbers exact; `split-ledger.json`
`hashVersion` byte-identical between legs).

## Recommendation — LAND

- Zero real pairs lost everywhere measured (M1 recall byte-stable; M2
  +2 correct pairs, the single "loss" a legal-equal interchangeable
  reassignment; unmatched unchanged).
- Ambiguity strictly down (M2: −2 blur events resolved into correct
  matches, binding stillAmbiguous 455→445, 61 promotions to the
  unique-hash tier; M1 tier attribution strictly better on 4 packages).
- Asks not identical — but the delta is 33 rows of 2,342 with every row
  attributed: the matching IMPROVEMENTS (fewer, better-targeted asks at
  the changed scopes), the era's `lib_` renames inside one vendor ask,
  and ±2 counter drift. One real cost case found (the 60000→25000
  constant that now re-names instead of silently carrying an old name
  onto a new value) — a defensible trade and bounded at a handful.
- Gate fully green; no parity spec edits; the vendor-name re-derivation
  is a one-time generation cost, as predicted by the study.
- Follow-ups for the ledger: (1) the vendor-inclusive M2 A/B once
  `fix/match-instrument-scale` lands; (2) the changed-code constant case
  (value-changed module bindings no longer carry) belongs with the
  exp062–065 changed-code frontier, not with the fingerprint policy.

## Diffstat

```
 15 files changed, 435 insertions(+), 150 deletions(-)
 crates/humanify-cli/src/unminify.rs                |   6 +-
 crates/humanify-core/src/graph.rs                  |   2 +-
 crates/humanify-core/src/hash/mod.rs               |   2 +
 crates/humanify-core/src/hash/serialize.rs         |  81 +++++-----
 crates/humanify-core/src/hash/serialize_test.rs    | 331 ++++++++++++++++++++ (new)
 crates/humanify-core/src/matching/cascade/cascade_test.rs |  22 ++-
 crates/humanify-core/src/matching/statement_align.rs |  55 ++---
 crates/humanify-core/src/matching/statement_context.rs |   4 +-
 crates/humanify-core/src/modules.rs                |  27 ++-
 crates/humanify-core/src/modules/vendor_content.rs |  57 ++--
 crates/humanify-core/src/modules/vendor_content/vendor_content_test.rs | 8 +-
 crates/humanify-core/src/twins/gates.rs            |   8 +-
 crates/humanify-core/src/unpack/bun.rs             |  42 ++--
 crates/humanify-core/src/unpack_test.rs            | 103 ++++++-
 docs/responsibility.md                             |   3 +-
```
