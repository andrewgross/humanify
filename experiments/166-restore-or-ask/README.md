# exp166 — restore or ask, never freeze (and the exact raw-to-era bridge)

STATUS: IN PROGRESS (2026-10-31). Branch `exp166-restore-or-ask` off main
`c7cafbc74`. Numbers below marked TBD are filled as the runs land.

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

TBD

## The fix (phase 3)

TBD

## The hop (phase 4)

TBD
