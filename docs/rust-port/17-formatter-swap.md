<!-- Plan of record for phase 5b (WP5.6a–g). Written 2026-09-25 by a read-only
planning pass over rust-port 730eb99; adopted by the structure owner (00-control §3,
"5b strategy", "5b self-hop gate", "5b control", "beautifier bugs"). Line
references are as of 730eb99 — re-check before relying on one. -->

# WP5.6 / phase 5b: the formatter swap

## Recommendation

Port Babel faithfully (strategy a) and split 5b into two steps:

- **5b-1** has a byte-identical gate. The native formatter must reproduce the TS-beautified text exactly.
- **5b-2** is the one noisy, eval-judged event. It ends the hash exemption.

Strategy (b), oxc codegen plus normalization, would make three changes in one uncontrolled event: new formatting, new transforms, and new hash bytes. It would also make the eval's two "must not move" columns (`novel`, `realLn`) impossible to judge, and every future oxc upgrade would become a formatter change across the whole walk. The reasons are in §2.

---

## 1. What the TS beautify does, and what depends on its exact output

### 1.1 Entry points and settings

- It runs at stage 6: `src/commands/unified.ts:1211` calls `createBabelPlugin`, and `src/unminify.ts:125-128` applies it to each unpacked file (the raw text still has its comments).
- `src/plugins/babel/babel.ts:150-169` calls `transformWithPlugins` (`src/babel-utils.ts:232-261`) with Babel `transform` settings `compact:false, minified:false, comments:false, sourceMaps:false, retainLines:false`.
- Babel merges every plugin's visitor into one traversal, in plugin order. The order is: `convertVoidToUndefined`, `flipComparisonsTheRightWayAround`, `makeNumbersLonger`, the patched beautifier, and then `libraryCarryPlugin.post` (only when the file has banner regions, finding #32).
- Two parse quirks to copy or fork on purpose:
  - `transform()` does not set `sourceType`, so Babel's default `"module"` (strict mode) applies. The pipeline's other parser (`parseSourceAst`) uses `"unambiguous"`.
  - It does not set `configFile:false` / `babelrc:false`.
- The printer is `@babel/generator` 7.29.7, the same version `printer.rs` was ported from. With `comments:false`, `_printNewline` never sees a comment line, so the output has no blank lines except inside template literals. The oracle text confirms this: in `/work/oracle/oracle-f7a707d/dumps/2.1.85-2.1.86/text/fresh.js`, every blank line is inside a template.
- Other Babel habits visible in that file:
  - an object literal with any properties prints one property per line (`(_ = {\n exports: {}\n }).exports`);
  - a single-parameter arrow has no parentheses (`H => H`);
  - string literals are printed from their raw source text;
  - indentation is 2 spaces.

### 1.2 Transform inventory

The last column marks bugs I found while reading the beautifier source (`node_modules/babel-plugin-transform-beautifier/src/index.ts`).

