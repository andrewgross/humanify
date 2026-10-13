# Config consolidation: one parse point, one typed config, one file

Status: DESIGN + INVENTORY, 2026-10-13 (main @ `872dff10`). Nothing here is
built. Asked for by Andrew: "we will need several sweeps of our code and evals
to clean up old flags and make sure config is parsed in ONE place and used
downstream." Rule applied throughout: no backwards compatibility — an old
spelling is deleted, not aliased.

## Totals first

- **149 configuration inputs found: 90 in the binary, 59 in the harness**
  (the harness count includes 16 flags parsed TWICE, once in
  `scripts/eval.ts` and again in `run.sh`).
  - binary: 44 pipeline flags, 2 `env-reads` flags, 27 flags on the six
    migration verbs, 14 `--disable`/`--probe` switch names, 2 API-key
    environment variables + the `.env` file;
  - harness: 16 `eval score` flags (+ the same 16 in `run.sh`), 5 `eval
neutrality` flags, 7 `neutrality.sh`, 8 `gate.sh`, 6 `selfhop.sh`, 2
    `boot-gate.sh` environment variables, `.humanify.local.json`, `check
--only`, and the clone census's `--loose` + `CLONE_CENSUS_ROOT`.
  - plus ~60 hard-coded tuning values, already listed in
    [tuning-values.md](./tuning-values.md); only the ones a user should turn
    become config (§3).
- **Dead or no-op: 11.** 7 dead (the `match` verb's inert `--sequential`; the
  eval's inert `--heap-mb` and the `NODE_OPTIONS` it sets on a Rust binary;
  the `libdetect` verb, no caller; `gate.sh` and `selfhop.sh`, not in the
  eval registry, which CLAUDE.md calls the only supported list; plus
  `BOOT_GATE_SOFT`, set only by a test) and 4 positive spellings that only
  restate the default (`--skip-libraries`, `--naming-floor`,
  `--naming-floor-sweep`, `--reconcile-prior-diff`).
- **Never passed by any harness script, test or launch golden: 11 pipeline
  flags** (`--context-tokens`, `--lane-threshold`, `--max-free-retries`,
  `--max-tokens`, `--module-concurrency`, `--timeout`, `--no-naming-floor`,
  `--no-naming-floor-sweep`, `--no-reconcile-prior-diff`, `--split-ledger`,
  `--relaxed-levers`). Live code, but unexercised: candidates to become
  config-file values, not flags.
- **To rename: 3** (`unpack --index` → `--bundle-order`; eval `--no-vendor` →
  `--no-vendor-score`; pipeline `--profile` → `--perf-trace`, so `--config`
  and "profile" stop competing). **To delete and fold into one setting: 2**
  (`--skip-libraries`/`--no-skip-libraries` → `[library] unnamed`, see
  library-matching-config.md).
- **Two drifts found:** CLAUDE.md says the boot gate pins `haiku-4-5`; the
  code pins `claude-opus-4-1` (`boot-gate.sh:45`, re-pinned 2026-09-28). And
  the model has two defaults: `gpt-4o-mini` for the pipeline, `openai/gpt-oss-20b`
  for the `unpack` verb.
- **Target:** one parse point (`humanify-cli`), producing one typed,
  immutable `Config` passed down; nothing below the CLI reads argv,
  environment or files for configuration; the whole config recorded in every
  run manifest; a guard test failing any config read outside the owner.
- **Sweep plan: 8 small steps** (§5), each gated by `npm run check`, with
  WARM neutrality wherever pipeline bytes could move, and the launch goldens
  updated deliberately for harness changes.

## 1. How configuration flows today

