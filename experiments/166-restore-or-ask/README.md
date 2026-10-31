# exp166 — restore or ask, never freeze (and the exact raw-to-era bridge)

STATUS: LANDED on the branch (2026-10-31). Branch `exp166-restore-or-ask` off
main `c7cafbc74`; scored at `210ed0558`. **Every hop bar met.** On the
2.1.215→2.1.216 hop in names mode, mints fall to 54 (exp164 501, default
81). App bytes under `vendor/` are 0. novel and realLn are byte-equal.
Restoration rises to 17,980 restored names at 9,016 applications. The vendor
buckets beat exp164 on every row. The cold self-hop reads 8 lines (names
mode's 1,040–1,176 is gone; the reference is 2–184). Neutrality is NEUTRAL in
both modes, and the gate passes 13/13.

## The trace (phase 1)

On the 2.1.215→2.1.216 hop, names mode filed the second copy of Claude
Code's plugin installer under `vendor/jose/module-af5c7d31.js` with its
minifier names (Fzp / $zp / Bzp); default mode files the same code under
`src/use-plugin-manager/plugin-install-manager.js` with real names. exp165
left two questions open: who moves it, and where the folder name "jose"
comes from. The answer to both is one bug:

- **The code path.** `unpack::bun::claimed_regions` converted each claimed
  lazy placement group to naming-era ordinals as `i - removed_before`. That
  accounts for the factory splice but NOT for the formatter, which splits
  statements (`var a, b, c;` → three; `a, b;` → two). Every split shifts
  every later ordinal. exp164 had measured the symptom (118 of 7,628
  converted statements content-correct) but routed around it.
  `naming::module_names::plan_library_modules` then joins a region to the
  era fossil module whose statement set equals the region's (drifted)
  ordinals. When a drifted all-`jose` region exactly matched the
  installer module's statements, the planner's verdict was **Move**
  (`claim_verdict` = `Ok("jose")`; era regions carry no module key, so the
  path is the synthetic `module-<content key8>`).
- **"jose"** is the claim of real jose groups (4664–4767 on 2.1.215),
  carried onto the wrong module by the drifted ordinals. It is not the
  index, not the model, not the prior. The `af5c7d31` stem is the
  installer module's own content key. Evidence on disk (2.1.216's
  eraRestoration dump): regions whose raw text is jose's JWE decrypt
  (`async function OIr(e,t,r){…"Flattened JWE must be an object"`) carry
  era statements 30194–30197, which are Fzp/$zp/Bzp. The same drift lands
  jose regions on the other three `vendor/jose/` modules. A fresh 2.1.215
  run of main's logic (`/work/exp166-data/cc215-main`) writes exactly the
  same four `vendor/jose/module-*.js` stems — the 2.1.215 join is
  confirmed, not inferred.
- **The gate that kept the model off.** The library-layout freeze:
  `layout.spans` (every moved statement) → `freeze.library_spans`, applied
  before the transfer and the waves. On 2.1.216 the raw names then ride
  exact-match from the prior (frf→Fzp, …), so the trap persists through
  the carry. The exp157 unclaimed-module rule is NOT involved.

## The missing measurement (phase 2)

`vendor_precision.py` (`npm run eval -- vendor-precision check|bun-dev`):
what a names-mode run put under `vendor/`, judged by source-map truth,
whether or not it was claimed:

- (a) app bytes under `vendor/` — the bar is 0;
- (b) library bytes under the WRONG package's folder;
- (c) the conversion's damage: region statements whose era statement holds
  a different raw statement, and moved (frozen) statements whose raw
  statement no claim of the moving package covers.

It reads one run's `--diagnostics` file. The eraRestoration block now
carries the exact raw-to-era bridge (each era statement's raw span), each
moved module's path and statements (moved modules get no placement trail —
the split pre-fills their paths), the vendored factories, and,
diagnostics only, the plan the RETIRED ordinal conversion would have made
from the same claimed groups over the same era body. So one run judges two
worlds: **ordinal** (main's logic; a unit test proves the simulation
reproduces the app-module move) and **exact** (this branch).

### Correction to exp165 (rule 9)

exp165's census classified `kind = "app" if label == "app"`, but bun-dev's
truth spells app bytes `<app>`. App bytes fell into "other" (reported as
"unmapped"), so **exp165's "not one app-labelled byte on bun-dev" was never
measured**. This check counts `<app>` explicitly. (The re-measurement below
finds 0 app bytes in moved era modules on bun-dev in either world, so
exp165's conclusion about the claim boundary happens to stand — but on this
measurement, not its own.)

