# exp183 — a bundle never claims its own package

STATUS: LANDED on branch `exp183-self-claim` (off main `3218b7b8d`; the
final commit is the HEAD of that branch). Scorer judgment, the exp172 check
and warm neutrality all ran and read as below; the gate is 13/13 in the
worktree and from a clean checkout. The iv index is NOT adopted —
`pairs.json` is not repointed, no index moved (adoption is a later step).

## The problem

exp177 built a larger index
(`/work/exp177-data/ix/full-bun-names-iv-asof20261025`, id `8462c502…`):
a newly indexed version of `agents` depends on `just-bash`, and the index
records a package's dependency code under the dependency's name — so the
index knows `just-bash`'s compiled output. The dev app `cli_just-bash` IS
the published just-bash package, and in that index ~3.9 MB of its OWN code
was claimed as the library `just-bash`. The scorer counts those bytes as
`<self>` (reported apart, so app-claimed stayed 0 and the precision bar
held), but in names mode such code would be vendored under
`vendor/just-bash/` — the app's own code leaving the app.

## The fix

One owner of the question "what is this bundle's own package?":
`crates/humanify-core/src/modules/own_package.rs` (the row in
`docs/responsibility.md`). Two askers hand it their own view of the
bundle's modules:

- **library discovery** (`libdetect::own_fence`, every bundler): the
  reference graph's module groups and discovery's claims, once the
  fixpoint has run. The fence asks the entry rule ALONE
  (`own_packages_from_entries`): the one package claiming MOST of every
  entry module's bytes (the modules the bundle's trailing glue runs,
  `app_factories::entry_factories`) is the bundle itself — a library is
  never the program it is the entry of. All entries must agree. Not
  refused by "another package depends on it": the entry is the app by
  construction. The own package's claims are set apart into
  `Discovery::self_claims` (and out of `library_class`/`boundary`/`queue`)
  — that code becomes APP code: never vendored, never restored from the
  index, named like app code.
- **the CommonJS app-module decision** (`modules::app_factories::decide`,
  exp172): asks `own_packages` — exp172's front rules (manifest read,
  mutual require) PLUS the new entry rule.

Name-blind on purpose (no name-resemblance, no file name), so a bundle
that vendors a library sharing its name with one of its dependencies still
matches that library.

### Round 2: the fence asks the entry rule only

The first cut let discovery ask exp172's front rules too. That is unsound
at discovery scope: an unclaimed module is no proof of app code there.
workflow_nest's node build regressed — `@swc/core` reads its OWN
`package.json` inside the bundle, an unclaimed helper relayed the entry's
require to it, the front named `@swc/core` the bundle's own, and the fence
unclaimed 11,238 bytes of real `@swc/core` code (bun-dev-node
workflow_nest, both indexes). The front rules answer the app-module
decision's question (where exp172's census validated them) and only that
asker uses them. Test:
`a_library_reading_its_own_manifest_inside_the_bundle_stays_claimed`
(discovery level, red first),
`the_fence_asks_only_the_entry_rule` (decision level).

## Results (exp096 scorer 14, per-set bytes)

