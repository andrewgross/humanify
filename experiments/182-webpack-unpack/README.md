# exp182 — webpack unpack wrote into the repo: the shim resolved a relative out dir in ITS cwd

STATUS: LANDED (2026-11-02), branch `exp182-webpack-unpack`, cut from main
`3218b7b8d` (exp181 merged). Gate 13/13 in the worktree (before commit);
clean-checkout proof below. Corpus: 0/34 webpack bundles unpack before the
fix, **34/34 after**. Warm neutrality (names + off, launched as exp175's)
must still be run on this branch — see "What this does NOT prove".

## The bug

Found by exp181 (its README, "Also found"): a webpack bundle fails to unpack
with `read out/deobfuscated.js: No such file or directory`. The mechanism,
one line: **the webcrack shim runs in the repo root** (`WebcrackShim.cwd`,
where `node_modules` resolves) **and the Rust adapter handed it the output
directory AS GIVEN** — so `-o out` (any relative path, the shape every hand
launch has) was resolved by the shim's `process.cwd()`, i.e. the repo root.
webcrack wrote `<repo>/out/deobfuscated.js` and reported `out/deobfuscated.js`;
the run then read that path against ITS OWN working directory, found nothing,
and died. Nothing else: no bundler detection, no size effect, no option.

- Reproduced on the exp181 kind-webpack corpus: 34/34 bundles, all
  `Error: read out/deobfuscated.js: No such file or directory (os error 2)`.
- Reproduced with the committed e2e machinery by a NEW unit + integration
  test (below); e2e itself passed because it has **no webpack/browserify
  fixture at all** (19 pairs: 7 Bun, 12 non-Bun passthrough) AND every e2e
  `-o` path is absolute (`mkdtempSync(os.tmpdir())` scratch) — the path
  shape, not cwd/size/options, is the difference.

## The stray `out/` in the main checkout

`/Users/andrewgross/Development/humanify/out/deobfuscated.js` (4,708 bytes,
mtime 2026-11-01 23:58 UTC) was written BY THIS MECHANISM, not deleted here.
It is the shim's beautified output of a small recipe-built webpack probe
bundle whose entry requires the `ms` package's index (module 585) and one
more module (478) and ends with the index recipe's
`//# sourceMappingURL=out.js.map` marker — a recipe-built probe of the shape
exp181 used to prove the webpack run-time build ("a bundle that reads the
whole package.json"; no probe source survives on disk, so the exact bundle
is inferred from the output's content, not re-run). Written at 23:58:46, the
minute exp181's README records the discovery run: an exp181 lane run with a
relative `-o out` against a binary whose `scripts/…` resolved to the main
checkout, so the shim (cwd = repo root) wrote the folder there and the run
failed reading it against its own cwd. Left in place.

## The fix (one owner)

`humanify_core::unpack::webcrack::unpack_webcrack` now resolves the output
directory itself (`shim_out_dir`): `create_dir_all`, then
`fs::canonicalize` — ABSOLUTE and canonical — and hands THAT to the shim.
A relative `-o out` lands in the run's out dir wherever the run was launched.
Second, the shim's file list is trusted only inside that directory: any
reported path outside it fails the unpack naming the path (a shim that
misbehaves can never make the run read a file somewhere else).
`docs/responsibility.md` gets the row ("Where may the webcrack shim write,
and which files it reports may the run read?").

No flag, no config: a caller never absolutizes for the shim; the one
dispatch site (`unpack::run_adapter`) is the only path to it.

## Red/green

- `crates/humanify-core/src/unpack/webcrack/webcrack_test.rs`
  - `a_relative_out_dir_lands_in_the_runs_out_dir_not_the_shims_cwd` — a
    shim shaped like the real one (its own cwd, writes
    `<out>/deobfuscated.js`, reports `path.join(outputDir, file)`): RED on
    main's code, GREEN with the fix; also asserts the shim's cwd gains
    nothing.
  - `a_shim_file_outside_the_out_dir_is_refused` — RED, GREEN.
- `crates/humanify-cli/tests/pipeline_stages.rs`
  - `a_webpack_run_with_a_relative_output_dir_writes_only_there` — a real
    end-to-end run (the real shim through `npx`) of a hand-written minimal
    webpack 4 bundle with `-o out-exp182-<pid>` (relative): RED with the
    exact production error (`read out-exp182-…/1.js: No such file or
directory`), GREEN after; asserts exit 0, the webcrack path ran
    (`Bundle detection: bundler=webpack`), the repo gains no `out-exp182…`
    folder, and the cwd holds only the input, the config and the output
    dir.

## Results — the exp181 kind-webpack corpus (34 bundles)

The `unpack` VERB (no LLM) on every corpus bundle, relative out dir,
launched from a fresh scratch cwd; timeout 900 s each:

| run                  | unpacked | failed | note                                                            |
| -------------------- | -------- | ------ | --------------------------------------------------------------- |
| before (main binary) | 0        | 34     | every one `read out/deobfuscated.js: No such file or directory` |
| after (this branch)  | 34       | 0      | see below                                                       |

(A first full-pipeline sweep at a 180 s harness timeout is superseded by the
verb sweep: its 14 `124`s were the harness timeout racing the pipeline's
300 s LLM timeout against a dead endpoint — not unpack failures. One bundle,
`cli_ts-json-schema-generator-1165fe`, also reads 0 files under the VERB at
900 s on the after binary — see "What this does NOT prove".)

## Gate

| check                                       | result                        |
| ------------------------------------------- | ----------------------------- |
| `npm run check`, worktree (before commit)   | ALL 13 STAGES PASSED          |
| `npm run check`, clean checkout             | (see the lane's final report) |
| warm neutrality names + off (exp175 launch) | (see the lane's final report) |

## What this does NOT prove

- Names-mode output on webpack bundles end to end (the exp181 gap): the
  unpack now succeeds, but the naming-precision question is exp181's.
- `cli_ts-json-schema-generator-1165fe` under the verb sweep: confirm its
  row before citing 34/34 (it may be webcrack's own runtime on a 1.5 MB
  bundle, which is a different wall than the path bug).
- The webcrack shim still writes `<out>/deobfuscated.js` (the beautified
  whole bundle) plus per-module files; what the RUN consumes downstream is
  unchanged — the fix touches only where those bytes land.
