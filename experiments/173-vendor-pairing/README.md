# exp173 — vendor pairing blind to the pipeline's own names

STATUS: BUILT (2026-10-31). Branch `exp173-vendor-pairing`, cut from
`exp171-names-default` (`c396c53bf`, not on main yet). The code commit is
`896dfca16`, and every run below was scored on it. The fix is real and
small, and it turns the e2e fixture `bun-text-assets` green. **But exp171
blamed two eval-scale symptoms on this defect, and neither one comes from
it** (rule 9: this file corrects exp171's README):

1. names mode's vendorReal of ~10,800 is a real dependency upgrade
   (highlight.js 10 → 11 on 2.1.197→198), counted in formatted lines;
2. the cold bimodality on 2.1.197→198 comes from one app `__esm` init
   binding that a tie in the reconcile sometimes leaves to a model draw.

On Claude Code the fix is inert: every eval column is inside the names
bands, and novel/realLn are byte-equal. The names-mode reference does not
need re-recording.

## Totals first

- **The defect (finding #90, exp171 defect 2) is fixed.** The content
  carry (`modules::vendor_pairing`) used to compare the prior's vendor file,
  which names mode had named and formatted (exp169), with the fresh raw
  factory, token by token. The pipeline's own names hid a changed module's
  twin, so the module took a new path. When the prior tree holds
  `.humanify/vendor-humanified.js`, both sides are now read through
  `blind_text`:

  - formatted by the pipeline's formatter;
  - every binding of the factory, declaration and resolved reference,
    spelled `_`, with a shorthand property expanded (`{load}` →
    `{load: _}`, as a rename writes it);
  - free names (other modules, `require`, globals), property names and
    literals kept.

  The pipeline names bindings only, so nothing it assigns can decide a
  pair. Any other prior reads as written, so off and report mode cannot
  change. `bun-text-assets` passes, and its `KNOWN_GAPS` entry is deleted
  (`KNOWN_GAPS` is now empty).

- **The fix is inert on Claude Code's eval.** In names mode almost every
  Claude Code vendor module is named by the index (1,384 `index`, 207
  carried `llm`, 29 assets, 1 fallback on 2.1.198). The content carry only
  sees modules nothing else named: on 2.1.197→198 that is 29 candidates,
  all app text assets, and 1 carry, the same before and after. The 4-pair
  label `exp173-names-r1` sits inside the names bands on every column.
  novel/realLn are 4,188 / 416,377, byte-equal.
- **vendorReal is real change, not this defect.** On 2.1.197→198 names
  mode's vendor diff is 8,000 lines in 143 files. All 143 files hold a
  factory whose raw bundle text changed (structural hash moved). 7,998 of
  the lines are highlight.js: 2.1.198 ships highlight.js 11 (its
  `beginScope`, `derivationStrict` and so on appear in 2.1.198's bundle,
  not in 2.1.197's). No file moved path in names mode: the index names
  them. Off mode reads the same upgrade as ~1,135 lines only because its
  bodies are one minified line each: its scorer re-paired 61 relocated
  grammar files at 2–4 lines apiece. exp169 already measured this effect
  on 2.1.216 (+904 lines, all formatting). About 22% of names mode's
  8,000 lines (~1,780) differ only in identifier spelling inside those
  genuinely changed modules: naming stability for changed code, not
  pairing. **So the bar "vendorReal back near off's ~1,300" cannot be met
  without hiding real change, and it should be dropped.** exp171's
  "+9,500 = defect 2" is retracted.
- **The 2.1.197→198 cold split is not vendor-driven (hypothesis
  REFUTED).** The extra ~400 noise statements / ~15,000 noiseLn in the
  "high" repeats come from ONE app binding: the `__esm` init of the
  error-classes module (minified `dt`, ~500 call sites in 275 statements).
  Its module changed on this hop (a `CliUserError` class was added). In
  every names run it gets 387 exact-match votes for its prior name
  `initErrorUtils`, plus ~14 stray votes for other `init*` names, and does
  not settle from them. Off mode settles the same binding by module vote,
  with 401 votes. The reconcile then decides:

  - it applies `initErrorUtils` (reconcile-asymmetric): the "low" mode
    (ref r2; exp173-names-198);
  - or it ABSTAINS on a contested candidate set (`initErrorUtils|initPathCache`,
    or 4–5 `init*` names), so the coverage sweep asks the model, which
    draws a new name (`initAnthropicClientModule`, `initModule`,
    `initializeAnthropicClientCore`) carried into every caller: the "high"
    mode (ref r1, r3; exp173-names-r1).

  With the fix, two cold repeats of the pair landed one low (505 / 6,966)
  and one high (924 / 22,419). The split stays, exactly as the mechanism
  predicts. Follow-up lane: why names mode does not settle that init
  binding by module vote the way off mode does.

## The fix

- `modules::vendor_pairing::blind_text` / `blind_module_features` (the
  pairing owns the features). The masking reads one parse with oxc
  semantics: `BindingIdentifier`, plus `IdentifierReference` resolved to a
  symbol. The shorthand test reuses `emit::cjs::babel::is_shorthand_value`,
  now `pub(crate)`. Text the formatter refuses is masked unformatted; text
  that does not parse is read as given.
- `naming::vendor_bodies::prior_bodies_named(root)`: the prior tree holds
  `VENDOR_NAMING_SOURCE`. `unpack::bun::carry_by_content` picks the
  reading once per run, for both sides and the IDF.
- The name carry needed no change. Once the changed module keeps its path,
  `name_vendor_bodies`' prior (the joined named text in path order) carries
  its names by the driver's exact/close match. On the fixture, v1.1.0's
  codec file differs from v1.0.0's by the one edited string.
- `docs/responsibility.md`: the vendor-pairing row records the blind
  reading.

Tests, each watched red first:

- `vendor_pairing_test::a_changed_module_pairs_with_its_pipeline_named_prior`:
  the fixture's codec, named and formatted, against its raw edited
  successor. As written: no pair (the defect). Blind: paired.
- `vendor_pairing_test::blind_text_masks_every_binding_and_keeps_free_names_and_properties`.
- `unpack_test::a_changed_module_keeps_its_file_when_the_prior_named_its_bodies`:
  a whole unpack hop on a prior whose vendor bodies were renamed and which
  carries the marker. Red with the gate forced off ("re-asked").
- e2e `bun-text-assets`: was a declared known gap, now passes; the entry
  is deleted.

## Gate table

| check                                                            | result                                                                                                                   |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `npm run check` (at `896dfca16`)                                 | **ALL 13 STAGES PASSED**; e2e 18 pairs passed, 0 known gaps                                                              |
| `bun-text-assets` e2e                                            | **passes**; KNOWN_GAPS entry deleted                                                                                     |
| eval `exp173-names-r1` (4 pairs, names, scratch, cold)           | 4/4 exit 0, cache +0, 4/4 boot OK                                                                                        |
| warm self-hop (2.1.216)                                          | **byte-identical, 0 cache writes**                                                                                       |
| cold self-hop (2.1.216)                                          | 8 ln (names range 8–8) ✓                                                                                                 |
| eval `exp173-names-198` (197→198 only)                           | exit 0, cache +0, boot OK; warm self-hop byte-identical; cold self-hop on 2.1.198 18 ln (no range on record for 2.1.198) |
| neutrality, off mode (`library.matching=off`, 85→86, warm)       | **NEUTRAL**: 0 files / 0 lines, cache +0 / +0, exit 0 / 0 (baseline `c396c53bf`)                                         |
| neutrality, report mode (`library.matching=report`, 85→86, warm) | **NEUTRAL**: 0 files / 0 lines, cache +0 / +0, exit 0 / 0                                                                |

## Results table (names mode, vs `ref-names-0357696e-r1..r3`; names bands)

TOTAL over 4 pairs:

| KPI                   | ref r1–r3 (range)           | **exp173-names-r1** | verdict               |
| --------------------- | --------------------------- | ------------------- | --------------------- |
| novel / realLn        | 4,188 / 416,377             | **4,188 / 416,377** | **= (bar)**           |
| treeLn                | 144,651–146,767             | 146,084             | ~0 (±2,116)           |
| cleanReal             | 83,959–85,001               | 84,465              | ~0 (±1,042)           |
| noise / noiseLn       | 2,740–3,137 / 49,830–65,539 | 3,137 / 65,085      | ~0 (±397 / ±15,709)   |
| reloc                 | 1,368–1,385                 | 1,363               | −5 vs r3 (band 17)    |
| newName               | 3,611–3,645                 | 3,638               | ~0                    |
| mints                 | 211–233                     | 212                 | ~0 (bar: not worse ✓) |
| vendorLn              | 1,579–1,587                 | 1,579               | =                     |
| vendorReal            | 10,832–10,928               | 10,928              | ~0 (±96)              |
| app: noise / noiseLn  | 2,696–3,093 / 49,309–65,027 | 3,094 / 64,573      | ~0                    |
| app: mints / newName  | 197–222 / 3,583–3,615       | 207 / 3,608         | ~0 (bar: not worse ✓) |
| app: novel / realLn   | 4,169 / 415,935             | 4,169 / 415,935     | =                     |
| vendor: vendorReal    | 10,696–10,792               | 10,792              | ~0                    |
| vendor: mints         | 4–10                        | 3                   | ~0                    |
| cross: treeLn (stmts) | 272 (14)                    | 272 (14)            | =                     |

Per pair (noise stmts / noiseLn / vendorReal; ref range over r1–r3):

| pair    | ref r1–r3                                                           | exp173-names-r1                 | exp173-names-198              |
| ------- | ------------------------------------------------------------------- | ------------------------------- | ----------------------------- |
| 85→86   | 1,493–1,507 / 33,518–33,950 / 162–170                               | 1,490 / 33,569 / 172            | —                             |
| 118→119 | 421–442 / 4,065–4,302 / 506–510                                     | 432 / 4,193 / 506               | —                             |
| 197→198 | **518 / 7,046** (r2) · **913–918 / ~22,200** (r1, r3) / 8,971–9,055 | **924 / 22,419** / 9,057 (high) | **505 / 6,966** / 8,955 (low) |
| 215→216 | 289–293 / 4,890–4,938 / 1,193                                       | 291 / 4,904 / 1,193             | —                             |

The bands do not move, so **the names-mode reference stays valid; nothing
needs re-recording**. exp171 can merge before or after this lane.

## What the 197→198 vendorReal is made of (`exp173-names-198`)

- 143 changed vendor files, 8,000 diff lines. 0 files with an unchanged
  structural hash, so every changed file's raw factory changed. 0 path
  moves.
- highlight.js 7,998 lines (core.js 922, then the language grammars);
  `@anthropic-ai/sandbox-runtime` 2.
- With every non-property identifier masked, 6,286 of the 8,064 lines still
  differ: ~78% code change, ~22% identifier spelling inside changed modules.
- Off mode charges the same upgrade ~2–4 lines per file, because its
  bodies are unformatted one-liners.

## The cold-variance evidence

- `noise-statements.ts` (this directory) dumps the scorer's NOISE
  statements per label. 305 hashes are noise in ref r1 and r3 but clean
  in r2, 14,144 lines in all. 275 of them carry the single rename
  `initErrorUtils → initAnthropicClientModule`.
- The trails for that binding (`--diagnostics` `strategyTrails`, loc
  `11473:6` of 2.1.198's era text), in the four names runs and three off
  runs:

| run              | votes for the prior name         | outcome                                             | name                            |
| ---------------- | -------------------------------- | --------------------------------------------------- | ------------------------------- |
| off r1/r2/r3     | 401 exact + 13 close             | settled by module-vote                              | the base's own name, kept       |
| names ref r2     | 387 exact + 13 close (+14 stray) | reconcile-asymmetric applied                        | `initErrorUtils` (kept)         |
| exp173-names-198 | same                             | reconcile-asymmetric applied                        | `initErrorUtils` (kept)         |
| names ref r1     | same                             | reconcile abstained (2 candidates) → coverage sweep | `initAnthropicClientModule`     |
| names ref r3     | same                             | reconcile abstained (4 candidates) → coverage sweep | `initModule`                    |
| exp173-names-r1  | same                             | reconcile abstained (5 candidates) → coverage sweep | `initializeAnthropicClientCore` |

The competing candidates are other bindings' cold-drawn `init*` names, so
whether the reconcile abstains is itself a cold draw. No vendor module
takes part, and the content carry made the same one asset carry in all
these runs.

## What this does not look at (rule 8)

- The fix's effect is shown on the fixture and the unit hops. On Claude
  Code, the content carry has no fallback vendor module to pair on these
  four pairs in names mode. Bundles where the index leaves many CommonJS
  modules to the model (bun-dev apps outside the index's coverage) are
  where it matters. No eval scores them across versions.
- Precision of the blind reading on real data is not separately measured.
  It is the same matcher with the same thresholds. A names-mode prior
  reads like an unnamed one after masking, and the fresh side's minified
  bindings were already masked by the lexer. The measured
  `pairsim3.py` precision (0 wrong in 475) was on off-mode trees.
- The ~22% identifier-spelling share inside changed vendor modules is a
  regex approximation (`/work/exp173-data/scripts/nameonly.py`), not the
  scorer.