```
argv ─► commander.rs port (pipeline) ──► JSON object of options
                                          │  unified.rs CommandOptions::from: v.str("…"), v.bool("skipLibraries")
                                          ▼
         .env + HUMANIFY_API_KEY/OPENAI_API_KEY ─► settings.rs resolve_settings ─► Settings (frozen)
                                          │
         --disable/--probe ─► kill_switches.rs SwitchState::configure (separately)
         --sequential/--relaxed-levers ─► unified.rs Levers (separately)
         --bundler/--minifier ─► toolchain (separately, after detection)
         paths (--prior-version, --split-ledger, --stats-json, …) ─► read where used in unified.rs
argv ─► clap (six migration verbs) ─► each verb's own args (own defaults)
```

What is already right: `env.rs` is the ONE environment reader (clippy
`disallowed_methods` bans `std::env::var` everywhere else, and
`kill_switches_test.rs` walks the sources for it); `Settings` has no `&mut`
API; the toolchain is resolved once. What is not: options reach downstream
code through four separate resolvers (settings, switches, levers, toolchain)
plus raw `CommandOptions` fields read throughout `unified.rs`; numbers are
parsed as JavaScript `parseInt` (`12abc` → 12) for TS-harness parity; and the
same knob can have different defaults per verb.

## 2. Inventory

### 2.1 The pipeline's argument parser (`commander.rs`)

`commander.rs` is a port of the npm `commander` 13 package, kept because the
TS harness used to spawn the TS pipeline with commander's grammar ("the
grammar IS contract surface — including its accidents"): a required option
swallows the next argument even if it starts with `-`; repeated options are
last-wins; `-vv` counts; `-c5` attaches; usage errors exit 1 and print the
help.

**Is it still needed?** No, on the no-backwards-compatibility rule. The TS
pipeline is gone (cutover 2026-09-26). The only spawner is the harness, and
its launch goldens (`run-launch*.golden.txt`) use plain forms only (`-c 472`,
`-vv`, `--flag value`). **Proposal:** replace the port with clap, the same
library the verbs use — one parser for the whole binary. Keep two things on
purpose: usage errors exit 1 (the pipeline keeps exit 2 out of its contract;
`try_parse` + exit 1), and `-vv` counting. Re-record the help goldens
(`test/golden/help/`) deliberately. ~800 lines and `surface_test.rs`'s
accident tests go.

### 2.2 Pipeline flags (`surface.rs`, 44 + the input path)

"Users" = passed by a harness script, a test, or a launch golden. Proposal
column: **flag** stays a flag; **config** moves to the config file (still
overridable by a flag only if a harness needs it); **delete**.

