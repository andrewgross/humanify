# exp200 — identity transforms: the code-identity hash ignores the minifier's STYLE

STATUS: PHASE 1 BUILT AND HOP-PROVEN, NOT MERGED (2026-11-08), branch
`exp200-identity-transforms`, rebased on main `d293aa886` (exp201 landed).
Steady state on 2.1.179 -> 2.1.181: vendor noise 53,338 -> 6,254 lines
(-88%), real change unchanged; the 2.1.197 -> 2.1.198 control is unchanged.
COST: the first hop after the upgrade (the match-key era bump) pays a
one-off ~34,000-38,000 vendor noise lines (§5). The full eval was NOT run.
Phase 2 (re-fingerprinting the library index) is costed only (§6).

Andrew, 2026-11-07: "in our hash we can treat those ops the same, and have
an easy way of adding identity transforms as plugins".

## Totals

- **Proof (§5): steady-state 2.1.179 -> 2.1.181, each binary on its OWN
  previous hop: vendor noise 53,338 (main) -> 6,254 (this branch), vendor
  files relocated 101 -> 11, src noise 4,134 -> 2,884; novel 1,145 /
  realLn 140,489 identical. Control hop 2.1.197 -> 2.1.198: vendor noise
  1,978 / 1,978, novel 1,262 / realLn 136,399 identical.** The main
  control matches exp201's reference on this pair (~53,300 / ~35,600).
- **The era-bump hop costs once:** the first run of the new binary on a
  main-written prior (manifest `hashVersion 4`) re-keys every vendor
  module by content. That re-key was broken for names-mode priors (it
  compared the prior's FORMATTED vendor files with raw bundle text: 194 of
  1,594 joined) — fixed here (both sides read through the formatter: 1,553
  of 1,594). It still re-derives every module's runtime identifier and
  re-names its body, so that hop reads 38,436 (2.1.178 -> 179) and 33,917
  (2.1.196 -> 197) vendor noise lines against main's 1,176 and 12. A
  follow-up can remove it: carry the identity (file, identifier, body
  names) through the stale-era re-key as the exact carry does (exp194).
- **Census, 2.1.179 -> 2.1.181:** 177 factory hashes gone / 167 new; 129
  of them pair up as the same module. Every one of the 129 differs ONLY by
  three minifier spellings: a concise arrow body (`x => E` vs
  `x => { return E; }`, 121 modules, one of them also needing the
  parentheses rule), and re-lettered class-private names (`#H` -> `#e`, 11
  modules; 3 need both). **With the match key's new transforms 129 / 129
  pairs hash equal again, and the hop's factory turnover drops from 177
  gone / 167 new to 10 gone / 0 new** (the 10 are modules really removed).
- **Controls stay different:** on 2.1.197 -> 2.1.198 (highlight.js 11, a
  real upgrade) 0 of 104 changed pairs are restored; on 2.1.120 -> 2.1.121
  (library code really changed: added statements, native optional chaining
  and class fields instead of the lowered forms) 1 of 41 (a pure private
  re-lettering). 2.1.71 -> 72 and 2.1.87 -> 89 are module REMOVALS (229
  and 76 gone, 2 and 7 new), not restyles; 1 module each is restored.
- **The registry:** `crates/humanify-ag-core/src/hash/identity_transforms.rs`.
  A transform = one entry (name, version, semantics-preservation condition,
  the walks that apply it) and one rule; a form declares a `TransformSet`
  instead of the old `wrapper_spelling` bool. exp094's wrapper spelling is
  entry one, byte-identical (every `hash_forms_pin_test` pin unchanged at
  commit `1170a35a4`). Two new entries: `concise-arrow-body`, `parentheses`.
  Private names are a form SETTING (`Privates::Slots`), not a transform.