| #   | transform                                                                    | source                            | example                                                                                                 | notes                                                                                                                                                            |
| --- | ---------------------------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `void <number>` becomes `undefined`, with no scope check                     | babel.ts:11-25                    | `void 0` → `undefined`                                                                                  | Wrong if a local binding named `undefined` exists                                                                                                                |
| 2   | Comparison with a literal on the left is flipped                             | babel.ts:27-56                    | `null == x` → `x == null`; `5 < a` → `a > 5`                                                            | `t.isLiteral` also matches template literals, so this reorders functions (#32 depends on that)                                                                   |
| 3   | Exponent literals are written out                                            | babel.ts:58-72                    | `5e3` → `5000`                                                                                          | Tests `raw.includes("e")`, so `0xe1` becomes `225` while `0xff` stays. The new value has no raw text, so it prints through JS `Number::toString`                 |
| 4   | Sequence expression in statement position is split                           | babel.ts:80-128 (the house patch) | `a(), b();` → `a(); b();`; `return a, b;` → `a; return b;`                                              | Patched to skip for-loop init/update/test                                                                                                                        |
| 5   | Multiple declarators are split                                               | beautifier 16-86                  | `var a, b, c = 1, d = 2;` → `var a, b; var c = 1; var d = 2;`                                           | Also moves all but the last `var` out of a for-init, and splits `export let a, b = 1`                                                                            |
| 6   | For-loop body wrapped in a block                                             | 95-99                             | `for (...) x;` → `for (...) { x; }`                                                                     | Only `ForStatement`; for-in, for-of and while are not wrapped (visible in fresh.js)                                                                              |
| 7   | Logical expression statement becomes `if`                                    | 125-146                           | `a && b;` → `if (a) b;`; `a \|\| b;` → `if (!a) b;`                                                     | **Semantic bug**: `a ?? b;` becomes `if (!a) b`. There are 33-35 sites per pair, e.g. `H._zod??(H._zod={})` → `if (!H._zod) { H._zod = {}; }` at fresh.js:19518  |
| 8   | `!0` / `!1` become `true` / `false`; `void x` in statement position is split | 148-200                           | `return void x;` → `x; return;`                                                                         | This `void` rewrite is scope-guarded (`hasBinding('undefined', {noGlobals:true})`)                                                                               |
| 9   | Conditional expression becomes if/else                                       | 202-275                           | `v = c ? a : b;` → `if (c) { v = a; } else { v = b; }`; also applies to `return c?a:b;` and to `c?a:b;` | The assignment case only applies when the left side is an identifier                                                                                             |
| 10  | `if` bodies wrapped in blocks, with three exceptions for the else branch     | 284-348                           | `if (x) a; else b;` → blocks                                                                            | Else is not wrapped when it is a conditional, logical, or `id = c ? :` expression statement (these become `else if` chains)                                      |
| 11  | `.concat` folding                                                            | 350-423                           | `"".concat(i)` → `` `${i}` `` (6 sites per pair; the two-argument forms are left alone)                 | Latent bugs: template `raw` is set to the cooked value (backslashes are lost), and the template+string case appends to `raw` twice (`raw += arg.value` repeated) |

All of these run before the text is re-parsed, so they change the AST that the Rust hashes, not just the whitespace.

### 1.3 Pipeline decisions that depend on beautify's exact output

1. **Statement population.** Transforms 4, 5, 7, 8 and 9 decide how many top-level statements the wrapper has. That count is the unit of:
   - `statementHash`;
   - placement (`place/tiers.rs::hash_tier`);
   - the ledger's `order` / `hashes` / `emitHashes` / `fossilModules`, whose length must equal the statement count (`tiers.rs:201-203`, `align.rs:117-121`);
   - the split emitter, which slices the rendered text by spans;
   - and the eval's `novel` count (below).
2. **Hash content.** The canonical serializer (`hash/serialize.rs`, used by `naming/driver/validate.rs:40-58`) ignores formatting but not transforms: `undefined` vs `void 0`, flipped operands, number values, template vs `.concat`, and block wrapping versus the hash's single-statement unwrapping (R5 cites `structural-hash.ts:770-788`).
3. **Text bytes.**
   - Every span.
   - Every prompt, and therefore every cache key (sha256 of the request).
   - `generated` is fresh text plus rename edits: `render_program_with` (`naming/waves/render.rs:217-229`). The oracle's `generated.js` has exactly as many lines as `fresh.js` (433,126).
   - Emitted files.
   - Line-keyed reconcile diffs.
4. **Code that imitates Babel's printer and assumes the fresh text is `@babel/generator` output.**
   - `naming/waves/generate.rs:1-28` (`TextView::pretty`): the re-indent rule, dropping parentheses around objects, compact spacing rules.
   - `render.rs`: the shorthand-property and export-split printer forms.
   - The graph's compact call-site code (TS `function-graph.ts:225,238`).
5. **Text scans that depend on formatting.** The Bun CJS factory marker scan (`modules.rs::identify_bun_cjs_factory:74`) finds nothing at graph time on the fresh text, because Babel prints the marker as `{\n exports: {}\n }` (lesson 7, `modules.rs:892-897`). A formatter that printed `{exports: {}}` would switch on the factory-body skip, which is a decision change.
6. **Function carry (#32).** Library classification uses the ordinal of functions in the beautify output tree, with raw start offsets. On the Rust side the `LibraryHook` is NOT PORTED (`naming/driver/library.rs:57-63`); the pipeline passes `library: None` (`unified.rs:716-723`).
7. **Rename-invariant check** (`validate.rs`). It compares AST-level signatures and ignores formatting, but only because `generated` is fresh text with renames spliced in. It stays valid under any formatter, provided that stays true.
8. **Consumers of `retainLines`.** The `using` desugar re-prints whole files through the Babel printer port (`finish/using/printer.rs`, finding #30). Its style only matches the rest of the tree if the formatter is Babel's.
9. **The eval harness's own KPIs**:
   - `experiments/034-eval-harness/statements.ts` counts top-level statements with TS `statementHash`;
   - `novel` is the number of new statement hashes, which depends on the statement population;
   - `realLn` is the line count of those novel statements (`text.split("\n")`), which depends on formatting;
   - `kpis.ts:178-205` marks both as "hold" columns, and their noise band is 0.
10. **The `--inject-ts-hashes` statement join** (`inject_ts_statement_hashes`, `placement_dump.rs:120-128`) is keyed by spans of the fresh text. It only works while the Rust fresh text is byte-identical to the TS fresh text.

---

## 2. Strategies

### (a) Faithful port of Babel's transforms and generator

**Starting point already in the repo.** `finish/using/{ast.rs 553, convert.rs 969, printer.rs 1671}` is a Babel-shaped, owned (mutable) AST, a converter from oxc, and a port of generator 7.29.7. It already has Babel's token state machine, parenthesis rules, `jsesc_double`, and a raw-number printing path.

**What is missing:**

- A non-`retainLines` mode: a real `newline()` / `_printNewline` (no blank lines when comments are off), and statement newlines in `printJoin`.
- ESM nodes (import/export declarations and specifiers). The converter currently rejects them.
- Carrying raw start offsets on function nodes (for #32).
- The 12 visitors, run as one merged traversal with Babel's requeue semantics: a replaced or inserted node is visited again.
- JS `Number::toString` for synthesized numbers. `dragonbox_ecma =0.1.12` is already in `Cargo.lock`, pulled in by `oxc_ecmascript`.
- A precomputed answer to "does any enclosing scope declare `undefined`?" (from oxc semantic).

**Effort (estimate):** about 2.0–2.8k new Rust lines on top of about 3.2k reused, roughly 6–9 agent-days. Each gate iteration takes seconds: format four 12 MB texts and fixtures, then run `cmp`.

**Expected output change:**

- 5b-1: zero bytes, and that is gated.
- 5b-2: only strings derived from hash bytes change (§3).
- `novel` / `realLn` should be exactly equal to a TS control scored at the same TS decisions.

### (b) oxc codegen plus a normalization pass

This is not a cheaper version of (a):

- The transforms still have to be ported (R5 defaults every row to "reproduce", because rows 4, 5 and 7–9 change the statement population). They would be written on the oxc arena AST with `oxc_traverse`.
- `oxc_codegen` would be a new pinned dependency. It is not in the lockfile or the cargo registry today.

What it saves is about 0.5–1k printer lines. What it costs:

- **Every byte changes and there is no oracle.** `--inject-ts-hashes` cannot work (it joins by fresh spans), so the formatter change and the end of the hash exemption happen in the same event.
- **The eval's hold columns stop meaning anything for this event.** `realLn` changes whenever the line layout differs, and `novel` changes wherever a transform differs. The one column with a measured band of 0 can no longer judge the change.
- **Code that assumes Babel output must be re-checked or rewritten**: the imitation code in `generate.rs` and `render.rs`, the marker scan from lesson 7, and the `using` desugar re-print (#30 becomes a style clash, and splicing only the transformed blocks becomes mandatory).
- **Upstream codegen changes become our changes.** Every oxc upgrade that touches codegen becomes a formatter change across the whole walk (R7). That conflicts with "the sacred compatibility is the binary with itself" (02 §9).

**Effort:** code M (about 1.2–1.8k lines), but judgment is open-ended. Each iteration is a cold eval of about 81+ minutes, and you would need two repeats to re-measure the noise bands.

**Expected output change:** every line of every file compared with the TS era, all prompts cold, and new hash bytes, all at once.

### Why (a)

The project's goal is formatting that is deterministic and stable across versions. A printer we own and freeze gives that. Matching Babel's bytes only matters for the migration gate, but it is exactly what shrinks the noisy event to hash bytes. After cutover, the owned printer can change on purpose, one measured lever at a time. Examples: splicing the `using` blocks (#30), and fixing the `??` bug.

---

## 3. How the hash exemption ends (5b-2)

The factory structural hash is computed at stage 3 on the raw bundle (`bun-module-classification.ts:433`). It does not depend on the formatter. Only `partitions.json` (the statement hashes) touches beautified text. Dropping `--inject-ts-hashes` changes:

**Vendor side:**

- `lib_<hash8>` fallback names and file names (`modules.rs:276`, `vendor_names.rs:148-158`);
- `stable_stem` runtime identifiers inside vendored bodies, `runtime.js`, and the app code that references them;
- vendor-namer batch keys, so those prompts go cold;
- the manifest's `structuralHash`, and the carry-over join `load_prior_vendor_names` (`vendor_names.rs:497`). Against a TS-written prior manifest it joins nothing.

**Statement side:**

- ledger `hashes`, `emitHashes`, `fossilModules[].hashes` and `stage-hashes.json`;
- the placement hash tier against the prior ledger;
- emission-order alignment (`align.rs`).

**A silent failure to fix first.** The Rust side still writes and accepts `hashVersion: 1` (`emit/stable_split.rs:38`, `tiers.rs:202`, `align.rs:117`). A Rust run given a TS-era ledger will treat the TS hashes as usable and match nothing, with no message. Bump `STATEMENT_HASH_VERSION` to 2 (red-first test: a v1 ledger gives `NoPriorHashes`, logged loudly). Optionally build R6's promised re-derivation of the prior's statement hashes from `humanified.js` (statement count equals `order.len()`).

**Places that sort by hash bytes:** `twins/fossil.rs:368` (`hashes_of.sort()`) and the `BTreeMap` in `vendor_names.rs:500`. Both look insensitive to order in effect; the census work package should confirm.

**Priors:** yes, they must be regenerated, and the eval does that by default. Note that the rebase run itself uses the archive TS-era prior (`run.sh` rebase block, `--prior-version $PRIORS/.../humanified.js`). The rebased base therefore loses layout and vendor carry-over compared with the TS reference. That is harmless for the KPIs, because both the scored run and its base are Rust. The production walk restarts fresh (02 §9).

---

## 4. Gate sequence

### 5b-1: byte-identical gates, all deterministic, no LLM

- **G1 (printer only):** new verb `humanify format <in>`, with the transforms disabled, equals a TS probe of `transformWithPlugins(code, [])` on the four `text/minified.js` files and the fixture corpus.
- **G2 (full formatter):** `format(minified.js) == text/fresh.js` byte for byte, for all 4 oracle pairs (`/work/oracle/oracle-f7a707d/dumps/*/text/`), plus fixtures. Port the beautifier's own `__tests__/__snapshots__` and `src/plugins/babel/babel.test.ts` as red-first Rust tests. Commit small goldens captured from TS; they become the formatter's frozen spec.
- **G3 (#32 carry):** `regions.json.libraryFunctions` equals the TS dump on the mixed-file cases (`/work/twins-posture/cases`). It must be `[]` on the four pairs.
- **G4 (end to end):** `/work/rust-port/gates/m3-gate.sh` with `--beautified-input` removed and `--inject-ts-hashes` still on. All four trees byte-identical to `/work/oracle/work-f7a707d` with 0 files excluded, cache copy writes 0 and manifest unchanged, miss audit equal (#39), boot ×4.
- `npm run check` green.

### 5b-2: the eval-judged event

**Harness change first.** Today `npm run eval -- score` cannot run the binary. It launches `npx tsx src/index.ts` in three places:

- the rebase block (`run.sh`, "rebasing prior");
- `experiments/lib/run-pipeline.ts:185-187`;
- the self-hop block (`run.sh`, after the pair loop).

`neutrality.sh` already has a per-leg command template. Add a `--bin <path>` flag to `scripts/eval.ts` `SCORE_FLAGS`, `run.sh`, and `RunConfig` (a command array). Flags are used rather than environment variables because a guard test forbids ambient env vars. Record the binary's sha256 and the commit it was built from in the run manifest; better still, have `run.sh` build it with `cargo build --release --locked` so the label's commit is the binary's commit.

**Things to accept or fix explicitly:**

- `--heap-mb` / `NODE_OPTIONS` do nothing for the binary.
- The binary accepts `--diagnostics` but does not write it (`unified.rs:517`), so `trail-report.ts` fails without stopping the sweep.
- The matcher preflight tests the TS matcher (via `test/e2e/harness`), not the binary. Label that clearly.

**Control.** `main-2026-09-18` was scored at `1813577`. `src/` has 8 decision-touching commits after the oracle `f7a707d` alone, and more before it (`git log f7a707d..730eb99 -- src/`), for example `fc7ecb2`, which changed prompt text. Rule 10 says the control must be cold together with the candidate. So:

1. Score a cold TS control at HEAD.
2. Score two cold Rust runs: `npm run eval -- score rust-5b-<sha>-a --bin target/release/humanify`, then `-b`.
3. Re-measure the bands from the Rust repeats (M4 asks for re-measured bands). Take the wider of these and `noise-bands.json`.

This needs the endpoint to be up (R14).

**Pass criteria:**

- all 4 pairs exit 0;
- boot OK ×4, both halves (`BOOT_GATE_MODEL` pinned);
- no kill switches active;
- `novel` and `realLn` exactly equal to the TS control (band 0);
- `noise`, `noiseLn`, `reloc`, `relocSt`, `newName`, `mints` (band 11, so "≈0" means inside the band), `treeLn`, `reorderLn`, `vendorLn`, `vendorReal` within the bands;
- concat-equivalence: port the TS `assertConcatEquivalence`, which the Rust currently skips (#41), before this run.

**Self-hop needs a new definition before the run.** The WP5.6 row says "self-hop = 0", but every cold self-hop on record differs: 96 lines in `main-2026-09-18`, and 92, 114 and 180 in `noise-band-r1..r3`. Cold self-hop replays nothing, so LLM variation is free to show. Proposed wording:

- the cold self-hop diff count falls inside the reference range; and
- a warm self-hop, replaying a scratch copy of a cache the self-hop leg itself filled, is byte-identical with 0 writes.

That is determinism with the model held fixed, which is the use rule 10 permits. This is a gate-definition change, so it needs the structure owner's sign-off.

**Optional deterministic step before the cold run.** With re-derived prior hashes, a warm run without inject should equal the M3 tree after substituting TS hash8 → Rust hash8 from the bijection report. The exception is vendor-namer prompts, which miss because their keys change. The miss audit would list those.

---

## 5. Work packages

| WP   | contents                                                                                                                                                                                                                                          | depends on                         | gate                                        |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ------------------------------------------- |
| 5.6a | Move `finish/using/{ast,convert,printer}` into a shared `core::format`. Printer gets a `retain_lines` flag, a real newline mode, and ESM nodes                                                                                                    | M3                                 | G1, and the using-desugar tests stay green  |
| 5.6b | The 12 visitors with Babel requeue semantics; number printing with `dragonbox_ecma` pinned; the `undefined`-binding scope oracle; the module-mode parse                                                                                           | 5.6a                               | G2                                          |
| 5.6c | Raw starts carried on function nodes; ordinal carry; wire the `LibraryHook` (`library.rs:57-63`)                                                                                                                                                  | 5.6b                               | G3                                          |
| 5.6d | Native stage 6 in `NamingRun` (`unified.rs:682-698`), which also lifts the single-file limit; delete `--beautified-input` (`surface.rs:34`, `main.rs:50,433-464`, `stages.rs` NOT_YET, `pipeline_stages.rs:138,187`); the token window from #41   | 5.6b, c                            | G4, which is also the 5b-1 milestone        |
| 5.6e | `hashVersion` 2 with the red-first refusal test; hash-order census; optional re-derivation; delete inject (`unified.rs:866-890`, `unminify.rs:84`, `unpack/gate.rs`, `split_stage.rs:56,201`, `stable_split.rs:179-207`, `placement_dump.rs:146`) | 5.6d                               | unit tests; the optional substitution check |
| 5.6f | Harness `--bin` (eval.ts, run.sh ×3, run-pipeline.ts, manifest fields); decide the self-hop definition; a one-pair smoke run                                                                                                                      | can run in parallel from the start | guard test red-first                        |
| 5.6g | The cold event: TS control plus two Rust runs; judge; write it up; re-point the reference                                                                                                                                                         | 5.6e, f; runs alone                | §4 criteria                                 |

**Risks and mitigations:**

- **Babel traversal-order edge cases break byte parity.** The byte gate on 4 × 12 MB plus fixtures finds them in seconds.
- **Memory and time of an owned AST for a 12 MB bundle.** Measure it. If needed, convert and print each wrapper-body statement separately, in parallel, joining in order (the `core::par` pattern).
- **Carrying the beautifier's bugs (`??`, the `.concat` raw text, the unguarded `void`).** Log them as new findings. Fix them after 5b-1 as separate, attributable changes.
- **Silent ledger mis-join.** The hashVersion bump.
- **A stale reference.** The fresh TS control.
- **Endpoint outage (R14).** Schedule the event around it.
- **Rule 11.** Read deltas only against the bands.

---

## 6. Cutover checklist after 5b

**Can be deleted:**

- `src/plugins/babel/` and the `babel-plugin-transform-beautifier` dependency;
- the Rust-only options and inject paths (listed above);
- the TS-dump comparison modes of `humanify-parity`, and `m3-gate.sh` (archive it);
- then all of `src/` at phase 6.

**Must be kept or moved first, because the harness imports `src/`:**

- `experiments/034-eval-harness/statements.ts` → `src/split/statement-hash.ts`. This is the KPI hash, so it must stay bit-identical.
- `experiments/lib/trees.ts` → `babel-utils`, `wrapper-detection`, `split/layout`, and the `StableSplitLedger` type.
- `experiments/lib/diff.ts` → `rename/diff-reconcile`, `file-utils`.
- `test/e2e/harness` and the fingerprint tests → `fingerprint-index`, `function-graph`, `rename/plugin`, which also power the matcher preflight. Retarget it to a Rust verb or retire it.
- The remaining `ceiling-*` and `size-*` scripts: keep or archive.

Move whatever is kept into a harness-owned directory, with Babel as a harness devDependency.

**`npm run check` changes:**

- `typecheck`, `knip`, `knip:prod`, `unit` and `census:clones` retarget to scripts/experiments/test;
- `e2e` (`src/**/*.e2etest.ts`) is replaced by a Rust end-to-end run of the binary on fixtures with a boot check;
- `fingerprint` retargets;
- `rust:parity` becomes a Rust-vs-Rust differ selftest;
- add `rust:build` (the release binary the eval uses) and `rust:format-golden` (the committed TS-captured goldens, the formatter's frozen spec).

**Docs:** `docs/pipeline-stages.md` stage 6 changes from `createBabelPlugin` to `core::format`; CLAUDE.md eval commands gain `--bin`.

### Critical Files for Implementation

- /Users/andrewgross/Development/humanify/crates/humanify-core/src/finish/using/printer.rs (with ast.rs and convert.rs next to it)
- /Users/andrewgross/Development/humanify/src/plugins/babel/babel.ts (and node_modules/babel-plugin-transform-beautifier/src/index.ts)
- /Users/andrewgross/Development/humanify/crates/humanify-cli/src/unified.rs
- /Users/andrewgross/Development/humanify/experiments/034-eval-harness/run.sh (with experiments/lib/run-pipeline.ts and scripts/eval.ts)
- /Users/andrewgross/Development/humanify/crates/humanify-core/src/emit/stable_split.rs (the hash version, plus place/tiers.rs and emit/align.rs)
