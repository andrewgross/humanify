# exp169 — vendored library code is named like app code (names mode)

STATUS: LANDED on the branch (2026-10-31). Branch `exp169-vendor-naming`,
rebased on main `32caeb572` (exp166, exp168, exp170 merged); scored at
`a706a0ae7`. **Every hop bar met.** In names mode on the 2.1.215→2.1.216
hop, minified vendor bindings fall from 21,428 to 917 (436 of them in the
11 bodies the pass leaves as extracted, 481 short real words such as `key`,
`url`, `err`). Vendor identifiers with a prior counterpart: 66,559 equal, 0
different, the same as exp166. Vendor name re-roll lines: 0. Mints 54 → 52
(vendor 2 → 0, app 52 = 52). novel and realLn are byte-equal. vendorReal
rises by 904 lines, and all 904 come from formatting: the same 26 newly
added vendor files now print on several lines each instead of one. Warm
self-hop byte-identical, cold self-hop 8 lines, app bytes in vendor/ 0.
Neutrality is NEUTRAL in default and report mode, and the gate passes
13/13. Phase 0 finds that CommonJS-authored apps put ALL their own code in
`vendor/`, in every mode. That corrects exp166's "0 app bytes".

Andrew's decision: in names mode, the identifiers in vendored library code
that the index did NOT restore get named exactly like app code — carried
from the prior release by exact or close match first, then asked of the
model. Until now the bodies of the extracted `__commonJS` factories were
copied out as written and never entered identifier naming in any mode. On
the 2.1.215→2.1.216 hop (exp166's walkthrough) that left 21,428 vendor
bindings minified, 32% of the vendor bindings.

## Phase 0: does app code live inside `__commonJS` wrappers?

### The wrapper kinds and what the unpacker does with each

Bun writes two module wrappers, each a small runtime helper read by shape
(`wrapper_kinds.py`; a text count, so approximate):

| wrapper                                                                      | what it holds                                              | where it goes                                                                                                            |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `__commonJS` — `X=(A,B)=>()=>(B\|\|A((B={exports:{}}).exports,B),B.exports)` | a module authored as CommonJS (`require`/`module.exports`) | **extracted by the unpack** into its own file — under `vendor/` (or `src/_assets/` for an app text asset), in every mode |
| `__esm` lazy init — `X=(A,B)=>()=>(A&&(B=A(A=0)),B)`                         | an ES module that must initialize lazily                   | stays in the app text (the era); only names mode's claims move a claimed one to `vendor/<pkg>/` (exp166's exact bridge)  |
| (no wrapper)                                                                 | an ES module Bun could hoist                               | the app text                                                                                                             |

- **Claude Code 2.1.216:** 1,647 `__commonJS` calls — exactly the 1,647
  factories the unpack extracts — and 4,850 `__esm` inits.
- **bun-dev (80 bundles):** 39,558 `__commonJS` calls, 2,803 `__esm`
  inits (47 bundles have none). The text count and the unpack's
  extraction agree on most bundles; where they differ, the unpack kept
  some factories in the app (the 9 bundles without a vendor record, and a
  few factories inside pre-bundled chunks).

### bun-dev (full source-map truth)

`self_in_factories.py` itemizes every `<self>` and `<app>` truth byte inside
a vendored factory's raw span (exp166's names-mode stub runs, 71 judged
bundles), per bundle, vendor file and ORIGINAL source file (the truth's
`sources` rows carry the file inside the package). The totals reproduce
exp166's exactly: 388,895 `<self>` bytes and 378 `<app>` bytes, out of
164 MB.

`<self>` is the app's own published package. In bun-dev the "app" is a
published npm CLI that the corpus builder bundled with a 70-byte entry
(`<app>`), so `<self>` IS the app's code. Itemized by app (each app
appears twice, bun and node targets; bytes summed over both):