Judged with the exp096 scorer over the F1 sets on BOTH indexes —
`cb64c1f5` (the current index) and `8462c502` (exp177's larger one) —
main `3218b7b8d` against the arm, scored at `b8dcacc71` (the arm's code
at that commit is the arm's code at HEAD; only docs changed after).
Bytes; before → after.

| set      | index    | named (right)             | wrong family      | app claimed | `<self>` claimed    |
| -------- | -------- | ------------------------- | ----------------- | ----------- | ------------------- |
| bun-dev  | cb64     | 179,019,113 → 179,019,113 | 384,084 → 384,084 | 0 → 0       | 289,675 → 289,675   |
| bun-dev  | 8462c502 | 179,690,308 → 179,690,308 | 383,532 → 383,532 | 0 → 0       | 4,151,047 → 289,675 |
| bun-held | cb64     | 45,336,703 → 45,336,703   | 72,095 → 72,095   | 0 → 0       | 0 → 0               |
| bun-held | 8462c502 | 45,518,219 → 45,518,219   | 70,718 → 70,718   | 0 → 0       | 0 → 0               |
| cc       | cb64     | 1,819,438 → 1,819,438     | 0 → 0             | 0 → 0       | 0 → 0               |
| cc       | 8462c502 | 1,819,438 → 1,819,438     | 0 → 0             | 0 → 0       | 0 → 0               |

- The target lands: `cli_just-bash`'s self-claimed bytes — 3,861,372
  (bun 1,930,605 + node 1,930,767) — drop to 0 on the larger index; that
  code becomes app code. Right bytes, wrong-family bytes and app-claimed
  bytes are UNCHANGED on every set and index (the summary columns are
  byte-identical main vs arm on all six combos), so precision holds.
- The remaining 289,675 `<self>` bytes on bun-dev are `ts-patch`
  (76,840) + `ts-json-schema-generator` (212,835) — bundles that are also
  published packages but whose entry modules the entry rule does not
  name. They read the same on main and here: pre-existing verdicts,
  reported apart by the scorer, not this change's target.
- `bun-held` and `cc` are byte-identical main vs arm on both indexes: the
  fence fires on neither (CC's own package is not claimed today — no
  self/selfOther bytes, identical claim counts), so CC's naming is
  untouched by construction.
- The larger index's just-bash self-claims carried 2,134 unmapped-unit
  bytes; those leave the judged counters with the fence and are not a
  judged category.

### exp172's CJS-app placement still behaves

Stub-LLM runs over the 90-bundle bun-dev list
(`experiments/166-restore-or-ask/stub-run.mts`) on both arms, scored by
`experiments/172-cjs-app-placement/score.py` against
`/work/exp172-data/bundev-census.jsonl`: the two arms' output sections are
BYTE-IDENTICAL (the documented 10 RUN FAILs on both arms; same
own-package cards, same per-bundle app-moved/lib-moved numbers). The
app-module decision now reaches `own_packages` (front rules + entry rule)
and behaves exactly as before.

### Warm neutrality on Claude Code

Launched as the exp175 README did, on 2.1.85→2.1.86, arm commit
`eac1f90fa` vs main `3218b7b8d`:

- names mode: NEUTRAL — differing files 0, differing lines 0, baseline
  leg cache +0 (the load-bearing one), both legs exit 0.
- off mode (`library.matching=off`): NEUTRAL — same zeros.

Both are the expected result given CC's own package is not claimed today.

## What could NOT be proved

- The entry rule is name-blind and evidence-bound by design, so it cannot
  name every bundle that happens to BE a package: the remaining
  289,675 `<self>` bytes (`ts-patch`, `ts-json-schema-generator`) are
  proof it is incomplete. This change claims only the shape Andrew asked
  about (an entry whose majority claimant is the bundle's own package);
  other self-claim shapes stay as they were.
- A bundle whose entry modules have no majority claimant, or whose
  entries disagree, gets NO own package and is untouched. Whether any
  real bundle is harmed by that refusal is not measured here (no such
  bundle appeared in the F1 sets).
- Neutrality proves the 2.1.85→2.1.86 pair only. The fence changes
  discovery claims for any bundle the entry rule fires on; bundles
  outside the F1 sets could see placement change that nothing here
  measures.
- The scorer has no noise band for a card verdict; the numbers above are
  exact arithmetic over the recorded cards, not re-run draws, so they
  carry no ± band — but they are single observations, not repeats.
- The iv index is judged but NOT adopted: `pairs.json` still points at
  `cb64c1f5`, and the per-kind precision problems exp181 recorded for
  the other bundlers are unchanged by this.

## Gate

`npm run check` — 13/13, run ALONE, in the exp183 worktree:

| stage              | result                                       |
| ------------------ | -------------------------------------------- |
| typecheck          | PASS 2.8s                                    |
| lint               | PASS 1.6s                                    |
| rust:fmt           | PASS 1.4s                                    |
| rust:clippy        | PASS 5.9s                                    |
| knip               | PASS 1.7s                                    |
| knip:prod          | PASS 1.3s                                    |
| census:clones      | PASS 1.0s                                    |
| census:concurrency | PASS 0.9s                                    |
| unit               | PASS 107.8s                                  |
| rust:unit          | PASS 273.1s                                  |
| rust:build         | PASS 124.8s                                  |
| rust:format-golden | PASS 0.0s                                    |
| e2e                | PASS 77.0s (19 pairs, 0 unexpected failures) |

And 13/13 again from a CLEAN checkout (detached worktree at the final
commit — the e2e fixtures under `test/e2e/fixtures/*` are gitignored, so
the committed tree is what the gate proves).