### bun-dev (full truth)

The corpus list holds 90 bundles. **71 are judged.** Excluded, by name:

- 10 fail in the pipeline on BOTH main's logic and this branch
  (pre-existing; same errors on both binaries):
  `bun-dev-bun-cli_just-bash-6676e9`, `bun-dev-bun-vscode_gh-deepmyst__mysti-4faa9b`,
  `bun-dev-bun-vscode_gh-rusiaaman__chat.md-331c54` (stable split: wrapper
  block absent from the estree json);
  `bun-dev-bun-electron_gh-kryptokrona__hugin-desktop-474f08`,
  `bun-dev-node-electron_gh-kryptokrona__hugin-desktop-474f08`,
  `bun-dev-bun-electron_gh-toantranmei__electron-vite-starter-16153e`,
  `bun-dev-node-electron_gh-toantranmei__electron-vite-starter-16153e`,
  `bun-dev-bun-vscode_gh-altimateai__vscode-dbt-power-user-a91260`,
  `bun-dev-node-vscode_gh-altimateai__vscode-dbt-power-user-a91260`,
  `bun-dev-node-vscode_gh-yessglory17__argus-c36d36` (parse error:
  "Expected function name").
- 9 write no bridge dump (no vendor-record unpack, so no claimed regions
  and no era placement to judge): `bun-dev-bun-cli_babel-istanbul-f3b1f1`,
  `bun-dev-bun-cli_ember-cli-afb389`, `bun-dev-bun-cli_health-88539c`,
  `bun-dev-bun-cli_jsforce-566556`, `bun-dev-bun-cli_lingui_cli-33be9c`,
  `bun-dev-bun-electron_gh-cclank__quickshot-6f4904`,
  `bun-dev-bun-electron_gh-sanlinhtik3__thalika-voice-clone-9c854e`,
  `bun-dev-bun-electron_gh-simon-fraser__musictube-player-904ca4`,
  `bun-dev-bun-vscode_gh-cschlosser__doxdocgen-435b5b`.

| era placement (moved lazy modules)          | ordinal (main) | exact (exp166) |
| ------------------------------------------- | -------------- | -------------- |
| region statements content-correct           | **0 / 2,515**  | 2,512 / 2,512  |
| moved modules / statements                  | 17 / 39        | 364 / 1,455    |
| moved bytes                                 | 11,538         | 470,361        |
| app bytes under vendor/                     | 0              | 0              |
| library bytes in the wrong package's folder | 9,160 (80%)    | 38,263 (8%)    |
| moved statements not claimed by the mover   | 27 of 39       | 0              |

The exact bridge moves ~30× more code into `vendor/` (364 modules / 470 KB
against 17 / 11.5 KB): the claims that were always there finally land on
their own statements. The hop below must show that the vendor buckets and
mints do not get worse from it. The exact world's 8% wrong-package bytes
are claim precision (the claims themselves name the wrong package), not
conversion — every moved statement is claimed by its mover.

Vendored factories (the unpack; identical in both worlds and largely in
every mode): 378 app bytes out of 164 MB. 221 of them are root `lib_*`
fallback files default mode writes too; 157 are index-placed in one
bundle (`…catalyst-05cf16`: 101 bytes a boundary sliver inside a
`websocket` factory, 56 bytes a `@discoveryjs/json-ext/src/version.js`).
The numbers do not justify tightening the exp157 unclaimed-move rule; it
was left alone.

### cc (sample truth)

Fresh stub runs of the fixed binary on 2.1.215 and 2.1.216 (cc's truth is
the 620-unit labelled sample on 2.1.216; 2.1.215 is judged on (c) only).

| era placement (moved lazy modules)        | 2.1.216 ordinal | 2.1.216 exact | 2.1.215 ordinal | 2.1.215 exact |
| ----------------------------------------- | --------------- | ------------- | --------------- | ------------- |
| region statements content-correct         | 0 / 7,628       | 7,628 / 7,628 | 0 / 7,571       | 7,571 / 7,571 |
| moved modules / statements                | 17 / 52         | 439 / 1,469   | 25 / 72         | 436 / 1,454   |
| moved bytes                               | 16,324          | 543,802       | 45,395          | 541,435       |
| moved statements not claimed by the mover | 40 (15,890 B)   | 0             | 57 (44,373 B)   | 0             |
| labelled app bytes under vendor/          | 0 of 0 labelled | 0 of 46,354   | —               | —             |

