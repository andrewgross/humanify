# What a new bundler / minifier plugin must provide

Written 2026-10-03 against main `549bcff4`. An audit plus a spec; it changes
no code. Andrew's direction: "Keep checking for other places where we are
hard coding pieces that should be loaded dynamically in the pipeline based on
what we detect, and see if we can define a clear spec of what a new minifier
detector/cleaner would need to add/implement as a plugin."

Everything in the pipeline was built on Bun bundles (the Claude Code
releases). esbuild was added as a second bundler on 2026-10-02 by reusing the
Bun code path. This file answers two questions:

1. **Where does the pipeline still assume Bun** (or some other specific
   bundler or minifier), and what happens on other input? — the inventory.
2. **What would a new bundler or minifier have to supply** to be handled
   properly, and which of those pieces are real plug points today? — the
   spec.

Read with [`pipeline-stages.md`](./pipeline-stages.md) (the twelve stages)
and [`responsibility.md`](./responsibility.md) (who owns which question).

**Update 2026-10-04 (finding #76, branch `feat/detected-toolchain`) — the
TOOLCHAIN.** Part 4's steps 1 and 3 are done. A run's plugin pieces are now
chosen in ONE place, `humanify_core::toolchain::resolve_toolchain`, at the
start of the run, from the detection verdict and the `--bundler` /
`--minifier` flags, and handed to every stage as values — no stage below it
compares a bundler or minifier NAME any more. Each choice is recorded with
its reason (`flag`, `detected`, `fallback`, `only-implementation`) in the
`--stats-json` file's `toolchain` block and in a `-vv` log line. Pieces it
holds: the unpack adapter (P2), the vendor record and its stamp (P5), the
library detector (P6), the never-rename lists (P7), the name profile (P10),
the module-layout record (P11) and the per-bundler tuning (P14); plus four
slots whose only implementation is today's Bun/esbuild behaviour — the
module wrapper grammar (P3), the interop helpers (P8), the bundle layout
(P9) and "which file is the app" (P13). Fixed by it: I9 (one dispatch
site), I14 (the finish accepts any registered vendor-record adapter's
stamp), I21 (the post-split reconcile uses the run's never-rename lists),
I23 (the group size is a tuning piece), I24 (the dead lane table is gone).
The statuses below are updated where they changed; the original audit text
is kept for the record. Neutral: byte-identical output against main's
binary on the five e2e fixture pairs and a stub-LLM Claude Code pair
(2.1.118 → 2.1.119, trees and asks).

**Update 2026-10-04 (finding #78, branch `fix/finish-own-vendor-and-interop`;
toolchain review R4, R6, R14).** P5: the split and the finish read the
vendor record THIS run's adapter wrote, handed down in memory — never the
one lying in the output folder, which may be an earlier run's (a vendor-less
bundle run into an esbuild run's folder used to re-link the stale vendor
files and fail the finish); files under `vendor/` this run did not write are
left alone and named in a WARNING. P8: the helper shapes (I16), the
reserved names (I17), the helper file's name (`.humanify/__bun-runtime.js`)
and the bound module helper are answered by the toolchain's
`InteropHelpers` piece (Bun's is the only implementation; nothing in the
esbuild adapter differs). P6: the two identical library detectors are one,
`LibraryDetector::VendorRecord`, chosen by "does the adapter write a vendor
record?". Byte-identical output vs main on the five e2e fixture pairs and
the stub-LLM 2.1.118 → 2.1.119 pair; the stats `libraryDetector` row now
reads `vendor-record`.

**Update 2026-10-04 (finding #79, branch `feat/module-grammar-seam`;
toolchain review R11, R2, R7, R12).** P3: `ModuleWrapperGrammar` has real
methods (the module helper, the factory classification, the factory
argument's forms, the lazy-init helpers) and every caller asks the run's
piece — the unpack, the naming stage's per-file skip (I10, behaviour
unchanged and measured), the artifact dump, and the fresh-grouping split,
whose own "most-used wrapper" guess is gone. P12: one lazy-init recogniser
(Bun's and esbuild's shapes) for the load order and the fossil layout;
I27 closed. Byte-identical vs main on the five e2e fixture pairs and the
stub-LLM 2.1.118 → 2.1.119 pair.

**Update 2026-10-04 (finding #80, branch `fix/naming-uses-unpacked-factories`;
toolchain review R2).** I10 CLOSED: the naming stage no longer re-runs the
module grammar on every file. Which code is a bundled module is the
UNPACKER's decision, made once: what it extracted is in `vendor/` files of
its own, and a factory it kept in the app is app code, named in the main
waves with call-graph context (on esbuild such a factory used to be skipped
and left to the coverage sweep). No unpacker (passthrough, webcrack) means
no factory list: everything in the file is app code. Byte-identical vs main
on the real Bun pair and the five existing e2e fixture pairs; a sixth
fixture, `esbuild-kept-factory`, holds the changed case.

## Words used here

- **Bundler** — the tool that glued many source files into one file (Bun,
  esbuild, webpack, …). It decides how each module is wrapped and how
  modules call each other.
- **Minifier** — the tool that shortened the names (`config` → `a`). It
  decides what a "made-up" name looks like. Bun and esbuild are both; terser
  and swc are minifiers only.
- **Plugin** — the set of pieces below that teach the pipeline one bundler or
  one minifier.
- **Selected by detection** — the pipeline looks at the input, decides which
  bundler/minifier made it, and picks the matching piece. The opposite is
  **hard-coded**: the Bun answer is used no matter what the input is.
- Status of each spec piece: **EXISTS** (a real plug point: a list you add
  to, chosen from detection), **PARTIAL** (a plug point exists but Bun is
  wired in somewhere — listed), **MISSING** (there is one hard-coded answer;
  a refactor comes first).

## Part 1 — the inventory

### Severity scale

- **High** — on a mainstream build from another bundler, a whole stage is
  unavailable or the output is wrong.
- **Medium** — loses quality silently, gives two different answers to one
  question, or will break the moment a new plugin is added unless someone
  knows to edit it.
- **Low** — harmless on other input today, cosmetic, or falls back to a safe
  (more conservative) behaviour.

### Totals

**35 findings: 2 High, 12 Medium, 21 Low** (31 in the pipeline, 4 in the
measurement harness). Of the 31 pipeline findings:

- 6 are already selected by detection, by the adapter, or by shape (good);
- 13 are hard-coded but harmless or safely conservative on other input;
- 3 are harmless today but become wrong as soon as the name-profile work
  reads the minifier verdict (I3-I5);
- 9 are hard-coded and wrong on other input.

The two High items:

- **Minified esbuild builds are not recognised** (I2). esbuild's detection
  looks for its helper NAMES (`__commonJS`, `__toESM`, …), which `--minify`
  shortens to single letters. A production esbuild bundle therefore detects as
  "unknown", gets the do-nothing adapter (the whole input as one `index.js`),
  and loses vendor extraction, vendor naming and the module-layout split —
  even though the extraction code would handle it (a minified esbuild build
  uses exactly Bun's factory shape; `modules.rs` says so).
- **The split only understands one bundle layout** (I25): the entire program
  wrapped in one big function with at least 50 names declared directly
  inside it (Bun's CommonJS wrapper, esbuild's `--format=iife`). An ES-module
  build (esbuild, Bun or rollup with `--format=esm`, which is the default for
  many modern builds) has its statements at the top level, so `--split`
  cannot split it. Naming still works — and since finding #87 (2026-10-05)
  the run finishes with the named output unsplit and a WARNING, instead of
  exiting 1 after naming.

### The findings, by stage

Status column: **selected** = chosen from detection or from the adapter
(good); **harmless** = hard-coded, but other input is unaffected or gets a
safe fallback; **wrong** = hard-coded and misbehaves on other input (example
given).

#### Stage 1 — detection (`crates/humanify-core/src/detect/`)

| #   | where                                                   | what it assumes                                                                                           | belongs to           | status                   | sev    |
| --- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------ | ------ |
| I1  | `detect/signals.rs:168-221` (`detect_bun_bundler`)      | Bun = a `// @bun` first line, or a `{exports:{}}` marker plus a `createRequire` import                    | Bun                  | selected                 | Low    |
| I2  | `detect/signals.rs:122-135` (`detect_esbuild`)          | esbuild = its helper NAMES appear (`__commonJS`, `__toESM`, `__toCommonJS`, `var __export`, `__require`)  | esbuild (unminified) | **wrong**                | High   |
| I3  | `detect/signals.rs:283-306` (`detect_bun_minifier`)     | Bun's minifier = more than 10 names like `$aB` in the first 16K characters                                | Bun                  | harmless today, see note | Medium |
| I4  | `detect/signals.rs:234-254` (`detect_terser`)           | terser = `void 0` or `!0` anywhere; always the lowest confidence                                          | terser               | harmless today, see note | Medium |
| I5  | `detect/signals.rs:256-281` (`detect_esbuild_minifier`) | esbuild's minifier = a `// some/path.js` comment in the first 200 characters                              | esbuild              | harmless today, see note | Medium |
| I6  | `detect.rs:26`                                          | every signal must sit in the first 16K characters                                                         | all                  | harmless                 | Low    |
| I7  | `humanify-model/src/detection.rs:14-55`                 | `rollup` is a selectable bundler with no detector and no adapter; `parcel` is detected but has no adapter | rollup, parcel       | harmless (do-nothing)    | Low    |

- **I2 example:** `esbuild --bundle --minify --format=iife` writes
  `var c=(e,t)=>()=>(t||e((t={exports:{}}).exports,t),t.exports)` — no
  `__commonJS` anywhere. No `// @bun` line and no `createRequire` import
  either, so detection says "unknown", and the run unpacks nothing.
  `--bundler esbuild` (or `bun`) as an override would work.
- **I3-I5 note:** today the minifier verdict is used for exactly one thing —
  adding swc's helper names to the never-rename list (I20). So a wrong
  minifier verdict costs nothing yet. It becomes **wrong** the moment the
  name-profile work (`feat/minifier-name-profiles`) picks a profile from it:
  - I3: jQuery- or AngularJS-flavoured code (`$http`, `$scope`, `$el`) reads
    as "Bun's minifier".
  - I4: every minified file has `void 0` or `!0`, so "terser" is the
    fallback verdict for any minified bundle whose other signals miss —
    including a Bun bundle with few `$` names in its first 16K.
  - I5: the `// path.js` comment is what an **unminified** esbuild or Bun
    build writes before each module. The verdict "esbuild minifier" here
    actually means "not minified" — the profile should be "none".

#### Stages 2-3 — choosing and running the unpacker (`crates/humanify-core/src/unpack/`, `modules/`)

| #   | where                                                                                              | what it assumes                                                                                                                                                                                                                                                                                            | belongs to   | status                                                                | sev    |
| --- | -------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ | --------------------------------------------------------------------- | ------ |
| I8  | `unpack.rs:61-148` (`UnpackAdapter`, `ADAPTERS`, `select_adapter`)                                 | the adapter list; each adapter says which detected bundler it takes                                                                                                                                                                                                                                        | all          | selected                                                              | Low    |
| I9  | `unpack.rs:152` (`run_adapter`), `humanify-cli/src/unminify.rs:88`, `humanify-cli/src/main.rs:429` | the adapter is RUN from three places; two of them special-case `Bun \| Esbuild` with `matches!` to pass the vendor namer and the prior — **FIXED 2026-10-04**: `unpack::run_adapter` is the one dispatch site; every caller hands it `AdapterRun` (namer, prior, shim) and each adapter takes what it uses | Bun, esbuild | fixed                                                                 | Medium |
| I10 | `modules.rs:181` (`identify_cjs_factory`) via `prior.rs:597` → `graph.rs:1479,1599`                | EVERY input file (any bundler) is searched for Bun's `{exports:{}}` marker, then esbuild's `__commonJS`; whatever is found marks "third-party factory" bodies, and the naming stage skips every function inside them                                                                                       | Bun, esbuild | fixed (#80: the naming-time scan is deleted; the unpack decides once) | Medium |
| I11 | `unpack/bun.rs:897` (`identify_bun_require`)                                                       | the module `require` is Bun's `createRequire(import.meta.url)` alias, rewritten back to `require(` in vendor files                                                                                                                                                                                         | Bun          | harmless                                                              | Low    |
| I12 | `unpack/bun.rs:55`, `modules/vendor_names.rs:352`                                                  | the vendor record is named `vendor/_bun-modules.json` / `BunModulesManifest` for both adapters; its doc still says the stamp is "always bun"                                                                                                                                                               | Bun          | harmless (naming only)                                                | Low    |
| I13 | `unpack/bun.rs:152` (`load_prior_vendor`)                                                          | a prior tree's vendor record is read whatever adapter wrote it                                                                                                                                                                                                                                             | all          | harmless                                                              | Low    |
| I14 | `finish/driver.rs:83-87` (`load_bun_manifest`)                                                     | the finish re-links vendor files only when the record's stamp is `"bun"` or `"esbuild"` — **FIXED 2026-10-04**: it accepts the stamp of any registered adapter that writes the record (`UnpackAdapter::of_vendor_record_stamp`)                                                                            | Bun, esbuild | fixed                                                                 | Medium |
| I15 | `modules.rs:58`, `modules.rs:478` (`factory_arg_function`)                                         | the two factory spellings (function passed directly; esbuild's `{"path"(exports, module){…}}`) are both accepted on every input                                                                                                                                                                            | Bun, esbuild | harmless (by shape)                                                   | Low    |

- **I9 example:** add a third vendor-extracting adapter to `run_adapter`
  only, and `humanify unminify` routes it through the generic branch with
  default options — no LLM vendor naming, no prior carry — without an error.
- **I10 example:** in a terser-minified webpack runtime the module cache is
  `var o=n[e]={exports:{}}`. The marker matches, and the "helper" is guessed
  as the nearest preceding declaration, `o`. The classifier then matches
  callees **by name, not by binding**: any top-level `var x=o(function(){…})`
  (a different `o`, say a memoiser) has its whole body treated as
  third-party code and none of its functions are named. Nothing reports it.
  Not observed on a real build; the shape makes it possible on any input,
  because the check runs whatever was detected. **FIXED 2026-10-04 (#80):**
  the naming stage no longer searches for factories; only the unpack does,
  and only when a bundler adapter was chosen.
- **I14 example:** a new adapter writes the same vendor record with its own
  stamp; `finish` silently skips the re-link, and the vendor files are left
  as bare factory expressions the runnable tree cannot load.

#### Stage 3 — what a vendored module may still reach (`unpack/bun/scope.rs`, `finish/relink.rs`)

| #   | where                                               | what it assumes                                                                                                                                                                                 | belongs to | status                                                                                                      | sev    |
| --- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ----------------------------------------------------------------------------------------------------------- | ------ |
| I16 | `unpack/bun/scope.rs:332` (`helper_shape`)          | the two module-interop helpers are recognised by Bun's exact shape: `__toESM` = 3-parameter arrow mentioning `"default"` and `.__esModule`; `__toCommonJS` = 1-parameter arrow with a `WeakMap` | Bun        | harmless — behind `InteropHelpers::recognise` since 2026-10-04 (#78)                                        | Low    |
| I17 | `unpack/bun/scope.rs:236` (`canonical_names_taken`) | if the bundle already uses the names `__toESM` / `__toCommonJS`, every factory that calls those helpers stays in the app, together with every factory that depends on it                        | Bun        | **wrong** — the names are `InteropHelpers::canonical_names` since 2026-10-04 (#78); the esbuild fix is open | Medium |
| I18 | `finish/relink.rs:47` (`BUN_RELINK_RUNTIME`)        | the helper file written into `.humanify/__bun-runtime.js` is Bun's implementation of `__commonJS`, `__esm`, `__toESM`, `__toCommonJS`                                                           | Bun        | harmless — `InteropHelpers::relink_runtime` / `runtime_file` (#76, #78)                                     | Low    |

- **I16:** esbuild's `__toCommonJS` has no `WeakMap`, so it is not
  recognised and falls to the next rule (a "bridged read" through the file
  that owns it) — still correct, just a different route.
- **I17 example:** an **unminified** esbuild build declares `var __toESM = …`
  by that name. When a CommonJS module `require()`s an ES module, esbuild
  writes `(init_x(), __toCommonJS(x_exports))` inside the factory, so that
  factory — and everything that requires it — is kept in `src/` instead of
  `vendor/`. Library code then gets named by the LLM as if it were the app.
- **I18:** esbuild calls its factories the same way (`(exports, module)`),
  which is why the esbuild e2e fixture boots. A bundler with a different
  factory calling convention would need its own helper file.

#### Stages 6-9 — formatting and naming

| #   | where                                                         | what it assumes                                                                                                                                                                                                                                   | belongs to            | status                      | sev    |
| --- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- | --------------------------- | ------ |
| I19 | `format/beautify.rs`                                          | undoes general minifier idioms (`void 0`, `!0`, comma chains, flipped comparisons)                                                                                                                                                                | all minifiers         | harmless (generic)          | Low    |
| I20 | `rename/eligibility.rs:59-159` (`create_skip_set`)            | never-rename lists per bundler (webpack, esbuild) and minifier (swc), chosen from detection — but the always-on rules (any `__word…` name, any `_word_word` name) already cover every listed entry except swc's `_extends` / `_inherits`          | webpack, esbuild, swc | selected (mostly redundant) | Low    |
| I21 | `finish/driver.rs:330`                                        | the post-split rename pass builds its never-rename list as `Eligibility::new(Some("bun"), Some("bun"))` — whatever was detected — **FIXED 2026-10-04**: it reads the run's `NeverRename` from the toolchain, the same value the naming stage uses | Bun                   | fixed                       | Medium |
| I22 | `rename/floor.rs:123` (`is_bun_token`) and its 9 callers      | "does this name look minifier-made?" is answered with Bun's shape on every input: the minted-name meter, the below-floor rule, vote candidacy, the family permute, the answer checks (echo, borrowed stem)                                        | Bun's minifier        | **wrong** (in flight)       | Medium |
| I23 | `naming/waves/processor.rs:1195`, `naming/driver/era.rs:487`  | module-level names are sent to the LLM in groups of 15 for esbuild, 10 for everything else (2026-10-04: the toolchain's `BundlerTuning` piece, no string check)                                                                                   | esbuild               | selected                    | Low    |
| I24 | `humanify-cli/src/util.rs:118` (`default_module_concurrency`) | a per-bundler lane width (esbuild 40, others 20) — but nothing calls it; only the maximum is used — **DELETED 2026-10-04** (the one value read, 40, kept as a constant)                                                                           | esbuild               | fixed                       | Low    |

- **I21 example:** for an swc build, the naming stage never renames
  `_extends`, but the post-split pass may. Today this cannot fire for Bun or
  esbuild input (I20: their lists add nothing the always-on rules miss).
  It is the "two owners, two answers" shape `responsibility.md` warns about.
- **I22 example:** terser and esbuild name variables from a letter-frequency
  alphabet (`e, t, n, r, …`, then `ee, te, …`, then 3-letter names). A large
  bundle uses many 3-letter, digit-free names. `is_bun_token` only flags
  short names, names with `$`, and letter-plus-digit heads, so those 3-letter
  names are invisible to the minted meter, and an answer that hands one back
  unchanged is accepted. This is the code `feat/minifier-name-profiles` is
  turning into per-minifier profiles (piece P10).

#### Stages 8, 10-12 — matching, placement, split, emit, finish

| #   | where                                                                                                                                                                                                                                                | what it assumes                                                                                                                                                      | belongs to            | status                  | sev    |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- | ----------------------- | ------ |
| I25 | `modules/wrapper.rs:26,54,68` and its callers: `place/input.rs:100-107`, `emit/stable_split.rs:205,246`, `graph.rs:1101`, `prior.rs:544,595`, `twins.rs:171`, `naming/driver/era.rs:421`, `naming/passes/family_permute.rs:209`, `unpack/bun.rs:542` | the bundle is ONE wrapper function (called, `!`-called, `.call`ed, or bare) holding the whole program, with at least 50 names declared directly in it                | Bun CJS, esbuild iife | **wrong**               | High   |
| I26 | `humanify-cli/src/unified.rs:669-690`                                                                                                                                                                                                                | the split, the prior carry and the reports read the LAST file the naming stage processed — for Bun and esbuild that is `runtime.js`, which the adapter writes last   | Bun, esbuild          | **wrong** for webpack   | Medium |
| I27 | `emit/load_order.rs:832`, `emit/bun_helpers.rs:107` (`identify_bun_lazy_init`)                                                                                                                                                                       | the lazy-init helper is found by Bun's exact text `x && (y = x(x = 0))`; its calls are then known to do nothing at load time, so the statements around them may move | Bun                   | harmless (conservative) | Low    |
| I28 | `emit/load_order.rs:710` (`registrar_name`)                                                                                                                                                                                                          | the export registrar is found by shape (a two-parameter arrow with `for (k in src) defineProperty(target, k, {get: src[k]})`)                                        | Bun, esbuild          | selected (by shape)     | Low    |
| I29 | `unpack.rs:111` (`provides_module_fossils`)                                                                                                                                                                                                          | which adapters record the original module layout ("fossils") the split follows                                                                                       | Bun, esbuild          | selected                | Low    |
| I30 | `twins/fossil.rs:121` (`is_esm_helper`) via `twins/gates.rs:1365`                                                                                                                                                                                    | the module-layout grammar (Bun's and esbuild's `__esm` init shapes) is also read by matching on every input, not only when the adapter declares fossils              | Bun, esbuild          | harmless (by shape)     | Low    |
| I31 | `finish/scaffold.rs:342-354`                                                                                                                                                                                                                         | the run instructions written into the tree talk about Bun                                                                                                            | Bun                   | harmless (text)         | Low    |

- **I25 example:** `esbuild --bundle --format=esm` writes the modules as
  top-level statements, with `export { … }` at the end — no wrapper. The
  split cannot read it ("the run's input bundle has no recognizable bundle
  wrapper"). The function graph falls back to the program scope, so naming
  runs; only the split tree is missing — written unsplit with a WARNING
  since #87, it used to exit 1. Bun's own `--format=esm` output is the
  same.
- **I26 example:** webcrack unpacks a webpack bundle into one file per
  module. With `--split`, the split is handed whichever module file was
  processed last; the wrapper check on the original bundle then usually
  fails (webpack keeps its modules as object entries, not as names declared
  in the wrapper, so its wrapper rarely reaches 50 names), and the run errors
  out rather than producing something wrong. Today the split is, in
  effect, Bun-and-esbuild only.
- **I27:** unminified esbuild's lazy init is
  `fn && (res = (0, fn[…])(fn = 0))`, which does not match, so its calls are
  treated as having load-time effects and fewer statements move. Safe, just
  less freedom. (A minified esbuild build uses Bun's shape and matches.)

#### The measurement harness (separate from the pipeline)

| #   | where                                                   | what it assumes                                                                                                                                                                                                                                                           | sev    |
| --- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| H1  | `experiments/lib/boot-gate.sh`                          | the boot check runs the tree with `bun` and asks it a live Claude Code question — it can only judge Claude Code trees                                                                                                                                                     | Low    |
| H2  | `experiments/034-eval-harness/` (the four scored pairs) | every scored pair, noise band and self-hop range is a Claude Code (Bun) release; nothing scores esbuild or webpack output                                                                                                                                                 | Medium |
| H3  | `experiments/lib/js/wrapper.ts:14`                      | the harness's own copy of the ≥50 wrapper rule; statement-level KPIs cannot read a bundle without a wrapper                                                                                                                                                               | Low    |
| H4  | `test/e2e/fixtures/`                                    | the only real bundler builds in the gate are two **unminified esbuild iife** builds; there is no committed real Bun build (Bun is covered by synthetic snippets in `crates/humanify-cli/tests/pipeline_stages.rs` and unit tests), no minified build, no ESM-format build | Medium |

## Part 2 — the spec: what a plugin must provide

Sixteen pieces, in pipeline order. "A plugin" is usually a bundler plugin
(P1-P9, P11, P12, P14) or a minifier plugin (P1, P10, P7); Bun and esbuild
are both.

### Summary

| piece                                       | stage  | status                        | a new plugin today must…                                                                                                                                                                                             |
| ------------------------------------------- | ------ | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P1 Detection signals                        | 1      | **PARTIAL**                   | add a signal function (each signal with its strength) + list entry + enum value; the bundler verdict is by strength, not order (2026-10-04, #81); minifier verdicts are not yet trustworthy enough to drive anything |
| P2 Unpack adapter (choose + run)            | 2-3    | **EXISTS** (2026-10-04)       | add an enum value, a `supports` rule, and its arm in the one dispatch site `unpack::run_adapter`                                                                                                                     |
| P3 Module wrapper grammar (factories)       | 3, 8-9 | **EXISTS** (seam, 2026-10-04) | add a `ModuleWrapperGrammar` value (helper, classification, factory argument, lazy-init helpers); every caller asks the run's (#79); the unpack alone decides (#80)                                                  |
| P4 Original source-path handover            | 3      | **EXISTS**                    | nothing, or fill `FactoryRecord::source_path` when the bundler keeps paths                                                                                                                                           |
| P5 Vendor record, vendor names, prior carry | 3, 5   | **EXISTS** (2026-10-04)       | reuse the record format; declare its stamp (`UnpackAdapter::vendor_record_stamp`) — the split and finish read the record THIS run wrote (#78)                                                                        |
| P6 Library detection                        | 4      | **EXISTS**                    | nothing if it writes the vendor record (`LibraryDetector::VendorRecord` is chosen by that); else add a detector                                                                                                      |
| P7 Never-rename helper names                | 7-9    | **EXISTS** (2026-10-04)       | add a list to `NeverRename` (`rename/eligibility.rs`); every consumer gets the run's value from the toolchain                                                                                                        |
| P8 Interop helpers for vendored code        | 3, 12  | **EXISTS** (slot, 2026-10-04) | add an `InteropHelpers` value: its shapes, standard names, helper file and module helper (Bun's is the only one; I17 still open)                                                                                     |
| P9 Bundle layout ("container") grammar      | 7-12   | **PARTIAL** (seam)            | every reader asks the toolchain's `BundleLayout` (2026-10-04); one implementation — add an ES-module top level                                                                                                       |
| P10 Name profile (minifier naming shape)    | 9      | **EXISTS** (#75)              | add a `NameProfile` (`rename/name_profile.rs`); chosen by the toolchain                                                                                                                                              |
| P11 Module-layout record ("fossils")        | 8, 10  | **PARTIAL**                   | per-adapter flag OFFERS the markers; the split uses them only when they cover >=99% of the app code (#87); the grammar is one shared shape list                                                                      |
| P12 Load-order helper shapes                | 11     | **EXISTS** (2026-10-04)       | registrar by shape works for both; lazy-init comes from the module grammar, Bun's and esbuild's shapes (#79)                                                                                                         |
| P13 Which unpacked file is the app          | 10-12  | **PARTIAL** (slot)            | "the last file processed" (I26) — now the toolchain's `AppFile` rule, read by the naming loop                                                                                                                        |
| P14 Per-bundler tuning                      | 9      | **EXISTS** (2026-10-04)       | add a `BundlerTuning` value; the dead lane table is deleted (I23, I24)                                                                                                                                               |
| P15 Formatting                              | 6      | not a plugin piece            | nothing — the formatter undoes generic idioms and is a frozen spec                                                                                                                                                   |
| P16 Fixtures and tests                      | gate   | **PARTIAL**                   | see the test list in each piece and the checklist                                                                                                                                                                    |

Rules every piece must keep (they come from the repo's history, not from
taste):

- **Deterministic.** Same input, same answer, every run — the warm
  byte-identity checks (neutrality, the warm self-hop) depend on it.
- **Precision first.** When a piece is not sure, it must fall back to the
  conservative answer: leave code in the app rather than extract it, leave a
  name unrenamed rather than guess, refuse rather than half-match. Every
  existing fallback works this way (I16, I27, the scope plan's "stays in the
  app" rule), and a plugin must not break that.
- **Fail loudly on what it cannot handle.** The split already refuses rather
  than splitting badly; a plugin's unpacker should refuse a shape it does not
  recognise rather than guess.
- **Round-trips.** A vendor record or ledger a plugin writes is read back by
  the NEXT release's run (the prior). Its format is a cross-release
  contract: stamp versions and re-key old ones by content (as
  `FACTORY_HASH_VERSION` does), never silently re-interpret them.
- **Selected once, from detection.** A piece should be chosen at the start
  of the run and passed down, never re-guessed deep inside a stage (I10 and
  I21 are what happens otherwise). Since 2026-10-04 the place is
  `toolchain::resolve_toolchain`: a new piece is a field there, chosen
  there, recorded by `Toolchain::record`, and handed down as a value.

### P1 — Detection signals (stage 1) — PARTIAL

**Question it answers:** which bundler and which minifier made this input,
and how sure are we?

**Interface today:** a plain function `fn(&str) -> Vec<DetectionSignal>`
over the first 16K characters, listed in `BUNDLER_DETECTORS`
(`detect.rs`) or called from `detect_minifier` (`signals.rs`). Each
signal names a bundler OR a minifier, with a tier (`definitive`, `likely`,
`unknown`); every bundler signal also carries a **strength**
(`SignalStrength`). The minifier verdict is the highest-tier minifier
signal. `--bundler` / `--minifier` override both
(`humanify_core::toolchain::resolve_toolchain`, where every piece is chosen
from the verdicts).

**The bundler verdict is decided by STRENGTH, never by list order**
(2026-10-04, toolchain review R3/R19; `detect::pick_bundler`, the rules
written on `humanify_model::detection::SignalStrength`). Strongest first:

1. **banner** — the bundler wrote its own name into the header
   (`// @bun`);
2. **own runtime name** — a runtime name only this bundler declares
   (`__webpack_require__`, `__webpack_modules__`, `webpackChunk`,
   `parcelRequire`, `require("_bundle_loader")`); a vendored library can
   mention one, so it ranks under a banner;
3. **shape** — two pieces that together only this bundler writes
   (browserify's `[0].call(` + `.exports}`, Bun's `{exports:{}}` +
   `createRequire` import); each piece alone is common text;
4. **shared helper name** — a name more than one bundler writes: esbuild's
   `__commonJS`, `__toESM`, `__toCommonJS`, `var __export`, `__require` are
   Bun's too (a real `bun build` writes them — `test/e2e/fixtures/bun-bundle`);
5. **weak token** — one common word (`installedModules`): tier `likely`,
   the only rank that is not `definitive`.

The verdict is the bundler whose strongest signal outranks every other
bundler's, at that signal's tier. Two bundlers sharing the strongest rank
is a **tie**: the verdict is `unknown`. Whenever the signals name more than
one bundler, the verdict carries a `conflict` (`resolution`: `strength` or
`tie`, and every candidate at its strongest signal, strongest first), and
every toolchain piece the bundler decided records it: `detected-by-strength`
or `fallback-on-tie` instead of `detected` / `fallback`. The toolchain acts
on a **definitive** verdict only: a `likely` one is reported, never used
(it used to send any file with the word `installedModules` to webcrack).
On every walked Claude Code release (126 versions, 808 input files) only the
banner fires, so the verdicts and the toolchain are byte-identical to before.

**A plugin must provide:**

- a value in `BundlerType` / `MinifierType` and in `SELECTABLE_*`
  (`humanify-model/src/detection.rs`) — the names are also the CLI values;
- a signal function, each bundler signal with its **strength** from the
  ranking above. Only a `definitive` verdict picks the unpacker; a name
  another bundler also writes is a shared helper name, not the bundler's
  own. A minifier signal should say how sure it is, and should be able to
  say **"not minified"** (I5).
- signals that survive minification. A bundler's helper names do not (I2);
  its structural shapes do (`{exports:{}}`, the lazy-init shape).

**Bun today:** `// @bun` first line (banner), or the factory marker plus
the `createRequire` import (shape). Minifier: the `$aB` count (I3, noisy).
An unminified Bun build also fires every esbuild helper name; the banner
outranks them (`bun-bundle` e2e fixture).
**esbuild today:** helper names (shared helper names — Bun writes them
too) — unminified builds only (I2); the minifier signal actually detects an
unminified build (I5).
**Fallback:** bundler `unknown` → the do-nothing adapter (the whole input as
`index.js`); minifier `unknown` → today nothing changes, because only swc's
list reads it.

**What is missing before a minifier verdict can drive anything (P10):**
the verdict needs a "none / unminified" answer, the `unknown`-tier terser
fallback must not be treated as a real verdict, and the Bun `$` count needs
a sturdier signal. Until then a name profile chosen from an `unknown` or
`likely` verdict should fall back to today's behaviour.

**Tests a plugin must ship:** a `detect_test.rs` case per signal, with a
real build's opening bytes, including the minified form; a negative case
showing another bundler's real output does NOT fire it — or, when it does
(as esbuild's names fire on Bun's output), a case showing the ranking
still names the right bundler.

### P2 — Unpack adapter: choosing and running it (stages 2-3) — PARTIAL

**Question:** given the verdict, which unpacker runs, and what files does it
write?

**Interface today:** the `UnpackAdapter` enum and `ADAPTERS` list
(`unpack.rs:61-79`), `supports(detection)`, `name()` (also the string
written into the pipeline config and the vendor record), and
`provides_module_fossils()` (P11). Output: an ordered list of
`UnpackedFile`s.

**A plugin must provide:** an enum value, its `supports` rule, its name, and
an unpack function that writes the tree and returns the files **with the
app's own code last** (P13 depends on that ordering today).

**Where Bun is wired in:** the adapter is dispatched in three places —
`unpack::run_adapter`, `humanify-cli/src/unminify.rs:88` and the `unpack`
verb in `humanify-cli/src/main.rs:429` — and the last two special-case
`Bun | Esbuild` to pass the LLM vendor namer and the prior vendor record
(I9). Bun and esbuild share ONE implementation (`unpack::bun::unpack_bun`);
esbuild differs only by the stamp string it passes.

**Refactor needed:** one dispatch site that always passes the namer and the
prior, so an adapter cannot be half-registered. **DONE 2026-10-04:**
`unpack::run_adapter(adapter, code, out_dir, AdapterRun)` is the only
place an adapter runs (the pipeline and the `unpack`, `libdetect` and
`match` verbs); `AdapterRun` carries the namer, the prior's vendor record
and the webcrack shim, and an adapter ignores what it does not use. The
adapter is chosen by `unpack::choose_adapter`, called only by the
toolchain, which also records why (flag / detected / fallback).

**Fallback:** `passthrough` (last in the list, supports everything).

**Tests:** `unpack_test.rs` cases for selection; the e2e fixture below.

### P3 — Module wrapper grammar: finding the bundled modules (stages 3, 8-9) — EXISTS (seam real, one implementation; I10 closed)

**Question:** where are the bundled third-party modules, and what is each
module's body?

**Interface today:**

- `modules::identify_cjs_factory(source) -> Option<IdentifiedHelper>` — the
  helper that wraps each module (`modules.rs:181`): Bun's `{exports:{}}`
  marker first, then esbuild's declared `__commonJS`.
- `modules::factory_arg_function(arg) -> Option<FactoryArg>` — the one owner
  of "what may the helper's argument look like" (`modules.rs:478`): a
  function, or esbuild's single-key object whose key is the module's path.
- `modules::classify_bun_modules(...) -> BunModuleClassification` — every
  `var X = HELPER(factory)` in the bundle's top-level statements, with a
  content-blind structural hash per body (`factory_structural_hash`,
  versioned by `FACTORY_HASH_VERSION`).
- `unpack::bun::identify_bun_require` — the module `require` alias to
  rewrite in vendor bodies (Bun only, I11).

**Must guarantee:** a factory is something whose body runs only as a
module (precision: everything inside is moved to `vendor/`); the structural
hash ignores the minified names so the same library joins across releases.
A factory the unpack keeps in the app (it reaches a binding a vendor file
cannot resolve) is app code from then on (#80).

**Where Bun is wired in:** the helper is matched by name rather than by
binding. Until #80 the search also ran on EVERY processed file, whatever
was detected, because the naming stage called the classifier too (I10);
now only the unpack runs it, once, and naming takes the unpack's split of
the code as given (extracted modules are vendor files; what stayed is app
code).

**Fallback:** no helper found → no factories → nothing skipped, nothing
extracted.

**Slot (2026-10-04):** `toolchain::ModuleWrapperGrammar` (one value,
`BunAndEsbuild`, whose `identify_factory_helper` is
`modules::identify_cjs_factory`). Its consumers still call the shared
owners directly — routing them through the run's value is Part 4 step 4.

**Seam (2026-10-04, branch `feat/module-grammar-seam`, finding #79,
toolchain review R11/R2/R7/R12):** `ModuleWrapperGrammar` answers the
questions its callers ask, delegating to the shared owners:
`identify_factory_helper` (the module helper), `classify_factories` (every
`var X = HELPER(factory)` in the layout's container), `factory_function`
(the argument's accepted forms) and `lazy_init_helpers` (P12). Bun's and
esbuild's adapters share the one value (`run_adapter` hands both to
`unpack::bun::unpack_bun`). Every pipeline caller takes the run's piece:
the unpack (`AdapterRun::module_wrappers`: the helper scan, the
classification, the extraction), the naming stage's per-file skip
(`prior::build_side_parts` via `NamingConfig`/`PriorMatchInput` and the
`match` verb), the `--dump-artifacts` classification sites, the split's
load order (`SplitOptions::module_wrappers`), and the fresh-grouping
split's vendor bucket — which used to tally "the most-used higher-order
callee, ≥2" (`detect_cjs_helper`, deleted) and so sent an app's own
`forwardRef`/`memo`-style wrapper to `vendor/` (review R7); it now takes
the helper the grammar recognises in the shipped text. That last one is a
behaviour change in the clustered regime only (passthrough/webcrack input
or `--disable fossil-split`); default Bun and esbuild runs place by fossils.
Byte-identical on the five e2e fixture pairs and the stub-LLM real Bun pair.

**I10 / R2 — measured in #79, CLOSED in #80 (below):** the naming stage
re-ran the grammar on every processed file. On the esbuild fixtures it
FINDS `__commonJS` on every naming input (both sides) but no factory is
left in the app (the unpack extracted them all), so it skips nothing —
switching it off changes neither tree nor asks. When a factory does stay
in the app on esbuild (a factory that WRITES an app binding, shown on a
one-line variant of `esbuild-bundle`), the skip removes its inner
functions from the main naming waves and the coverage sweep names them
instead, one function per ask without call-graph context (5 sweep asks
instead of 5 wave asks); on Bun's formatted text the grammar never fires
(the formatter splits `{exports:{}}`), so the same factory would be named
in the waves. Whether to pass the unpacker's factory list down or retire
the naming-time detection was a separate decision.

**I10 closed (2026-10-04, finding #80, Andrew's call: one source of truth,
the unpacker's).** Applied to the files naming actually sees, the
unpacker's factory list contains nothing to skip: every module it
extracted is no longer in the app text (it is a `vendor/` file, named or
filtered as a whole file, as before), and every factory it kept is app
code by its own decision. So "use the unpacker's list" is exactly "naming
does no factory scan": `prior::build_side_parts` no longer classifies, the
graph's factory skip (`build_*_graph`'s `factories` argument,
`modules::is_inside_factory_body`) is deleted, and `module_wrappers` left
`NamingConfig` / `EraOptions` / `PriorMatchInput` / the `match` verb. With
no unpacker (passthrough, webcrack) there is no factory list: the whole
file is app code. On Bun this matches what the scan did (it never fired on
formatted text); elsewhere it changes one thing: a passthrough input that
still holds esbuild's `__commonJS` factories (an esbuild bundle the
detector was not sure of) used to have those factory bodies dropped from
naming and from prior matching, and now names them as app code — the
same answer the Bun-formatted text always got. Red/green
`a_factory_the_unpacker_kept_in_the_app_is_named_in_the_main_waves`;
e2e fixture `esbuild-kept-factory` (the one-line variant, plus the report
line that proves the factory's app write survives). Byte-identical vs main
`8f9a5057` on the real Bun pair (fresh and prior, trees and asks) and the
five existing fixture pairs; on `esbuild-kept-factory` the trees are
identical under the stub and the asks move: fresh 6 sweep asks → 1 (the
five inner functions are wave asks), prior 2 sweep asks → 0 (the new
`tripled` is a wave ask).

**Tests:** `modules_test.rs` cases for the helper and argument shapes from a
real build (minified AND unminified), and a negative case (the helper's
name reused for an unrelated function).

### P4 — Original source-path handover (stage 3) — EXISTS

**Question:** did the bundler keep each module's original file path?

**Interface:** `FactoryRecord::source_path` → the vendor record's
`sourcePath` and the ledger's `fossilModules[].sourcePath`. **Recorded only**
— no name, join or placement may depend on it (it is absent from every
minified build).

**Bun:** never present. **esbuild:** the object key of each unminified
factory and lazy init. **Fallback:** absent. **Tests:** the
`esbuild-bundle` fixture's records.

### P5 — Vendor record, vendor names and the prior carry (stages 3, 5) — EXISTS

**Question:** what is each vendored module called, and how does that name
survive to the next release?

**Interface today:** the vendor record `vendor/_bun-modules.json`
(`BunModulesManifest`: `adapter` stamp, `hashVersion`, `runtimeFile`, one
entry per module with file name, name, where the name came from, structural
hash, captured reads). Naming is a fixed ladder — license banner → a
distinctive repository URL → the prior release's name for the same
structural hash → an LLM guess → `lib_<hash>`. The prior is read by
`unpack::bun::load_prior_vendor`; an old-era record is re-keyed by content.

**Already generic:** the record format, the naming ladder and the content
re-key do not depend on the bundler. A new vendor-extracting plugin should
reuse them. The `lib_<hash8>` fallback has one spelling owner,
`modules::hash_fallback_name` (finding #81, review R28): a classified
module passes its structural hash; a body with no classification (the
unpack's floor for an unrecorded module, the fresh grouping's vendor
bucket) passes `sha256(text)` through `modules::content_fallback_name`.
The text floor named none of the 1,493 vendor files on the real Bun pair.

**Where Bun is wired in:** the finish's re-link only reads records stamped
`"bun"` or `"esbuild"` (I14); library detection selects by the same two
strings (`libdetect.rs:119`); the file name says "bun" (I12, cosmetic, but
renaming it is a migration because prior trees hold it).

**Refactor needed:** the allow-lists should come from the adapter list
("does this adapter write a vendor record?"), not from string matches.
**DONE 2026-10-04:** `UnpackAdapter::vendor_record_stamp` declares it; the
finish asks `UnpackAdapter::of_vendor_record_stamp`, the library detector
is chosen from the adapter value (`LibraryDetector::for_adapter`), and the
unpack writes `adapter.vendor_record_stamp()` into the record.

**The run's own record (2026-10-04, finding #78, toolchain review R6):**
the finish used to re-link whatever `vendor/_bun-modules.json` it found in
the output folder, and nothing clears that folder — so a run into the folder
of an earlier run re-linked the EARLIER run's vendor files (a vendor-less
bundle run into an esbuild run's folder failed its finish re-parsing them;
a different Bun build would have pulled stale modules into its runnable
tree). Now the record the adapter wrote is handed down in memory
(`Unpacked::vendor_record` → `SplitStageInput` → `FinishInput`); the split's
vendor captures (`unpack::bun::vendor_captures`) and the re-link
(`finish::driver::this_runs_manifest`) read only it. Files under `vendor/`
that this run did not write are never re-linked, rewritten or deleted (they
may be the user's) — the finish names them in a WARNING. Still reading the
folder: library detection's vendor-record layer (it runs only right after
its adapter rewrote the record) and the `--dump-artifacts` partitions
family.

**Tests:** a prior → next-release pair in the e2e fixture (the existing
bundle fixtures run fresh, then with `--prior-version`);
`a_rerun_into_the_same_folder_never_relinks_the_earlier_runs_vendor_files`
(crates/humanify-cli/tests/pipeline_stages.rs).

### P6 — Library detection (stage 4) — EXISTS

`libdetect::LibraryDetector`: a registry of two — `vendor-record` (reads
the vendor record) and `default` (banner comments and webcrack's module
paths; supports everything). Since 2026-10-04 (finding #78, toolchain
review R14) the vendor-record detector is chosen by "does the run's adapter
write a vendor record?" (`UnpackAdapter::vendor_record_stamp`), so a
plugin that writes the record gets it with no edit here; one that does not
gets `default`. (It used to be two identical detectors, `bun` and
`esbuild`, each chosen by adapter equality — a third record-writing adapter
would silently have got `default`.) Tests: `libdetect_test.rs`.

### P7 — Never-rename helper names (stages 7-9) — PARTIAL

**Question:** which names belong to the bundler's or minifier's runtime and
must never be renamed?

**Interface today:** `rename::eligibility::create_skip_set(bundler,
minifier)` (`eligibility.rs:59`): a universal list (`exports`, `require`,
`module`, `__filename`, `__dirname`) plus per-bundler / per-minifier lists,
plus two rules that apply to every input: any `__word…` name, any
`_word_word` name.

**A plugin must provide:** its runtime helper names, if they survive
minification and are not already covered by the two always-on rules. (In
practice: almost none. Bun minifies its helpers to single letters, which are
correctly renamed and recognised by shape elsewhere.)

**Where Bun is wired in:** the post-split rename pass hard-codes
`Some("bun"), Some("bun")` (I21). It should receive the run's selection.
**DONE 2026-10-04:** the lists are a typed value, `NeverRename`, chosen
once by the toolchain (from the same bundler + minifier verdicts the naming
stage always read) and passed to the naming stage, the match stage's
graphs, the `match` verb and the post-split reconcile alike. On Bun and
esbuild input the change is invisible: their lists add nothing the two
always-on rules miss. It differs only for an swc-detected input, where the
reconcile now also refuses `_extends` / `_inherits`, as naming always did.
swc's helper names are ONE list, `rename::eligibility::SWC_HELPERS`
(finding #81, review R20): the never-rename set takes all 17, and the swc
minifier signal (`detect::signals::detect_swc_minifier`) reads only the 15
marked `detection_marker` — `_extends` and `_inherits` are Babel helper
names too, so seeing one says nothing about swc. Contents unchanged on both
sides (the `skip-list.json` table and the detector's tests pin them).

**Tests:** the existing `skip-list.json` parity table, extended per plugin.

### P8 — Interop helpers for vendored code (stages 3, 12) — EXISTS (one implementation)

**Question:** when a vendored module refers to something outside itself,
how is that reference kept working in the runnable tree?

**Interface today** (`unpack/bun/scope.rs`, `finish/relink.rs`): every
outside reference from a factory body is sorted into: another factory
(re-linked), a CommonJS wrapper name (fine as is), a **runtime helper** (the
reference is rewritten to a standard name and the finish supplies the helper
from `.humanify/__bun-runtime.js`), a read of an app binding (bridged through
the owner file's getter), or anything else (the factory stays in the app).

**A plugin must provide:** the shapes of its interop helpers, the standard
names they map to, and the helper file's implementation — or nothing, in
which case the references fall to the bridge or stay in the app (safe).

**Where Bun is wired in:** the helper shapes are Bun's (I16); the helper
file is Bun's implementation (I18); the "names already taken" guard keeps
unminified esbuild factories in the app (I17).

**Tests:** the e2e boot check (the split tree must print the same output as
the input bundle), with a fixture that has a CommonJS module requiring an ES
module.

**Slot (2026-10-04):** `toolchain::InteropHelpers` (one value, `Bun`); the
finish's relink writes `interop.relink_runtime()`.

**The whole piece behind the slot (2026-10-04, finding #78, toolchain
review R4):** `InteropHelpers` now answers every interop question —
`recognise` (the helper shapes, I16; Bun's implementation is
`unpack::bun::scope::bun_helper_shape`), `canonical_names` (the standard
names a vendored reference is rewritten to and the relink binds; a bundle
already using one keeps the helpers' callers in the app, I17),
`runtime_file` (`.humanify/__bun-runtime.js`), `module_helper`
(`__commonJS`, the wrap `exports.f = __commonJS(F)`) and `relink_runtime`.
The unpack's scope plan gets the run's piece through `AdapterRun::interop`;
the relink and the `match` verb's vendor wrapping take it as an argument.
Bun's is the only implementation: nothing in the esbuild adapter differs
today (esbuild's `__toCommonJS` does not match Bun's shape and takes the
bridge route, I16). Fixing I17 for unminified esbuild is a separate,
esbuild-only behaviour change — a second `InteropHelpers` value is where it
lands. Byte-identical on Bun and the esbuild fixtures.

### P9 — Bundle layout ("container") grammar (stages 7-12) — PARTIAL (seam real, one implementation)

**Question:** where are the bundle's top-level statements — the list the
split slices into files, the scope whose names are "module-level"?

**Today:** one hard-coded grammar (`modules/wrapper.rs`): the program is a
single wrapper function holding ≥50 names. Ten call sites read it (I25). A
program without a wrapper is treated as "not a bundle" by the split and as
"use the program scope" by naming.

**What a plugin piece would look like:** a function that returns the
container — either "the body of this wrapper function" or "the program's
own top level" — plus how the bundle's entry context is handed in (the
wrapper's `exports`/`require`/`module` parameters for CommonJS, nothing for
an ES module, `import`/`export` statements for ESM). The split, the emit
(`emit/cjs.rs`, which today rebuilds the wrapper's parameters as a shared
context) and the matching inventory would all take the container from it.

**Refactor needed first:** route the ten call sites through one owner that
takes the selection, then add an "ES-module top level" container and teach
the emit to write ESM imports/exports. This is the largest item here and the
one that opens ESM-format bundles from every bundler.

**Fallback:** today's grammar; no wrapper → no split. Since finding #87
(2026-10-05) that is a loud WARNING, not an error after naming: the split
stage checks the layout on the run's original input BEFORE splitting
(`BundleLayout::original_bundle_binding_count`, once — the split is handed
the settled count, `InputGate::Settled`), and on a layout it does not read
writes the named output unsplit (what a run without `--split` writes),
prints `WARNING: --split skipped: <reason> … (docs/plugin-spec.md P9,
I25)`, names that file as the next release's prior and exits 0;
`--stats-json` records `splitMethod.method = "not-split"`. The
`esbuild-cjs`, `esbuild-esm` and `bun-esm-minified` fixtures hold that
behaviour (their known-gap entries are gone).

**Slot (2026-10-04):** `toolchain::BundleLayout` (one value,
`SingleWrapperFunction`). It is recorded, but the ten callers still read
`modules/wrapper.rs` directly — that routing is the refactor above.

**Seam (2026-10-04, branch `feat/bundle-layout-seam`, finding #77):** the
first half of the refactor above is done. `BundleLayout` answers the four
layout questions the pipeline asks, delegating to `modules/wrapper.rs`:
`find_wrapper` (the container, with the ≥50-name gate on the text at
hand), `recognize_wrapper` (the container by shape only, for a text whose
run already passed the gate on its original input),
`original_bundle_binding_count` (that input gate) and
`wrapper_parameter_roles` (the wrapper parameters' roles by position —
`exports, require, module, filename, dirname` — review R5, read by the
runnable emit). Since finding #81 (review R21) those roles ARE
`toolchain::COMMONJS_CONTEXT` — the one list of what Node's CommonJS
wrapper hands a module (name + role). The never-rename set, the unpack's
"resolved in the vendor file anyway", the proximity window's always-kept
names and the known-globals report (its `commonjs` environment) read it
too; they used to be five lists. The match fingerprint's vocabulary
(`matching::features::KNOWN_GLOBALS`) is declared NOT to: it is a frozen
hash input. Still open: the emit's entry call and shared-context runtime
spell the names in their own text. Every pipeline reader takes the run's value from the
toolchain: the graph's module scope, both match sides, the twins'
statement inventory, the fresh-era freeze, the family permute, the unpack's
classification, the split (input gate, wrapper body, vendor bridges,
stale-ledger re-derivation, placement carry), the runnable emit, the
artifact dump's classification sites, and the finish's bundle carry — which
had a SECOND rule of its own ("the first function with exactly N
statements", review R9) and now uses `recognize_wrapper` plus a count check
that abstains (`wrapper-statement-count-mismatch`) instead of carrying into
an inner function. Byte-identical on the e2e fixtures and a stub-LLM real
Bun pair. Left for the ES-module half: a second layout variant, one parse
mode per run (R10), and the entry-context call the emit writes
(`init(module, require, …)`) which still assumes CommonJS.

### P10 — Name profile: what a minifier-made name looks like (stage 9) — EXISTS (landed #75; chosen by the toolchain since 2026-10-04)

**Question:** does this name look like something a minifier invented? (Who
gets counted as minted, which answers are junk, which LLM answers borrow a
minified name.)

**Today:** `rename::floor::is_bun_token` (`floor.rs:123`) answers it with
Bun's shape for every input. Its callers: the minted census
(`naming/passes/census.rs`), the below-floor rule (`rename/validated.rs:724`,
`rename/votes.rs:98`, `naming/passes/sweep.rs:584`), the family permute
(`naming/passes/family_permute.rs:59`), the answer checks
(`is_minified_echo`, `is_borrowable_stem` / `MinifiedStems`,
`is_sweep_answer_acceptable`), and the reconcile's half-mint rule
(`is_half_mint_head`).

**In flight:** `feat/minifier-name-profiles` is turning these shape rules
into per-minifier profiles chosen once from detection. On 2026-10-03 that
branch had no commits yet, so this section describes the interface this
spec needs; reconcile it with the branch when it lands.

**Landed on the branch (2026-10-03, finding #75):** `rename::name_profile`
— `NameProfile` (bun, esbuild, terser, swc, none) chosen once by
`select_name_profile` and carried to every caller above; every
`rename::floor` predicate takes it (`is_bun_token` is now the Bun
profile's private rule behind `is_minifier_token`). It follows the
guarantees below: Bun is byte-for-byte today's (frozen battery + stub-LLM
pair), the minifier DETECTION verdict is not read at all (P1), an unsure
input stays on Bun, and `--minifier none` counts nothing as minted. Only
the flags and a definitive bun/esbuild bundler verdict select another
profile. The esbuild/terser/swc profile does NOT yet cover the digit-free
3-letter names below — none of the three measured minifiers' digit-free
names can be told from words by shape alone. Paths that ask no shape
question are handed no profile (finding #81, review R24): the finish's
relink and its bundle carry (a replay of renames already decided) use
`RenameState::for_replay`; asking that state for a profile panics.

**Interface this spec needs:** a `NameProfile` value, chosen once from the
minifier verdict and passed to every caller above, answering at least:

- `is_minted(name)` — today's `is_bun_token`;
- `is_borrowable_stem(name)` — which minted names an LLM answer can wear as
  a word;
- the single-letter and convention exceptions, which are policy (Andrew's
  2026-09-30 decisions) and should stay shared, not per-profile.

**Must guarantee:** the Bun profile is byte-for-byte today's behaviour (prove
with a warm neutrality run on a Claude Code pair); an unsure verdict (P1)
picks the Bun profile, not a guessed one; a "not minified" verdict picks a
profile that counts nothing as minted.

**What terser/esbuild profiles need that Bun's lacks:** digit-free 3-letter
names drawn from the minifier's letter-frequency alphabet (I22).

**Tests:** `floor_test.rs`-style tables per profile, built from a real
build's binding names (every name the minifier minted must be flagged; the
names it kept must not).

### P11 — Module-layout record ("fossils") (stages 8, 10) — PARTIAL

**Question:** does the bundle still show which original source file each
group of statements came from, and how do the files import each other?

**Interface today:** `UnpackAdapter::provides_module_fossils()` OFFERS the
markers (`place::method::MarkerOffer`); the grammar is
`twins::fossil::extract_fossil_modules` — each original source file ends
with its lazy-init definition, whose leading init calls are its imports.

**Which split method a bundle gets (finding #87, 2026-10-05):** from what
the bundle CONTAINS, never from which bundler wrote it — the owner is
`place::method::choose_split_method`. Bun and esbuild write a lazy-init
module only for an ES module that is `require()`d or loaded by `import()`;
the grammar is true only when EVERY source file loaded that way (Claude
Code's shape). The split measures the markers' coverage
(`twins::fossil::marker_coverage`): the share of the app code's bytes (module
factories set aside as vendor code) that sits inside a recorded module in a
shape a lazy module's top level can hold — hoisted functions and classes,
`var`s with inert initializers, the export registrar call, the init itself.
Everything else inside a module is eager code glued in; everything after
the last init is the entry tail. It is a LOWER bound on the eager code (an
eager module of functions and constants looks lazy).

- coverage ≥ 99% (`MARKER_COVERAGE_THRESHOLD`) → the module markers;
- otherwise the prior's layout (with a prior) or the fresh grouping, for
  the WHOLE bundle — with each lazy module's end kept as a file boundary
  and a lazy module under the file line cap never cut inside
  (`assign_clustered`'s `module_ends`): a lazy module shares no file with
  the code around it, which also keeps the CommonJS module that
  `require()`s it out of a load-time cycle.

Measured on the text a run splits: Claude Code's eight eval versions
99.900%–99.963% (2.1.197 lowest; uncovered = the 4–8 statement entry
tail), so 99% leaves 10× headroom; the fixtures with one planted lazy
module 17–45%, `bun-mixed` 44%, `bun-plain` 0, `bun-lazy` 99.6%. Why not
markers for the covered part plus the fresh grouping for the rest: the
covered part is only an upper bound, so its file boundaries cannot be
trusted, and on Bun's output the eager code all follows the lazy modules,
so the boundaries the markers do prove are exactly the module ends the
fresh grouping now keeps. The choice, coverage and reason go to the run log
(`Split method: …`) and `--stats-json` (`splitMethod`); `humanify detect
--split-method <bundle>` prints them for an input.

**A plugin must provide:** whether its bundles carry this record, and the
init shapes (the grammar covers Bun's and esbuild's, raw and formatted).

**Where Bun is wired in:** the grammar is one shared shape list rather than
per-plugin; matching reads it on every input (I30, harmless by shape).

**Fallback:** markers not offered, absent or under the threshold → the
fresh grouping (or the prior's layout), as above. The fresh grouping's size
targets scale to the app (finding B1 of the same scan:
`place::assign::cluster::scaled_cluster_config` — files linear in the app's
lines at Claude Code 2.1.89's ~259 lines per file, top-folder sizes with
the square root of the file count; exactly the old constants at Claude
Code's size, 440,850 lines).

**Tests:** `place/assign/fossil`, `place/method` and `twins` cases; the
e2e fixtures `bun-plain` (no markers), `bun-mixed` (2 of 6 modules lazy),
`bun-lazy` (all lazy — the one real build on the marker method), each
holding its `splitMethod` and which module lands in which file
(`expect.apart`).

### P12 — Load-order helper shapes (stage 11) — EXISTS (2026-10-04)

**Question:** which helper calls are safe to move past (they do nothing
until called later) when the split reorders statements?

**Interface today:** `emit::load_order::bundle_load_order_facts` admits
two helpers: the lazy-init helper (`identify_bun_lazy_init`, Bun's text
only — I27) and the export registrar (`registrar_name`, by shape — works for
Bun and esbuild, I28).

**A plugin must provide:** its lazy-init helper's shape. **Fallback:** not
recognised → calls are treated as having effects (safe, fewer moves).

**Tests:** `load_order` unit cases with the plugin's helper texts.

**One lazy-init owner (2026-10-04, finding #79, toolchain review R12):**
the load order no longer reads Bun's TEXT (`identify_bun_lazy_init`,
deleted). It asks `ModuleWrapperGrammar::lazy_init_helpers`, the same
shape recogniser the fossil layout (P11) uses —
`twins::fossil::lazy_init_helper_names`: Bun's and esbuild's forms, raw and
formatted. Before switching, both recognisers were run side by side on
every split of the stub-LLM real Bun pair (2.1.118 fresh, 2.1.119 with that
prior): each named the same single binding. On the esbuild fixtures the
text found nothing and the shape finds `__esm`, so I27 is closed: the load
order now knows esbuild's init calls do nothing at load time (both
fixtures' trees still byte-identical to main). The text recogniser's two
quirks went with it: in a comma declaration it named the FIRST declarator,
not the helper, and it accepted a 3-parameter non-thunk form the shape
does not.

### P13 — Which unpacked file is the app (stages 10-12) — MISSING

**Question:** of the files the unpacker wrote, which one is the bundle's own
code — the file the split cuts up and the next release's prior is made from?

**Today:** the last file processed (I26). True for Bun and esbuild
(`runtime.js` is written last), arbitrary for webcrack.

**What a plugin piece would look like:** the unpack result names its app
file (or says "several modules, no single app file", in which case the split
declines with a clear message instead of running on one module).

**Slot (2026-10-04):** `toolchain::AppFile` — today's only rule,
`LastProcessed`, is what the naming loop asks before replacing the file it
hands the split. A second rule lands there.

### P14 — Per-bundler tuning (stage 9) — PARTIAL

Two knobs keyed on the bundler: the module-level naming group size (esbuild
15, else 10 — selected, I23) and the module lane width (a table that nothing
reads, I24). A plugin may set its own values; the defaults apply otherwise.
The dead lane table should be deleted or wired in. **DONE 2026-10-04:** the
group size is `toolchain::BundlerTuning::module_group_size`; the lane
table is deleted. Since finding #81 (review R23) the naming stage is handed
the NUMBER (`NamingConfig::module_group_size`), not the bundler-keyed type.

### P15 — Formatting (stage 6) — not a plugin piece

The formatter undoes idioms every minifier uses and is held to a frozen
byte spec (`test/parity/format-goldens.json`). It deliberately has no
plug point (`pipeline-stages.md`). A minifier with a genuinely new idiom
should extend the formatter for everyone, with a golden.

### P16 — Fixtures and tests — PARTIAL

Every plugin must ship, in `test/e2e/fixtures/<bundler>-bundle*/`:

- **committed real builds** of a small app at two versions
  (`build/v1.0.0/build/index.js`, `build/v1.1.0/build/index.js`), with the
  exact build command and the tool's version pinned in the fixture's README
  (see `esbuild-bundle/README.md` for the form), and `"bundle": true` in
  `fixture.config.json` so the e2e runs with `--split`;
- a **minified** build as well as an unminified one (the shapes differ —
  I2, I27);
- source that exercises: a CommonJS dependency (vendor extraction), an ES
  module required from CommonJS (P8), lazily-loaded ES modules (P11), and a
  deterministic stdout report, so the e2e's boot step proves the split tree
  behaves exactly like the input.

`scripts/e2e.ts` then runs each fixture fresh and with `--prior-version`,
twice each for byte-determinism, and boots the tree. Plus the per-piece
unit tests listed above.

Today (2026-10-05): unminified esbuild iife builds (`esbuild-bundle*`,
`esbuild-kept-factory`), a minified one (`esbuild-minified`), top-level
CJS/ESM builds (`esbuild-cjs`, `esbuild-esm`, `bun-esm-minified` — split
skipped, I25), and four real Bun CJS builds: `bun-bundle`, and `bun-plain`
/ `bun-mixed` / `bun-lazy` (no / some / all modules lazily loaded — which
split method a bundle gets, #87). The eval scores Claude Code only (H2), so
there is no cross-version quality measurement for any other bundler.

## Part 3 — adding a plugin: checklist

For a new **bundler** (say, rollup or webpack 5) today:

1. Detection (P1): add the `BundlerType` value, a signal function in
   `detect/signals.rs` (each signal ranked by `SignalStrength` — the
   verdict is decided by strength, so the list position does not matter),
   its entry in `BUNDLER_DETECTORS`, and the `SELECTABLE_BUNDLERS` entry.
   Write positive tests with minified and unminified real builds, and
   negative tests on Bun/esbuild output.
2. Unpacker (P2): add the `UnpackAdapter` value, its `supports` rule and
   `name`, and its arm in the one dispatch site, `unpack::run_adapter`.
   Write the app's own code last.
3. If the bundle wraps modules in factories (P3): extend
   `identify_cjs_factory` / `factory_arg_function`, and check what the
   change does to every OTHER bundler's input (they run everywhere).
4. If it reuses the vendor record (P5): return its stamp from
   `UnpackAdapter::vendor_record_stamp` (the finish then re-links it, and
   the vendor-record library detector is chosen for it automatically).
5. Interop helpers (P8): add an `InteropHelpers` value — its shapes
   (`recognise`), standard names, helper file and module helper — and
   choose it in `resolve_toolchain`; or accept that factories using them
   stay in the app.
6. Never-rename names (P7): add a list only if the always-on rules miss
   them.
7. Lazy init (P12) and module-layout record (P11): add shapes if the bundler
   has them; set `provides_module_fossils`.
8. If its output has no single wrapper function: the split is not available
   until P9 and P13 exist.
9. **Register in the toolchain** (`humanify_core::toolchain`): if a piece
   has a new implementation (a `BundlerTuning`, an `InteropHelpers`, a
   `ModuleWrapperGrammar`…), add the value and choose it in
   `resolve_toolchain` — from the flags or a DEFINITIVE detection, never
   from the minifier detection verdict (P1) — and give it a `name()` so
   `Toolchain::record` writes it into the run's stats. Nothing below the
   toolchain may compare bundler or minifier names.
10. Fixtures (P16), then `npm run check`, then add a row to
    `responsibility.md` for any new owner.

For a new **minifier** (say, terser): P1 (with a "not minified" answer and
honest tiers), P10 (a name profile — after the in-flight branch lands), P7
if its helpers survive, and a minified fixture.

## Part 4 — recommended order for making the seams real

Ordered so each step is small, provable, and unblocks the next. Every step
must leave Claude Code (Bun) output byte-identical: prove it with a **warm**
`experiments/lib/neutrality.sh` run, not the eval (rule 11).

1. **One "what did we detect" value, passed down** — bundler, minifier and
   adapter, chosen once, replacing the string comparisons (`"bun"`,
   `"esbuild"`) in the finish (I14, I21), library detection, the naming
   group size (I23) and the dead lane table (I24). Neutral by construction
   on Bun input. This is also the hook the name-profile branch needs.
   **DONE 2026-10-04** — `toolchain::resolve_toolchain` (finding #76).
2. **Make the minifier verdict trustworthy before anything reads it** (I3-I5):
   add "not minified", stop treating the terser fallback as a verdict, and
   make the name profile fall back to Bun's when unsure. Then land the name
   profile (P10) on top. Neutral on Bun input by the fallback rule.
3. **One adapter dispatch site** (I9). Pure refactor, neutral. **DONE
   2026-10-04** — `unpack::run_adapter` (finding #76).
4. **Pass the unpacker's factory grammar to the naming stage** instead of
   re-detecting on every file (I10). Neutral on Bun input (same grammar,
   same answer); changes behaviour only on other bundlers, which is the
   point. **First half DONE 2026-10-04** (finding #79): every caller asks
   the toolchain's `ModuleWrapperGrammar`. **Second half DONE 2026-10-04**
   (finding #80): the naming stage takes the unpacker's decision — no
   per-file re-detection; a factory kept in the app is app code.
5. **Recognise minified esbuild** (I2), with a committed minified esbuild
   fixture first. The extraction code already handles the shape.
   **Fixture committed 2026-10-04:** `test/e2e/fixtures/esbuild-minified`
   (a declared known gap in `scripts/e2e.ts`: detected unknown →
   passthrough, no `vendor/`; with `--bundler esbuild` the same build
   extracts 26 vendor files and boots — the fix is detection alone).
6. **Per-plugin interop helpers and lazy-init shapes** (I16-I18, I27), with
   a fixture that has CommonJS requiring an ES module. (Lazy-init: one
   recogniser, Bun's and esbuild's shapes, behind `ModuleWrapperGrammar` —
   finding #79.)
7. **The container seam and "which file is the app"** (P9, P13): the large
   refactor that opens ESM-format bundles and a webpack split. Plan it as
   its own piece of work; it touches the split, the emit and the matching
   inventory. (P9's routing half is done — finding #77: every reader asks
   `BundleLayout`; what remains is the second layout. P13 is unchanged:
   one rule, `AppFile::LastProcessed`.) **Fixtures committed 2026-10-04**
   (declared known gaps in `scripts/e2e.ts`, all failing "no recognizable
   bundle wrapper" after unpack + naming): `esbuild-esm`,
   `bun-esm-minified`, and `esbuild-cjs` — esbuild's `--format=cjs` is
   top-level too (Node's module wrapper is implicit), and it is the layout
   real Node CLIs ship (pnpm, wrangler).
8. **Measurement for other bundlers** (H2): an eval pair from a non-Bun app,
   so a change aimed at esbuild or webpack can be judged on something other
   than Claude Code.