- **Phase 1 changed ONE persisted form: the match key, v4 -> v5** (the
  vendor manifest's `hashVersion`): function fingerprints, factory
  structural hashes (the exact carry), statement contexts. The library
  index's forms (`units/3`, `vendor-content-key/4`) are NOT changed — they
  keep the v4 settings, explicitly (§3). Two in-run forms were added:
  `carry-content-key` (the stale-era carry's re-key) and
  `match-key-privates-verbatim` (the twin gates' first comparison).
- **Phase 2 cost (not built):** the stored bundles and sourcemaps of every
  indexed version are still on disk (`/work/exp152-data/work/*/build/`), and
  the builder already reuses a finished build — so re-fingerprinting is a
  fingerprint + merge pass, no install, no re-bundling: an estimated 3-4 h
  wall, ~170 GB of new module shards, a new ~43 GB index. Expected gain: on
  every version 2.1.69..2.1.179, 110-136 vendor modules (median 125; 11,315
  module-versions over 93 versions) would get the whole-module index answer
  (a real source path) that 2.1.181's restyled copies got; about 37 per
  version are today named by the model, not placed by the index at all.

## 1. Census (step 1)

Tools (kept under `/work/exp200-census`, never in the repo — they read
Claude Code text): a scratch binary over `humanify-ag-core` that classifies
a bundle's factories exactly as the pipeline does (its v4 hashes reproduce
every walk manifest `structuralHash`) and prints each factory's canonical
token stream; a pairing script (mutual-best Jaccard of 5-token shingles,
short identifiers masked, score >= 0.3); `diff` over the two token streams
of each pair; and a classifier over the differing runs.

| hop | gone / new (v4) | paired | difference classes in the pairs | restored by match-key/5 | gone / new (v5) |
| --- | --- | --- | --- | --- | --- |
| 2.1.179 -> 2.1.181 | 177 / 167 | 129 | concise arrow 118, private alphabet 8, both 3 | **129** (concise+parens alone 118, privates alone 8) | **10 / 0** |
| 2.1.71 -> 2.1.72 | 229 / 2 | 1 | private alphabet 1 | 1 | 228 / 1 |
| 2.1.87 -> 2.1.89 | 76 / 7 | 3 | real 2, private alphabet 1 | 1 | 75 / 6 |
| 2.1.120 -> 2.1.121 | 55 / 54 | 41 | real (new statements, optional chaining / class fields not lowered) 40, private 1; concise arrow rides along in 26 | 1 | 54 / 53 |
| 2.1.197 -> 2.1.198 (control) | 149 / 149 | 104 | real (highlight.js 11) 104; concise arrow rides along in 33 | **0** | 149 / 149 |

Per class: would a rule for it ALONE restore the hash?

| class | modules (179->181) | alone restores | condition |
| --- | --- | --- | --- |
| concise arrow body | 121 | 117 of 118 concise-only; the 118th also needs parentheses | always (ECMA-262 ConciseBody = evaluate and return) |
| parentheses | 1 (inside a concise body: `x => ({...})[k]` vs `return {...}[k]`) | with concise | always: the tree keeps the grouping; a printer adds or drops parens by position |
| class-private alphabet | 11 | 8 of 8 private-only | consistent re-lettering per declaring class is alpha-renaming |
| `!0`/`true`, `void 0`, comma-vs-statements, `if`-vs-`&&`, property quoting, number spelling | 0 | — | not observed on any of the five hops: not built |
| syntax lowering (`a?.b` vs `a == null ? void 0 : a.b`, class fields vs constructor assignment) | seen on 2.1.120 -> 121 only, always next to real code change | — | NOT semantics-preserving in general (`[[Define]]` vs `[[Set]]`) or needs temporaries matched: not a transform |

Side finding: one factory per version (e.g. 2.1.179 and 2.1.181) hashes as
the literal `null`: `factory_structural_hash` deserializes the ESTree JSON
with `unwrap_or(Value::Null)`, and that factory's JSON contains a lone
surrogate escape serde refuses. Any two such factories collide. Not fixed
here (one module, not a style question) — a follow-up.

## 2. The registry (step 2)

`hash/identity_transforms.rs`:

- `Transform` (the variant), `TRANSFORMS` (one `TransformSpec` each: name,
  version, condition, walks), `TransformSet` (what a form declares; iterates
  in registry order).
- `Transform::rewrite(node_type, node) -> Rewrite`: the ONE rule per
  transform. Three kinds of rewrite, so no transform ever copies a module:
  `TypeToken` (rename the node's type token — wrapper spelling),
  `Inner(child)` (walk a child in the node's place — parentheses), `Node`
  (a small replacement node — the concise body becomes
  `BlockStatement[ReturnStatement]`, copying only the body expression).
- Both walks that carry transforms — the canonical serializer and the
  statement hash — apply a set through the same `Rewrite`
  (`the_statement_walk_applies_a_set_it_is_given`). The align walk has none
  (`every_transform_a_form_lists_is_applied_by_its_walk`).
- Adding a transform = one variant + one `TRANSFORMS` entry + its rule +
  its tests + the version bump of every form that lists it.
- `docs/responsibility.md` has the new row ("Which minifier-STYLE spellings
  does a hash form treat as the same code?"); `docs/matching-architecture.md`
  §3.2 records the as-built normaliser.

`Privates::Slots` (canonical walk): a private name is the slot of the
innermost walked class body that declares it (`P=#$n`, first occurrence); an
inner class's own `#x` shadows; a name no walked class declares is keyed by
name. Two fields collapsed into one, or a shadowing change, stays different
(`private_names_that_mean_different_fields_stay_apart`).

## 3. Which forms phase 1 changed, and why those (step 3)

| form | stamped in | read by | phase 1 |
| --- | --- | --- | --- |
| `match-key` | vendor manifest `hashVersion` | function fingerprints (prior matching), factory `structuralHash` (exact carry, manifest order), statement contexts | **v5**: + concise-arrow-body, + parentheses, privates slotted |
| `match-key-privates-blinded` | — (in-run) | twin gates' masked fallback | moves with match-key |
| `match-key-privates-verbatim` (new) | — (in-run) | twin gates' first comparison | match-key with privates verbatim — the twins TRANSFER private spellings (`#a` -> `#registryCache`), so a re-lettering must reach their masked branch |
| `carry-content-key` (new) | — (in-run) | stale-era carry re-key (`load_prior_vendor` -> `rekey_prior_by_content`) | the vendor-content walk under the current match key |
| `vendor-content-key` | index manifest `module` | index whole-module print (builder AND run-time lookup) | **unchanged** (v4 settings) |
| `units` | index manifest `units` | index unit prints, edges, names | **unchanged** (v4 settings) |
| `statement` | split ledger `hashVersion` | split inheritance, statement twins | **unchanged** — the eval scorer reads it for the advisory `spellingIdenticalLines` column (`dev eval stmts`), so changing it moves the measurement; and a bump re-derives the prior ledger through a bijection proof that a class merge refuses. Ready to turn on (one set entry) once the scorer pins its own spelling form |
| `kpi-statement`, `identity-key`, `rename-signature`, `align-*` | | | unchanged |

**The decoupling, said out loud.** Until this lane `VENDOR_CONTENT` and
`LIBRARY_UNITS` were written `..MATCH_KEY`, and `forms_test` asserted
`VENDOR_CONTENT.version == MATCH_KEY.version`: the vendor content key WAS
both the index's module print and the carry's re-key. A match-key change
therefore could not be made without re-keying every built index. The split:
a private `MATCH_KEY_V4_SETTINGS` holds the settings every index was
stamped with; the two index forms derive from it (their stamps and bytes
are unchanged — `version_stamps_are_pinned`: `module=vendor-content-key/4
units=units/3`; the pin test's index-key lines `V` and unit lines are
line-for-line identical); the carry's re-key moved to its own form,
`carry-content-key`, which follows the match key. The new test
`the_index_forms_keep_the_settings_they_were_stamped_with` replaces the old
equality. **This is a decision for Andrew** (the brief asked not to
decouple silently): the alternative is to keep them coupled and ship phase
1 and phase 2 together.

What moved in the pins (each digest re-recorded, lines diffed against the
pre-change run): canonical — 6,898 of 21,474 lines, ONLY the match-key and
match-key-blinded columns (identity, rename signature, every slot mapping
unchanged); graph — function rows only (module bindings unchanged);
modules — 42 of 496 factory hashes (index keys unchanged, carry-key lines
added); the two matching-helper pins that hash under the match key.
`match_stage_and_transfers_are_pinned` — the match outcomes on the e2e
fixtures — did NOT move.

Red/green: `minifier_style` (serialize_test), the commander-shaped factory
tests in `modules_test` (`a_restyled_factory_keeps_its_structural_hash`,
`a_real_change_in_a_restyled_factory_changes_its_hash`,
`the_carry_content_key_ignores_the_restyle_and_the_index_key_does_not`) were
written first and failed; the parentheses test was added after the census's
one leftover. Two exp094 assertions ("a concise arrow never unifies") were
superseded and rewritten; the frozen TS probe's bucket partition
(`matching_test`) now carries one declared merge (`function(){return
leaf()}` with `() => other()`, the same code modulo local names).

## 4. What the eval will need

`cargo dev eval score <label>` on this branch (names mode, scratch bases),
run ALONE, compared with `cargo dev eval leaderboard ref-names-111401fe-r1
<label>` (exp201's five-pair names reference; bands `byScorer.2`). On
scratch bases every base is rebuilt by the candidate with no prior, so
every scored hop is the steady state — the era-bump cost (§5) never
appears in the eval; it appears once in a walk or a user's first run on a
main-written tree. pairs.json now holds 2.1.179 -> 2.1.181 (third pair):
expect its vendor noise to fall from ~53,300 toward ~6,000 and every other
pair's vendor columns to stay inside the bands. Hold columns: novel /
realLn must not move (band 0).

## 5. Proof (step 4)

All runs cold, live gpt-oss-20b, the walk's per-hop config (names mode, the
`cb64c1f5…` index), scored by scorer v2 (`cargo dev eval walk-report`).
Candidate = this branch before the rebase (`232f32fce`'s tree); control =
main `582356d90` built the same way. Data: `/work/exp200-runs/`.

**(a)+(b) Steady state — each binary walks two hops on its own output**
(`/work/exp200-runs/chain`; hop 1 starts from the walk's tree):

| hop | binary | vendor noise | vendor real | vendor files relocated | src noise | nameOnly | novel | realLn |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2.1.179 -> 2.1.181 | main | 53,338 | 35,662 | 101 | 4,134 | 10,924 | 1,145 | 140,489 |
| 2.1.179 -> 2.1.181 | **exp200** | **6,254** | **15,986** | **11** | **2,884** | 9,972 | 1,145 | 140,489 |
| 2.1.197 -> 2.1.198 (control: real upgrade) | main | 1,978 | 7,883 | 0 | 1,542 | 6,678 | 1,262 | 136,399 |
| 2.1.197 -> 2.1.198 (control: real upgrade) | **exp200** | 1,978 | 7,873 | 0 | 1,544 | 6,664 | 1,262 | 136,399 |
| 2.1.178 -> 2.1.179 (era bump) | main | 1,176 | 103 | 2 | 470 | 2,144 | 512 | 81,908 |
| 2.1.178 -> 2.1.179 (era bump) | exp200 | 38,436 | 12,718 | 25 | 3,720 | 4,868 | 512 | 81,908 |
| 2.1.196 -> 2.1.197 (era bump) | main | 12 | 0 | 0 | 8 | 680 | 167 | 34,286 |
| 2.1.196 -> 2.1.197 (era bump) | exp200 | 33,917 | 9,667 | 17 | 2,194 | 2,894 | 167 | 34,286 |

Vendor name sources on 2.1.181: exp200 1,557 carried, 0 index / main
1,390 carried, 154 named by the index (the restyled modules, given new
source-path files — the 56,222-line walk hop). The walk itself read 56,222
/ 32,782 on this hop (another cold draw of the same main pipeline). The
"vendor real" drop is the relocations going away: a module moved to a new
file was charged as real vendor lines; the TOTAL novel / realLn, the hold
columns, are byte-equal in every row. Single cold runs: rule 11 applies to
the small columns (src noise, nameOnly), not to a 47,000-line vendor move
with a mechanism (the carry count) logged behind it.

**Single hop from the WALK's prior (the era-bump path), first attempt:**
before the carry-key fix the re-key joined 194 of 1,594 modules and the
hop read vendor noise 58,850 (main 53,334); 2.1.197 -> 198 read 8,759
(main 1,978). After the fix: 1,553 / 1,470 joined, vendor noise 85,038 /
35,695 — every identifier and body name re-derived (above). That is what
§Totals calls the one-off era-bump cost.

**(c) Gate** — `cargo dev check`, bare, under the heavy lock, after the
rebase onto main `d293aa886`; the clean checkout is `0dba3d33e` (a detached worktree, fresh `npm ci --ignore-scripts`, cold build). ALL 11 STAGES PASSED in both:

| stage | worktree | clean checkout |
| --- | --- | --- |
| shims | PASS | PASS |
| no-private-addresses | PASS | PASS |
| rust:fmt | PASS | PASS |
| rust:clippy | PASS | PASS |
| census:clones | REVIEW (advisory; no group touches this lane's code) | REVIEW (same) |
| census:concurrency | PASS | PASS |
| census:slop | PASS | PASS |
| rust:unit | PASS | PASS |
| rust:build | PASS | PASS |
| format-golden | PASS | PASS |
| e2e | PASS | CLEAN_E2E |

(The worktree's first attempt failed `rust:unit` on one webpack test only
because this new worktree had no `node_modules` — the setup step
`npm ci --ignore-scripts`; green after it.)

## 6. Phase 2 — re-fingerprinting the index (design and cost only)

**What changes:** `LIBRARY_UNITS` and `VENDOR_CONTENT` derive from the
current match key's transforms instead of `MATCH_KEY_V4_SETTINGS`
(`units/4`, `vendor-content-key/5`); the edge and names tables re-key with
the unit prints. `literals/1` is untouched (source-mode literal extraction
reads no hash form).

**Can it be re-derived without re-bundling?** Yes. The as-built store
(`/work/exp152-data`) kept every version's work directory: installs
(298/300 sampled) and both slices' `req`/`dyn` bundles with their
sourcemaps (289/300 sampled; ~2.4 MB of bundle per version), each stamped
with `.recipe`. `ensure_build` (humanify-ag-index `build.rs`) skips a build
whose stamp matches, and the shard key includes the fingerprint version, so
a build with the new stamps finds every module shard missing, reuses every
install and bundle, re-runs only `fingerprint_bundle`, and reuses every
literal shard.

**Cost estimate:** ~158 GB of bundle text to parse and fingerprint
(66.5k versions x ~2.4 MB). At this host's measured rate for the pipeline's
parse + classify + hash (17 MB in 2 s single-core) times ~3 for the unit,
edge and names walks, ~15 core-hours, under 1 h at 30 jobs; the merge
measured 1.7 h at 30 jobs and 99 GB peak RSS (exp162 streaming finish).
Total ~3-4 h wall, as the only heavy job. Disk: the new module shards are
the size of the old ones (79 + 88 GB) since the shard key differs, plus a
new ~43 GB index; 634 GB free on `/work` today. The old index stays
readable only by binaries before the bump (a run refuses a mismatched
stamp); pairs.json's `libraryIndex` must be re-pointed and the names-mode
references re-scored.

**Expected recall gain:** the restyled 2.1.181 copies of these modules got
whole-module answers (a source path) from the index built with Bun's
current style. Counting, on each of 2.1.69..2.1.179, factories whose v5
shape equals one of the 1,061 path-placed 2.1.181 factories while their v4
shape matches no 2.1.181 factory: **110-111 per version on 2.1.69-2.1.120, 125 on 2.1.121-2.1.148,
136 on 2.1.149-2.1.179; median 125, 11,315 module-versions over 93
versions.**
On 2.1.179, of those 136: 96 are index-placed today but only by package
(hash-derived file names), 37 are named by the model (no index placement),
3 by a declared identity. A lower bound: it only sees modules still present
at 2.1.181, and only the whole-module answer (unit-level recall would also
rise wherever the app's copy is concise-spelled and the index's is not, or
the reverse).