The conversion: 1,994 / 1,980 groups, 0 dropped. Copy B of the plugin
installer now lands in `src/` on both versions (named), and `vendor/jose/`
holds 63 real jose modules (the flattened-JWE code) — main's logic put the
installer there and jose's own code in `src/`.

### App code under vendor/, itemized (the fixed binary)

`itemize_vendor_app.py`: every app-labelled byte under `vendor/`, with the
mechanism that put it there, plus (cc) every UNLABELLED vendor file whose
text looks like app code.

- **bun-dev: 4 cases, 378 bytes, one bundle**
  (`bun-dev-node-vscode_gh-rudreshveerkhare__catalyst-05cf16`) — and **none
  is app code**: all four are the `websocket` package's own modules, which
  the source map attributes to the app (a truth defect):
  - `vendor/websocket/module-3c62f4b0.js` — 101 of 1,054 bytes: websocket's
    `package.json` object (the tail of it mapped to the app). Mechanism:
    index-claimed factory.
  - `vendor/@discoveryjs/json-ext/src/version.js` — 56 bytes:
    `module.exports = <package.json>().version`, websocket's `lib/version.js`.
    Mechanism: unclaimed-but-clean move (exp157) — and the WRONG package:
    the one-liner is generic, and the index's whole-module answer for it is
    json-ext's identical `src/version.js`.
  - `vendor/lib_3c078d91.js` — 173 bytes: websocket's `lib/websocket.js`
    (the `{server, client, router, …, version}` export object). Mechanism:
    root `lib_*` fallback (default mode writes it too).
  - `vendor/lib_82619232-14.js` — 48 bytes: websocket's `index.js`
    (`module.exports = require(lib/websocket)`). Mechanism: root fallback.
- **bun-dev era placement (moved lazy modules): 0 app bytes** in either world.
- **cc 2.1.216 and 2.1.215: 0 labelled app bytes under `vendor/`**. Suspect
  scan (token counts only, nothing quoted): no vendor file outside
  `@anthropic-ai/*` and `@modelcontextprotocol/*` contains an app-specific
  token (`claude`, `CLAUDE_`, `marketplace`, `mcp`, `statusline`, `subagent`,
  `permission mode`). The generic-token hits (`plugin`, `hook`) are library
  code: commander's `command.js` (hooks), the AWS/smithy middleware
  "plugin" stacks, msal-node's client plugins, node-forge's PRNG plugins,
  ajv-formats, and four root `lib_*` files — checked by hand: three are AWS
  SDK client constructors (`middlewareStack.use(get…Plugin(config))`), one
  a DOM shim (`document.plugins`). All library, root fallback (the same in
  default mode).
- Mechanism census of what lands under `vendor/`: claims (the exact era
  moves, the index-claimed factories), the exp157 unclaimed-clean factory
  moves, the root fallback, and — on a hop — placement inheritance. The
  jose class (a lazy fossil module filed on drifted claims) is GONE: it
  needed the ordinal conversion.

## The fix (phase 3)

