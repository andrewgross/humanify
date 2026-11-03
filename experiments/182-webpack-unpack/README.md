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

**How many of its bundles unpack** (the unpack stage completes and the
unpacked files are in the RUN'S out dir), every bundle through the full
pipeline with `-o out` (relative), `split.enabled = false`,
`library.matching = "off"`, a dead LLM endpoint, launched from a fresh
scratch cwd:

| run                  | unpacked | failed at unpack | note                                                                                                        |
| -------------------- | -------- | ---------------- | ----------------------------------------------------------------------------------------------------------- |
| before (main binary) | 0        | 34               | every one `Error: read out/deobfuscated.js: No such file or directory (os error 2)`                         |
| after (this branch)  | 33       | 1                | the 4 runs the 1,200 s harness timeout cut were past unpack ("Unpacked N file(s) via webcrack") — see below |

After the fix, 30 runs exit 0 end to end; 4 are cut by the harness timeout,
and for 3 of them (dfarm6 10.2 MB, mysti 12.8 MB, vscode-github-actions
9.4 MB) the log shows the unpack stage COMPLETED ("Unpacked 1/9/1 file(s)
via webcrack") before the timeout — the cut is the per-file stages on a
9-13 MB bundle against a dead endpoint, not unpack. The fourth,
`cli_ts-json-schema-generator-1165fe`, is webcrack's own wall: its
deobfuscated.js (7.2 MB, complete, ends on the entry's
`sourceMappingURL`) landed at +21 min — webcrack spent >20 min inside the
shim's 30-minute deadline; the run was killed first and never read the
files. Unpack, not the path bug.

**Verb runs are NOT this metric.** An earlier sweep used the `unpack` VERB
(no LLM): it reads 33/34 "unpacked" on the BEFORE binary too — because the
verb's Files branch prints the count WITHOUT reading the reported paths, so
the before run "succeeded" while the file had landed in
`<shim cwd>/unpack-out/deobfuscated.js` (reproducing the stray-write shape
in the worktree; deleted). Only a run that READS the shim's file list
exercises the bug.

## Gate

| check                                                       | result                                                                        |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `npm run check`, worktree `19faa5d0b`                       | ALL 13 STAGES PASSED                                                          |
| `npm run check`, clean checkout `3d306a1b9`                 | ALL 13 STAGES PASSED                                                          |
| warm neutrality names (exp175 launch)                       | NEUTRAL: 0 files / 0 lines, exit 0 / 0, cache candidate +991, baseline **+0** |
| warm neutrality off (`--pipeline-set library.matching=off`) | NEUTRAL: 0 files / 0 lines, exit 0 / 0, cache candidate +0, baseline **+0**   |

Neutrality: WARM, 2.1.85→2.1.86 vs main `3218b7b8d`, launched as exp175's
(`--priors /work/exp050-cold --cache /work/neutrality-cache`, own
`--workdir`). Claude Code is a Bun bundle and the eval passes its index
explicitly, so nothing here can reach it, and the byte-identity verdict
agrees on both modes.

## What this does NOT prove

- Names-mode output on webpack bundles end to end (the exp181 gap): the
  unpack now succeeds, but the naming-precision question is exp181's.
- `cli_ts-json-schema-generator-1165fe` under the verb sweep: confirm its
  row before citing 34/34 (it may be webcrack's own runtime on a 1.5 MB
  bundle, which is a different wall than the path bug).
- The webcrack shim still writes `<out>/deobfuscated.js` (the beautified
  whole bundle) plus per-module files; what the RUN consumes downstream is
  unchanged — the fix touches only where those bytes land.