| app (package)            | `<self>` bytes in factories | source files | what it is                                                                                                                                | the app's own?                                  |
| ------------------------ | --------------------------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| ts-json-schema-generator | 238,701                     | 152          | its `dist/src/**` (tsc output of its own source: SchemaGenerator, NodeParser/_, TypeFormatter/_)                                          | **yes**                                         |
| ts-patch                 | 97,502                      | 44           | its `patch/*`, `actions/*`, `module/*`, `cli/*` (tsc output, `__importDefault` helpers)                                                   | **yes**                                         |
| doctoc                   | 26,826                      | 5            | `doctoc.js`, `lib/transform.js`, `lib/file.js`, …                                                                                         | **yes**                                         |
| cz-customizable          | 13,630                      | 7            | `lib/questions.js`, `lib/build-commit.js`, `index.js`, …                                                                                  | **yes**                                         |
| @umijs/fabric            | 7,149                       | 2            | `dist/verifyCommit.js`, `cli.js`                                                                                                          | **yes**                                         |
| babel-istanbul           | 4,537                       | 8            | `lib/cli.js`, `lib/util/*`, `lib/store`, … (the package is a fork of istanbul; this is the fork's own source)                             | **yes**                                         |
| cross-var                | 112                         | 2            | its entry `index.js` / `src/index.js` (a babel-register bootstrap), 56 B per target                                                       | **yes**                                         |
| just-bash                | 240                         | 2            | 2 slivers of its own pre-built `dist/bin/chunks/*.js` mapped onto `sql.js` / `seek-bzip` factories (a `var __dirname=` line Bun inserted) | no — third-party code the app ships pre-bundled |
| @workflow/nest           | 198                         | 1            | `dist/swcrc.js` mapped onto a 74-byte native-addon path stub and a sliver of `@swc/core/binding.js`                                       | no — a mapping sliver                           |
| (catalyst, `<app>`)      | 378                         | —            | exp166's truth defect: the `websocket` package's own modules mapped to the app                                                            | no                                              |

For the six CommonJS-authored apps, **100% of the app's own code is inside
`__commonJS` factories**: the bundle's whole `<self>` total equals the bytes
in factories (ts-json-schema-generator 119,343 of 119,343 per target,
ts-patch 48,745 of 48,745, doctoc, cz-customizable, @umijs/fabric and
babel-istanbul likewise); `src/` holds only the entry. Where it lands:
313,285 B under `vendor/<the app's own package>/…` (the index knows the
app's npm package, so names mode files it there by index path),
71,659 B at the vendor root (`vendor/lib_*.js`), 4,329 B under another
package's folder (`vendor/glob/…`, `vendor/json5/…`: index paths the
claims got wrong). A default-mode stub run of ts-patch puts the same 125
factories under `vendor/` (as `lib_*` files) and 1.7 KB in `src/`: this is
the unpack, in every mode.

### Claude Code 2.1.216

The labelled sample cannot answer this: its brief says "CommonJS factories
are NOT in the population" (`cc_factory_labels.py`: 0 labelled units of any
kind inside a factory). So `cc_factory_scan.py` scans every factory by
token counts for Claude Code's vocabulary (counts and paths only; output
under /work, never committed):

- 1,647 factories, 5.05 MB. **29 are app text assets** (554 KB: Claude
  Code's own prompt and skill texts, a factory whose body exports a
  string) — the unpack files them under `src/_assets/`, never `vendor/`.
- 1,618 under `vendor/` (4.50 MB, 328 package folders). The only hits are
  two `@grpc/grpc-js` modules whose `var __dirname="…"` line (Bun inlines
  the build machine's path) contains the build directory's name — library
  code. Claude Code's own code is ES modules: hoisted or `__esm`, never
  `__commonJS`.

### Verdict

App code ends up inside a `__commonJS` wrapper exactly when the app's own
source was written as CommonJS, and then ALL of it does. The unpacker
extracts every `__commonJS` factory — app or library — into its own file,
under `vendor/` unless it is a text asset, in every mode. On bun-dev that
is 388,477 genuine app bytes (six CommonJS apps entirely, plus cross-var's
entry) — 0.24% of the 164 MB factory population, but the whole app for
those six. On Claude Code, which is ES-module source, the only app code in
`__commonJS` wrappers is the 29 text assets, and they go to `src/_assets/`,
not `vendor/`.

**Correction (rule 9) to exp166's "app bytes under `vendor/` = 0":** that
bar counted the `<app>` label only. Counted as `<self>` — the label
bun-dev gives the app's own code — the app bytes under `vendor/` on bun-dev
are 388,477, in every mode (the unpack's placement, not names mode's
claims). exp166's era-placement number (0 app bytes moved by claims) is
unaffected. This lane names those bytes (they are factory bodies); it does
not move them. Moving a CommonJS app's own modules out of `vendor/` is a
placement question for a later lane: the unpack would need to tell the
app's package from a dependency (the index's answer for the app's own
package is the signal that put them at `vendor/<app>/`).