Two parts, both names-gated (the bridge exists only when the discovery
carries the index's module answers), so default and report mode are inert.

1. **The exact raw-to-era bridge (primary).** The printer records, for every
   statement it prints, the input offset of the first source-spanned node
   inside it (`format_file_with_origins`; the text is byte-identical — the
   golden suite unchanged). The unpack maps every container statement's raw
   span to its span in the runtime text through the exact edit list
   (`unpack::bun::EditMap`). Together they say which raw statement every era
   statement came from — a split statement included. `convert_regions` maps
   each claimed group onto exactly those era statements (a group with an
   unmapped raw statement is dropped whole; on cc: 0 dropped of 1,994). The
   ordinal arithmetic is deleted from the unpack.
2. **Restore or ask, never freeze.** The freeze holds only the bindings the
   index restored (`restored_binding_spans`: a binding whose name Site B
   restored inside the raw statement its era statement came from), held by
   the one hold Site A's applications share (`RestoredHolds`: carried,
   library-held, joined to the transferred set of its innermost function,
   a module binding's row skipped). Every other identifier — moved module
   or not, its wrapper included (the module step no longer skips moved
   modules) — takes the normal flow: the prior's carry, then the model.

Tests (red first, each watched fail before the fix):
`a_moved_modules_unrestored_identifiers_are_asked_not_frozen`,
`a_moved_modules_unrestored_identifiers_carry_from_the_prior` (red on the
old freeze), `a_restored_slot_is_never_asked` (red with the hold disabled:
the restored slot was asked),
`a_library_region_after_a_split_statement_moves_its_own_module_not_the_app_one`
(the jose shape; red on the ordinal path: the app module stayed minified),
plus the formatter provenance tests (`format/origins_test.rs`) and the
unpack bridge test. The placement root cause on the unpack side (the exp157
unclaimed-clean move) was measured and LEFT ALONE: its only app-labelled
bytes on bun-dev are the 56-byte truth defect above.

## The hop (phase 4)

`npm run eval -- score <label> --pairs 215->216` (names mode, the full
names index `cb64c1f5…`, scratch base, cold, one pair, under
`/work/heavy.lock`). Two labels: `exp166-names-hop` (commit `421a5e817`,
the bridge + restore-or-ask) and `exp166-names-hop2` (`210ed0558`, plus "a
minifier-shaped restoration is not a hold"). The before columns are exp161's
default leg and exp164's names leg (the same config).

| KPI                    | default (exp161) | names exp164 | exp166 hop 1 | **exp166 hop 2** | band  |
| ---------------------- | ---------------- | ------------ | ------------ | ---------------- | ----- |
| clean                  | 34,636           | 34,559       | 34,627       | 34,625           | ±17   |
| noise                  | 281              | 358          | 290          | 292              | ±17   |
| novel                  | 986              | 986          | 986          | **986 (=)**      | 0     |
| noiseLn                | 3,304            | 5,990        | 4,900        | 4,919            | ±1376 |
| realLn                 | 122,066          | 122,066      | 122,066      | **122,066 (=)**  | 0     |
| reloc                  | 337              | 338          | 341          | 343              | ±35   |
| newName                | 753              | 786          | 754          | 761              | ±13   |
| mints                  | 81               | 501          | 1,482        | **54**           | ±33   |
| tree churn             | 25,345           | —            | 25,522       | 25,537           | —     |
| nameOnly lines         | 3,946            | 4,166        | 4,164        | 4,192            | —     |
| cold self-hop (ln)     | 18               | 1,040        | 10           | **8**            | 2–184 |
| restoration apps       | —                | 9,016        | 9,016        | **9,016 (=)**    | —     |
| restored names (proxy) | —                | 12,980       | —            | **17,980**       | —     |

- **Hop 1's mints (1,482) are explained and fixed.** About 1,400 of them were
  index-restored names that are themselves minifier tokens (`i`, `k`, `v`,
  `x`, `_a`: the package's own source). The new freeze held them in claimed
  statements, where exp164 had let the model re-ask them. A restored name
  that passes the mint census's own gate (`rename::floor::is_minifier_token`)
  tells nothing the minified name did not, and a short name can coincide with
  an unrelated minifier name in the same statement. So it now takes the normal
  flow (`a_minifier_shaped_restoration_is_not_a_hold`, red then green). Hop 2's
  54 remaining are model-chosen short names (`x`, `y`, `i`, `op`).
- **Restored names (exp161's proxy, the same covered list):** exp164 12,980
  (44.0% of 29,516), hop 2 17,980 (54.3% of 33,130). Cross-version
  stability on that scope: 100% (33,130 / 33,130). exp164's was 99.96%.
- **Determinism:** the warm self-hop is byte-identical with 0 cache writes on
  both labels. Both pairs exit 0 with 0 provider errors, and the boot gate is OK.

**The line-diff buckets** (exp161's `bucket-trees`; files / lines):

| vendor       | default | names exp164 | exp166 hop 2 |
| ------------ | ------- | ------------ | ------------ |
| identical    | 1,595   | 1,612        | **2,031**    |
| nameOnly     | 0 / 0   | 3 / 72       | **0 / 0**    |
| realChange   | 0 / 0   | 5 / 34       | **0 / 0**    |
| trulyAdded   | 23 / 97 | 33 / 439     | **26 / 263** |
| trulyRemoved | 0 / 0   | 0 / 0        | 0 / 0        |

| src          | default      | names exp164 | exp166 hop 2 |
| ------------ | ------------ | ------------ | ------------ |
| identical    | 4,276        | 4,205        | 3,828        |
| nameOnly     | 22 / 108     | 52 / 276     | 47 / 252     |
| realChange   | 552 / 19,909 | 558 / 20,089 | 537 / 19,918 |
| trulyAdded   | 30 / 4,546   | 30 / 4,546   | 29 / 4,463   |
| trulyRemoved | 0 / 0        | 10 / 342     | 2 / 140      |

(src's `identical` count falls because ~420 library modules left src/ for
vendor/, where they are identical.) The exact bridge moves ~30× more code into
`vendor/`: on hop 2, 440 era modules / 1.43 MB placed by claim (exp164: 18 /
22 KB). The buckets show it does not make vendor worse: every exp164 vendor
noise row goes to 0.

**exp168's distribution (`vendor_distribution.ts`, read-only;
`tables/vendor-distribution-compare.md`):**

- era (app) statements filed into `vendor/` by placement: exp164 105,255 B
  in 18 files, **hop 2: 0**;
- vendorReal 506 → **289** (default 120); vendorLn 255 → **183** (= default);
  name re-rolls 72 → 0; mint-bearing vendor lines 122 → 1;
- identical era vendor code keeps its names: 8,811 same / 0 different
  (exp164 728 / 12).

**App code crossing the src/vendor boundary** (statements with a prior
counterpart, both directions, `cross_boundary.py`):

- exp164: 11 src→vendor statements / 7,775 B, in exp168's 10 new
  `vendor/<pkg>/module-*.js` files. **The exact bridge removes all 10.**
- hop 2: 7 src→vendor statements / 2,421 B, in two `@anthropic-ai/*` modules
  (sandbox-runtime, claude-agent-sdk). The claims are genuine (the bundle
  ships those npm packages); they appeared on 2.1.216 and not on 2.1.215. That
  is claim instability across versions (matching's territory), not this lane.
- vendor→src: 0 in every run.

The 289 vendorReal lines are 23 ajv factory files (120 lines, the same as
default) plus three new `@anthropic-ai/*` module files (166 lines; two of
them are the cross-boundary moves above).

### The library scorer

The scorer reads `humanify discover` reports. Regenerated with the final
binary: **bun-dev 89 / 89 byte-identical** to exp165's reports (main's
discovery), and **cc 2.1.216 identical** except the `bundlerOverride`
field (this run passed `--bundler bun`). Claims, bundle order and every
libraryDiscovery field are equal, so the scorer cards are unchanged (app
claimed 0). Nothing in this lane touches discovery.

### Neutrality and gate

- Warm neutrality vs main `c7cafbc74` (pair 2.1.85→2.1.86, `--cache
/work/neutrality-cache`, `--priors /work/exp050-cold`, under
  `/work/neutrality.lock` + `/work/heavy.lock`):
  **default mode: NEUTRAL** (0 files / 0 lines, cache +0 / +0, exit 0 / 0);
  **report mode** (`library.matching=report`, the standing index):
  **NEUTRAL** (0 / 0, +0 / +0, 0 / 0).
- `npm run check`: **ALL 13 STAGES PASSED** at `210ed0558`.

## The per-identifier walkthrough

`factory_bindings.mjs` + `walkthrough.py` write one row per binding in
`src/` and `vendor/` of the hop's 2.1.216 tree to
`/work/exp166-data/hop-walkthrough/bindings.jsonl` (CC-derived; never
committed). Each row records the file, the package, the minified name, the
final name, the source of the name, and whether it equals the 2.1.215 name.
Bucket totals: `tables/hop-walkthrough-buckets.json`. Of 298,272 bindings:

- vendor/ (66,871): 35,917 index-restored, 2,492 carried by exact match,
  4 model-asked, 4,854 unpack plumbing, 21,428 left minified (vendored
  factory bodies the index did not answer: never asked in any mode), 2,176
  without a trail. 66,559 equal the 2.1.215 name, 0 differ (312 have no
  same-path prior).
- src/ (231,401): 155,695 carried by exact match, 8,631 by close match /
  reconcile, 4,273 model-asked, 10,989 index-restored, 3,356 by rule, 665
  left minified, 47,792 without a trail (overwhelmingly the split's
  cross-file import bindings, named after what they import). 216,881 equal
  the 2.1.215 name, 956 differ.

## What this does not look at (rule 8)

- cc's truth is a 620-unit sample, and most of the bundle is unlabelled. The
  cc app check is "0 of the labelled", plus a token-count suspect scan.
- The bun-dev runs use the stub LLM (placement is LLM-independent; naming is
  not judged there).
- `cross_boundary.py` counts only statements with a prior counterpart.
- The walkthrough's "index-restored" for era rows is a name match inside the
  statement's raw span (the pipeline's own hold rule), not a binding-level
  trail.