| flag                                         | default                                   | consumed in                             | users                                                  | proposal                                                |
| -------------------------------------------- | ----------------------------------------- | --------------------------------------- | ------------------------------------------------------ | ------------------------------------------------------- |
| `<input>`                                    | —                                         | unified.rs                              | all                                                    | flag                                                    |
| `--endpoint`                                 | `https://api.openai.com/v1`               | settings.rs                             | eval, neutrality, tests                                | flag + `[llm] endpoint`                                 |
| `--api-key`                                  | env `HUMANIFY_API_KEY` > `OPENAI_API_KEY` | settings.rs                             | eval, tests                                            | flag; env stays (secrets do not go in a file)           |
| `-m, --model`                                | `gpt-4o-mini`                             | settings.rs                             | eval                                                   | flag + `[llm] model`; one default for every verb        |
| `-o, --output-dir`                           | `output`                                  | unified.rs                              | all                                                    | flag                                                    |
| `-v, --verbose`                              | 0                                         | log.rs                                  | eval (`-vv`)                                           | flag                                                    |
| `-c, --concurrency`                          | `DEFAULT_CONCURRENCY`                     | settings.rs                             | eval (`-c 472`)                                        | flag + `[llm] concurrency`                              |
| `--module-concurrency`                       | derived from `-c`, bundler-aware          | util.rs                                 | none                                                   | config                                                  |
| `--max-tokens`                               | unset                                     | unified.rs, split_stage.rs              | none                                                   | config `[llm]`                                          |
| `--context-tokens`                           | 32768 (in code)                           | split_stage.rs, place/assign/namer.rs   | none                                                   | config `[llm]`                                          |
| `--retries`                                  | 3                                         | settings.rs                             | tests                                                  | config `[llm]`                                          |
| `--timeout`                                  | `DEFAULT_LLM_TIMEOUT_MS`                  | settings.rs                             | none                                                   | config `[llm]`                                          |
| `--llm-cache`                                | unset                                     | settings.rs                             | eval self-hop, neutrality                              | flag                                                    |
| `--reasoning-effort`                         | server default                            | settings.rs                             | eval                                                   | flag + `[llm]`                                          |
| `--skip-libraries` / `--no-skip-libraries`   | skip                                      | settings.rs:174, unified.rs:655, era.rs | one test                                               | **delete** → `[library] unnamed` + `--library-matching` |
| `--log-file`                                 | unset                                     | log.rs                                  | eval                                                   | flag                                                    |
| `--diagnostics`                              | unset                                     | unified.rs                              | harness (2)                                            | flag                                                    |
| `--stats-json`                               | unset                                     | unified.rs                              | eval                                                   | flag                                                    |
| `--dump-artifacts`                           | unset                                     | many dump owners                        | tests                                                  | flag                                                    |
| `--dump-asks`                                | unset                                     | unified.rs                              | harness (2)                                            | flag                                                    |
| `--bundler`, `--minifier`                    | detected                                  | toolchain                               | tests                                                  | flag                                                    |
| `--batch-size`                               | 25                                        | settings.rs                             | harness (5)                                            | flag + `[naming]`                                       |
| `--max-retries`                              | 2                                         | naming/waves                            | surface test only                                      | config `[naming]`                                       |
| `--rename-retries`                           | 2 (`REASK_LIMIT`)                         | naming/reask                            | tests                                                  | config `[naming]`                                       |
| `--max-free-retries`                         | 100                                       | naming/waves                            | none                                                   | config `[naming]`                                       |
| `--lane-threshold`                           | 25                                        | naming/waves                            | none                                                   | config `[naming]`                                       |
| `--split`                                    | off                                       | unified.rs                              | eval                                                   | flag                                                    |
| `--split-pure`                               | off                                       | emit, finish                            | tests                                                  | flag                                                    |
| `--split-ledger`                             | auto next to prior                        | split_stage.rs                          | none (auto-discovered)                                 | flag (it is an input path)                              |
| `--prior-version`                            | unset                                     | unified.rs                              | eval                                                   | flag                                                    |
| `--reconcile-prior-diff` / `--no-…`          | on with a prior                           | settings.rs                             | none                                                   | config `[levers]`; delete both flags                    |
| `--naming-floor` / `--no-…`                  | on                                        | settings.rs                             | none                                                   | config `[levers]`; delete both flags                    |
| `--naming-floor-sweep` / `--no-…`            | on                                        | settings.rs                             | none                                                   | config `[levers]`; delete both flags                    |
| `--rename-ledger`                            | unset                                     | writers.rs                              | tests                                                  | flag                                                    |
| `--sequential`                               | off (relaxed schedule)                    | unified.rs → `Levers`                   | eval (`--pipeline-arg --sequential`), 11 harness files | flag + `[schedule]`                                     |
| `--relaxed-levers` (hidden)                  | all                                       | unified.rs → `fast.rs`                  | surface test only                                      | config `[schedule] relaxed_levers`                      |
| `--disable <switches>`                       | none                                      | kill_switches.rs                        | neutrality/eval via `--pipeline-arg`, tests            | flag (ablation) + `[switches] disable`                  |
| `--probe <probes>`                           | none                                      | kill_switches.rs                        | tests                                                  | flag + `[switches] probe`                               |
| `--simulate-llm-latency`                     | unset                                     | unified.rs                              | 1 script                                               | flag                                                    |
| `--profile`                                  | unset                                     | profiler                                | harness (2)                                            | **rename `--perf-trace`**                               |
| new: `--config <file>`                       | compiled-in defaults                      | the parse point                         | —                                                      | flag                                                    |
| new: `--library-matching`, `--library-index` | off                                       | the parse point                         | —                                                      | flag (library-matching-config.md)                       |