## Phase 1: the build

Where the bodies bypassed naming: `unpack::bun` slices each factory body
(`vendored_body`) and writes it under `vendor/`; Site B's restoration
(`naming::restore::apply_body_restoration`) renames the slots the index
answered, on the bundle, and nothing else ever touches the body. The era
(`naming::driver::run_naming` over the runtime text) never sees it.

**One path, not a parallel copy.** The bodies do not join the era (that
would change what the split and the runtime see). They get their own pass
that IS the app's naming: `naming::vendor_bodies::name_vendor_bodies` joins
the bodies under `vendor/` into one text — each body formatted and then
one top-level statement `(<body>);`, in vendor-path order (paths are
stable across releases, so both sides' orders agree) — and hands it to
`run_naming`, the same driver, graph, matching, transfer, waves, floor,
sweeps and reconcile the app gets. The prior is the prior release's joined
named text, `.humanify/vendor-humanified.js` (written beside
`humanified.js`, the era's prior), so cross-version stability comes from
the existing exact/close prior carry. The named text is split back into
one body per file and written over the unpack's file before the split and
the finish run.

- **Holds.** Site B's restored names stay held by the era's ONE hold rule:
  `restored_binding_spans` is now a thin case of `held_binding_spans`
  (a binding declared in statement _i_ whose name the index restored
  there, unless the name is minifier-shaped). The vendor text gives one
  name set per body statement (`NamingInput::statement_holds`, read from
  the bridge's restored edits inside each factory's declaration span).
- **Stable paths, manifest, runtime.** A body's free identifiers — the
  module identifiers (`lib_<hash>.f()`), `require`, the captures the
  finish's relink bridges — are never renamed, so `plan_modules`'
  identifiers, the file paths and `_bun-modules.json` cannot move.
- **Fail-soft.** A body the formatter's strict parse refuses ships as it
  was; an invalid named text or one that does not split back ships every
  body as it was; a prior the driver refuses as another program is
  dropped and the bodies are named fresh.
- **Gate.** The pass runs only when the raw-to-era bridge exists, which is
  names mode with the index's module answers. Default and report mode
  never reach it.
- **Formatting.** The named bodies are formatted (as app code is): the
  prompts show the code around an identifier by line, and a minified body
  is one line. Names-mode vendor bodies therefore change shape too.
- **Two bodies the pass leaves as extracted** (found by the first hop,
  `exp169-names-hop`, which the harness refused to score): a body holding a
  lone UTF-16 surrogate escape (`"\uD800"`) — oxc's ESTree JSON cannot
  carry one, the match stage reads it, and ONE such body made the driver
  refuse the prior for every body ("estree json program body missing"), so
  the first hop named all 1,618 bodies fresh; and a body with a formatted
  line over 20,000 chars — highlight.js keyword tables and symbol lists
  (lines up to 130k chars) made 7 prompts of 33–84k tokens, over the
  model's 32,768, and every one failed. On Claude Code: 5 + 6 bodies.

Tests (red first, each watched fail): `vendor_bodies_test.rs` —
`an_unrestored_factory_bodys_identifiers_are_asked_of_the_model`,
`a_restored_name_is_held_not_asked` (red with the holds disabled: the
restored name was asked), `an_unchanged_body_carries_its_names_from_the_prior`
(0 asks, identical text), `bodies_are_joined_in_path_order_and_returned_in_input_order`,
`the_joined_text_splits_back_into_its_bodies`.

Stub check (bun-dev ts-patch, names mode): 125 bodies named, 1,086
restored names held; a self-hop on its own output reads `prior used`, the
vendor tree is byte-identical, and the whole run sends 4 requests.

## Phase 2: the hop

`npm run eval -- score exp169-names-hop2 --pairs 215->216` with exp166's
names settings (full names index `cb64c1f5…`, scratch base, cold, under
`/work/heavy.lock`). Before = `exp166-names-hop2` (`210ed0558`), re-scored
with exp170's split scorer (`rescore.ts`: OLD-FIELDS IDENTICAL, partitions
add up). The first attempt, `exp169-names-hop` (`92aa23e9d`), was refused
by the harness (7 failed calls) and is not scored. It found the two
leave-as-extracted rules above.

### Gate table

| check                                               | result                                                            |
| --------------------------------------------------- | ----------------------------------------------------------------- |
| `npm run check`                                     | **ALL 13 STAGES PASSED** (at `a706a0ae7`, rebased on `32caeb572`) |
| neutrality, default mode (2.1.85→2.1.86, warm)      | **NEUTRAL**: 0 files / 0 lines, cache +0 / +0, exit 0 / 0         |
| neutrality, report mode (`library.matching=report`) | **NEUTRAL**: 0 files / 0 lines, cache +0 / +0, exit 0 / 0         |
| hop run                                             | exit 0, 0 provider errors, boot OK                                |
| warm self-hop                                       | byte-identical, 0 cache writes                                    |
| cold self-hop                                       | 8 lines (reference 2–184)                                         |

### Results table (totals, then per surface; bands are 0f338ffa's)

| KPI                                          | exp166 hop2          | **exp169 hop2**      | Δ                            |
| -------------------------------------------- | -------------------- | -------------------- | ---------------------------- |
| treeLn                                       | 25,537               | 25,500               | −37                          |
| novel                                        | 986                  | 986                  | **= (bar)**                  |
| realLn                                       | 122,066              | 122,066              | **= (bar)**                  |
| mints                                        | 54                   | 52                   | −2 (~0 ±33; bar ≤ 54 ✓)      |
| newName                                      | 761                  | 737                  | −24                          |
| noise / noiseLn                              | 292 / 4,919          | 289 / 4,927          | ~0 / ~0                      |
| reloc                                        | 343                  | 346                  | ~0 ±35                       |
| vendorLn (reducible)                         | 183                  | 183                  | =                            |
| vendorReal                                   | 289                  | 1,193                | **+904** (formatting, below) |
| **app:** treeLn / noise / noiseLn            | 25,471 / 286 / 4,645 | 25,434 / 283 / 4,653 | −37 / ~0 / ~0                |
| **app:** mints / newName / reloc             | 52 / 750 / 331       | 52 / 726 / 334       | = / −24 / ~0                 |
| **app:** novel / realLn                      | 982 / 121,995        | 982 / 121,995        | =                            |
| **vendor:** mints                            | 2                    | 0                    | −2                           |
| **vendor:** noise / noiseLn / novel / realLn | 6 / 274 / 4 / 71     | 6 / 274 / 4 / 71     | =                            |
| **vendor:** vendorReal                       | 223                  | 1,127                | +904                         |
| cross: treeLn / stmts                        | 132 / 7              | 132 / 7              | =                            |
| cold self-hop (ln)                           | 8                    | 8                    | (2–184)                      |
| requests, fresh 2.1.215 run                  | 68,238               | 75,198               | +6,960 (+10%)                |
| requests, the hop                            | 1,903                | 1,951                | +48                          |
| hop leg wall time                            | 327 s                | 347 s                | +20 s (vendor pass 23 s)     |

**Bars:** novel/realLn byte-equal ✓; mints 52 ≤ 54 ✓; minified vendor
bindings 21,428 → 917 ✓ (itemized below); vendor stability 66,559 equal /
0 different ✓; name re-roll lines 0 ✓; app columns not worse ✓ (newName
better by 24, outside its ±13 band; the rest inside); warm self-hop ✓;
cold self-hop ✓; app bytes in vendor/ 0 ✓.

**What got worse: vendorReal, +904 lines, and it is not dependency change.**
Both labels add the same 26 vendor files on this hop (23 new ajv factories,
three `@anthropic-ai/*` era modules) and change nothing else: the 2,031
vendor files the two versions share are byte-identical across the hop in
BOTH labels. exp166 printed the 23 factories minified, one line each; this
lane formats them. The added files hold 263 lines in exp166's tree and
1,167 in this one, and 1,167 − 263 = 904. The scorer counts lines, so the
same additions cost more lines. (The added ajv files also land in
different folders, `ajv/` here and `ajv-2020/`, `dependent-required/`, …
in exp166. That is the vendor namer's cold draw for new modules: exp166's
own two cold runs differ by 238 vendor path lines on the base. This lane
runs after that namer and never touches paths.)

**Ask volume and time.** A fresh Claude Code run sends about 7,000 more
requests (+10%), and the vendor pass takes 137 s of wall time. A hop with a
prior sends +48 requests, and the pass takes 23 s. 19,001 restored names
are held on the base and 19,096 on 2.1.216.

**Minified vendor bindings** (exp166's walkthrough over the hop tree,
`/work/exp169-data/hop-walkthrough/`, CC-derived, not committed): vendor
`left-minified` falls from 21,428 to 917.

- 436 are in the 11 bodies left as extracted: 218 in the 5 lone-surrogate
  bodies and 218 in the 6 long-line bodies (highlight.js languages, symbol
  tables).
- 481 are in named bodies, and they are the walkthrough's ≤3-character
  test catching real short words: `key` 155, `lib` 30, `obj` 29, `fs` 21,
  `red` 19, `url` 14, `err` 13, `tag` 11, `mod`, `ctx`, `idx`, `str`, …
  About 30 are single letters (`a`, `b`, `c`, `i`, `j`) the model chose.
- Vendor `kept` rises from 2,176 to 25,929: the walkthrough has no trail
  for this pass, so it labels its names "kept". `index-restored` reads
  32,656 against 35,917 because minifier-shaped restorations (`i`, `k`)
  are re-asked and renamed, as exp166's rule says, and so no longer match
  the restored-name test.
- src: left-minified 665 → 679, equalsPrior False 956 → 1,025. Both are
  cold-draw scale (app columns inside bands).

**Vendor stability and re-rolls** (exp168's `vendor_distribution.ts`,
`/work/exp169-data/vd-compare.md`): name re-roll lines 0 → 0, mint-bearing
vendor lines 1 → 0, identical factory code keeps its names 57,727 →
57,727 (0 different). Its "vendor body names" buckets move to "factory not
paired to the raw bundle" (51,117). That is an instrument limit: it pairs
body text to the raw bundle, and formatted bodies no longer pair.

**Placement is untouched.** The vendor record's runtime identifiers are the
same set in both trees (1,647 factories), `src/`'s `lib_<hash>` references
are identical, and cc's labelled app bytes under `vendor/` are 0
(`vendor-precision check … --truth cc216`; factories hold no labelled
units, see phase 0).

## What this does not look at (rule 8)

- The bun-dev runs were not repeated: this lane renames identifiers inside
  vendor files and does not move them. The phase-0 `<self>` bytes are in
  `vendor/` in every mode.
- The walkthrough's "left-minified" is a ≤3-character name test. It
  counts `key` and `url` as minified, and it has no trail for this pass's
  names.
- `--dump-asks` rows do not include this pass's asks (its dispatch log is
  off), and `--stats-json` carries no vendor-pass block; its counts are on
  the progress line.
- One hop, one cold draw. The vendor pass's names on a FRESH run are
  model draws; across a hop they carry.

## Follow-ups

- The factory parameters are Bun's `(exports, module)`. The model names
  them (`moduleContext`, `moduleExports`) because the real names are on the
  never-use list. Naming them by rule would save two asks per body and
  give the right names.
- A CommonJS app's own modules sit in `vendor/` (phase 0). Moving them out
  is a placement lane.
- Lone surrogates in oxc's ESTree JSON break any text that holds one,
  including the era. The era has none on Claude Code today; a general fix
  belongs in `ingest::parse_estree_json`.
