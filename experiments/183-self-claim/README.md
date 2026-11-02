# exp183 — a bundle never claims its own package

STATUS: IN PROGRESS on branch `exp183-self-claim` (off main `3218b7b8d`).
(The results table and gate table below are filled at the end of the run;
numbers in flight are marked.)

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

## What could NOT be proved

(filled at the end)

## Gate

(filled at the end)