The 14 switch names behind `--disable`/`--probe` (`kill_switches.rs`
`Switch::ALL`: family-permute, shingle-probe, content-anchor, anchor-preempt,
anchor-nearident, allsame-vote, empty-decl-hash-guard, registrar-exemption,
emit-align, name-align, vendor-inherit, manifest-prior-order,
post-split-reconcile, fossil-split) are ablation switches with a registry and
an experiment of record each: **keep**, and record the resolved set in the
config record.

### 2.3 Subcommands

| verb           | parser    | flags                                                                                                                                                                            | users                                      | proposal                                                                                                         |
| -------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `env-reads`    | commander | `--markdown`, `-o`                                                                                                                                                               | its own test and help golden               | keep or delete — a user feature with no harness use (open)                                                       |
| `detect`       | clap      | `--profile`, `--toolchain`, `--split-method`                                                                                                                                     | `scripts/e2e.ts`                           | keep; `--profile` → `--perf-trace`                                                                               |
| `unpack`       | clap      | `--prior-version`, `--llm-cache`, `--model` (default `openai/gpt-oss-20b`), `--reasoning-effort` (`low`), `--max-tokens`, `--llm-log`, `--index`, `--carries`, `--webcrack-shim` | `tests/pipeline_stages.rs`                 | keep; `--index` → `--bundle-order`; model/effort read from the same `[llm]` config as the pipeline (one default) |
| `libdetect`    | clap      | `--files`, `--webcrack-shim`                                                                                                                                                     | none found                                 | delete, or fold into `humanify index` as the detector's verdict                                                  |
| `format`       | clap      | `-o`, `--no-transforms`, `--plant`                                                                                                                                               | `experiments/lib/match-truth`              | keep                                                                                                             |
| `format-check` | clap      | `--show` (10), `--plant`                                                                                                                                                         | `scripts/format-golden.sh`                 | keep                                                                                                             |
| `match`        | clap      | `-o`, `--sequential` (**inert**: "never changed the dump"), `--bundler`, `--minifier`, `--work-dir`, `--keep-work-dir`, `--webcrack-shim`                                        | `experiments/lib/match-truth`, a Rust test | keep; **delete `--sequential`**                                                                                  |
| new: `index`   | clap      | library-matching-config.md §4                                                                                                                                                    | —                                          | add                                                                                                              |

### 2.4 Environment and files the binary reads

