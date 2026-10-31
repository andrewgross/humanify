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

TBD