| input                                | where                          | proposal                                            |
| ------------------------------------ | ------------------------------ | --------------------------------------------------- |
| `HUMANIFY_API_KEY`, `OPENAI_API_KEY` | `env.rs` via `settings.rs`     | keep (secrets); still read only in `env.rs`         |
| `.env` in the working directory      | `env.rs` (`dotenvy`, once)     | keep only for the API key; document that it is read |
| `std::env::args`                     | `env.rs::user_args`            | keep (the parse point's input)                      |
| any other env read in `crates/`      | none (clippy ban + guard test) | keep it that way                                    |

### 2.5 The harness

| input                                                                                                                                                                                                                                                                          | parsed in                                                 | consumed in             | users                     | proposal                                                                                                                                                                                                                       |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------- | ----------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `eval score` flags (16: `--force-mixed`, `--seeded-base`, `--archive-prior`, `--pairs`, `--heap-mb`, `--endpoint`, `--llm-cache`, `--no-layout`, `--no-vendor`, `--no-boot-prompt`, `--no-self-hop`, `--inputs-base`, `--priors-base`, `--workdir`, `--bin`, `--pipeline-arg`) | `scripts/eval.ts` (validated) AND `run.sh` (parsed again) | run.sh, run-pipeline.ts | eval                      | one parse: eval.ts validates and writes ONE run config JSON; run.sh reads that file, not argv. Delete `--heap-mb` (inert). Rename `--no-vendor` → `--no-vendor-score`. Add `--config`, `--library-matching`, `--library-index` |
| `NODE_OPTIONS=--max-old-space-size`                                                                                                                                                                                                                                            | run-pipeline.ts (from `heapMb`)                           | nothing (Rust binary)   | launch goldens            | **delete**; update `run-launch*.golden.txt` deliberately                                                                                                                                                                       |
| `eval neutrality` flags (5) → `neutrality.sh` (7: `--workdir`, `--cache`, `--priors`, `--inputs-base`, `--endpoint`, `--candidate-cmd`, `--baseline-cmd`)                                                                                                                      | eval.ts and neutrality.sh                                 | neutrality.sh           | CLAUDE.md gate            | keep; same one-run-config shape as score                                                                                                                                                                                       |
| `gate.sh` (8), `selfhop.sh` (6)                                                                                                                                                                                                                                                | their own parsers                                         | themselves              | not in the eval registry  | **delete or register** (CLAUDE.md: an instrument not in `scripts/eval.ts` is not supported)                                                                                                                                    |
| `BOOT_GATE_MODEL` (default `claude-opus-4-1`)                                                                                                                                                                                                                                  | `boot-gate.sh:45`                                         | boot prompt             | run.sh, walk.sh           | make it a flag of the boot gate recorded in the run config; **fix CLAUDE.md, which says haiku-4-5**                                                                                                                            |
| `BOOT_GATE_SOFT`                                                                                                                                                                                                                                                               | boot-gate.sh                                              | soft failure            | only `run-launch.test.ts` | delete with its archived callers, or make a flag                                                                                                                                                                               |
| `.humanify.local.json`                                                                                                                                                                                                                                                         | `llm-endpoint.sh`                                         | endpoint                | every instrument          | keep (untracked host config, guarded by `test/no-private-addresses.test.ts`)                                                                                                                                                   |
| `check --only`                                                                                                                                                                                                                                                                 | scripts/check.ts                                          | stage subset            | developers                | keep                                                                                                                                                                                                                           |
| `CLONE_CENSUS_ROOT`, `--loose`                                                                                                                                                                                                                                                 | scripts/clone-census.ts                                   | scan root               | its test                  | env → flag (`--root`)                                                                                                                                                                                                          |
| `pairs.json`, `noise-bands.json`, `self-hop-reference.json`, allowlists                                                                                                                                                                                                        | their readers                                             | scoring                 | eval                      | data, not config: keep                                                                                                                                                                                                         |

The harness already has its own guard: `test/measurement-owners.test.ts`
bans named ambient env vars and `${VAR:-default}` reads in the living shell
scripts (boot-gate.sh exempted by an owner decision of 2026-08-12).

## 3. Target

1. **One parse point.** `humanify-cli` parses argv (clap), the config file
   (`--config <file>`, TOML) and the two API-key env vars, in that order of
   precedence: flag > config file > compiled-in default (env only for the
   key). It validates everything up front: unknown keys, unknown switch
   names and bad numbers fail before any work (no `parseInt` leniency).
2. **One typed, immutable `Config`**, sections: `[llm]`, `[naming]`,
   `[levers]`, `[schedule]`, `[switches]`, `[split]`, `[library]`
   (+ `.lookup`, `.discovery`), `[paths]` (cache root), `[outputs]`
   (diagnostics, stats, dumps). `Settings`, `SwitchState`, `Levers` and the
   raw `CommandOptions` fields collapse into it. The toolchain stays a
   separate value because it is DERIVED from the input by detection, but it
   is built once from `Config` + input and passed down beside it.
3. **Nothing below the CLI reads configuration.** `humanify-core` takes
   `&Config` sections (or plain values) as parameters. It never sees argv,
   the environment or a config file path.
4. **Recorded whole.** Every run writes the resolved config (secrets
   redacted), its sha256 and the source of each value (flag / file /
   default) into the stats file and run manifest. The eval records the
   config sha per label and refuses mixed labels, like base mode.
5. **Guard test.** Extend the existing env guard: a test that walks
   `crates/` and fails any `std::env`, `clap`/argv access, or config-file
   read outside the owner module, and any `Config` field mutation (there is
   no `&mut` API). The harness side keeps `measurement-owners.test.ts` and
   gains: run.sh reads the run config file, never `$@` beyond its path.
6. **Tuning values.** The ~60 rows in tuning-values.md stay compiled-in
   constants unless a user has a reason to turn them. Those that are exposed
   (library thresholds, naming retry budgets, LLM limits) live in ONE place:
   the `Config` defaults. A constant is never both a config value and a
   separate `const` elsewhere.

## 4. What does not change

Output bytes. Every step below is a parsing or plumbing change; the
neutrality runs prove it. The kill-switch registry and its experiment
references stay. Secrets never go in the config file or the run record.

## 5. Sweep plan

Each step is one branch, one merge. Every step: `npm run check` run bare.
Steps marked **N** can move pipeline bytes and also need WARM neutrality
(`experiments/lib/neutrality.sh <base> 2.1.215:2.1.216 --cache
/work/neutrality-cache`, 0 files / 0 lines, baseline leg +0 writes). Steps
marked **G** touch how the harness launches the binary and update
`run-launch*.golden.txt` deliberately, with the diff reviewed in the PR.

1. **Delete the dead** (G). Match `--sequential`; eval `--heap-mb` and
   `NODE_OPTIONS`; the `libdetect` verb (or keep with a caller);
   `gate.sh`/`selfhop.sh` unless registered in `scripts/eval.ts`;
   `BOOT_GATE_SOFT`. Fix CLAUDE.md's boot-gate model line.
2. **Renames** (G). `unpack --index` → `--bundle-order`; eval `--no-vendor` →
   `--no-vendor-score`; `--profile` → `--perf-trace` (pipeline and
   `detect`); `CLONE_CENSUS_ROOT` → `--root`.
3. **clap for the pipeline** (N). Replace `commander.rs` + `surface.rs`
   with clap declarations; exit 1 on usage errors; typed numbers; re-record
   the help goldens. Neutrality because `meta.json`'s `flags` record and the
   stats file are written from the parsed options.
4. **One `Config` type** (N). Fold `Settings`, `SwitchState`, `Levers` and
   `CommandOptions` into it; one default per knob (the `unpack` verb reads
   the same `[llm]` defaults). Write the resolved config into stats and the
   run manifest. Add the guard test (red first: plant a `std::env` read and
   a raw option read in core, watch it fail).
5. **The config file** (N). `--config <file>`, TOML, compiled-in defaults,
   strict keys. Move the 11 unexercised flags and the paired
   `--x`/`--no-x` levers into it and delete those flags. A test proves the
   compiled-in defaults equal today's defaults, value by value.
6. **One library decision** (N). Delete `--skip-libraries` /
   `--no-skip-libraries`; add `--library-matching off|report|names`
   (default off) and `[library] unnamed = "leave"|"model"`. Neutrality on
   the default; an e2e run proves `unnamed = "model"` equals today's
   `--no-skip-libraries` byte for byte.
7. **Harness: one run config** (G). `eval.ts` validates once and writes the
   run config JSON; `run.sh` and `neutrality.sh` read it instead of
   re-parsing argv; pipeline.json records the binary's config sha and the
   library block.
8. **Thresholds into the config, by area** (N each). Library lookup and
   discovery first (they arrive with matching-architecture step 4), then
   any naming/LLM constants a user needs; update tuning-values.md rows to
   point at the config key.

## 6. Open

- Delete `env-reads` (a user feature with no harness use) or keep it?
- TOML or JSON for the config file? (TOML proposed: comments, and the
  harness can still write JSON-equivalent TOML.)
- Whether flags that duplicate a config key (`--batch-size`, `-c`, `--model`)
  stay as flags for the harness's convenience, or the harness writes a
  config file per run instead.
