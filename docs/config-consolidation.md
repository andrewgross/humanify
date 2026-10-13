# Config consolidation: one parse point, one typed config, one file

Status: DESIGN + INVENTORY, 2026-10-13 (main @ `872dff10`). Sweep steps 1-4
landed on main (`df6ace2a`); **step 5 (the `--config` file) is done** on
branch `cleanup/config-sweep-5-6` — §5 records what changed. Steps 6-8 are
not built. Asked for by Andrew: "we will need several sweeps of our code and evals
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
  `--no-vendor-score`; `detect --profile` → `--perf-trace`; the pipeline's
  `--profile` becomes the config key `[outputs] perf_trace`, so `--config`
  and "profile" stop competing). **To delete and fold into one setting: 2**
  (`--skip-libraries`/`--no-skip-libraries` → `[library] unnamed`, see
  library-matching-config.md).
- **Two drifts found:** CLAUDE.md says the boot gate pins `haiku-4-5`; the
  code pins `claude-opus-4-1` (`boot-gate.sh:45`, re-pinned 2026-09-28). And
  the model has two defaults: `gpt-4o-mini` for the pipeline, `openai/gpt-oss-20b`
  for the `unpack` verb.
- **Decided 2026-10-13:** TOML config file; only six operational flags
  stay (input, output, model endpoint, `--library-matching`,
  `--library-index`, `--config`); conflicts and bad values are refused
  loudly before any work (§3.1); `env-reads` is deleted.
- **Target:** one parse point (`humanify-cli`), producing one typed,
  immutable `Config` passed down; nothing below the CLI reads argv,
  environment or files for configuration; the whole config recorded in every
  run manifest; a guard test failing any config read outside the owner.
- **Sweep plan: 8 small steps** (§5), each gated by `npm run check`, with
  WARM neutrality wherever pipeline bytes could move, and the launch goldens
  updated deliberately for harness changes.

## 1. How configuration flows today

```
argv ─► clap, surface.rs (pipeline) ───► CommandOptions (typed; was a commander.rs port until step 3)
                                          │
                                          ▼
         .env + HUMANIFY_API_KEY/OPENAI_API_KEY ─► settings.rs resolve_settings ─► Settings (frozen)
                                          │
         --disable/--probe ─► kill_switches.rs SwitchState::configure (separately)
         --sequential/--relaxed-levers ─► unified.rs Levers (separately)
         --bundler/--minifier ─► toolchain (separately, after detection)
         paths (--prior-version, --split-ledger, --stats-json, …) ─► read where used in unified.rs
argv ─► clap (the stage verbs, main.rs) ─► each verb's own args (own defaults)
```

What is already right: `env.rs` is the ONE environment reader (clippy
`disallowed_methods` bans `std::env::var` everywhere else, and
`kill_switches_test.rs` walks the sources for it); `Settings` has no `&mut`
API; the toolchain is resolved once. What is not: options reach downstream
code through four separate resolvers (settings, switches, levers, toolchain)
plus raw `CommandOptions` fields read throughout `unified.rs`; and the same
knob can have different defaults per verb. (Numbers were parsed as
JavaScript `parseInt`, `12abc` → 12, until step 3 typed them.)

## 2. Inventory

### 2.1 The pipeline's argument parser (`commander.rs` — DELETED in step 3)

**Done (step 3, 2026-10-13):** `commander.rs` (839 lines), its accident
tests, the `env-reads` command and `util::parse_number` are deleted; the
pipeline is declared for clap in `surface.rs` (`CommandOptions` derives
`clap::Args`). Usage errors still exit 1. Every launch the harness makes
parses to the settings the port gave it — `launch_parity_test.rs` against a
fixture captured from the port at `587e211f` (53 command lines: both launch
goldens, e2e.ts, ask-trace.ts, run.sh `--pipeline-arg`, neutrality.sh, the
crate's integration tests, one row per flag). The accidents, old → new:

| accident (commander port)                                                    | now                                                                                |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| a value option swallowed the next flag (`--model --split` → model "--split") | refused: `a value is required for '--model <model>'`                               |
| a negative number passed as a value (`--retries -1` → -1)                    | refused: `unexpected argument '-1'`                                                |
| repeated options last-wins (`-c 4 -c 8` → 8)                                 | refused: `the argument '--concurrency <n>' cannot be used multiple times`          |
| attached short values and combos (`-c5`, `-ofoo`, `-vc 5`)                   | refused, naming the argument; only `-vv` (the harness's form) stays                |
| numbers were `parseInt` (`-c 12abc` → 12, `--timeout 1e3` → 1)               | typed: non-negative integers, else `invalid value '12abc' for '--concurrency <n>'` |
| `--skip-libraries` + `--no-skip-libraries` together: last wins               | refused as a conflict; each alone keeps its effect until step 6                    |
| (both spellings set false — finding #19)                                     | already fixed by two declarations; stays fixed                                     |
| unknown option: `unknown option '--x'` + the whole help                      | `unexpected argument '--x' found` + usage line                                     |
| `-V` printed the bare version                                                | prints `humanify <version>`                                                        |

The original analysis:

`commander.rs` was a port of the npm `commander` 13 package, kept because the
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

"Users" = passed by a harness script, a test, or a launch golden. Andrew's
rule (2026-10-13): **only a few operational flags stay** — the input, the
output directory, the model endpoint (`--endpoint`, `--model`),
`--library-matching`, `--library-index` and `--config`. Everything else is
**config-file only**; its flag is deleted. The harness writes one TOML file
per run instead (§5 step 7).

| flag                                         | default                                   | consumed in                             | users                                                  | proposal                                                                    |
| -------------------------------------------- | ----------------------------------------- | --------------------------------------- | ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `<input>`                                    | —                                         | unified.rs                              | all                                                    | **flag** (positional)                                                       |
| `--endpoint`                                 | `https://api.openai.com/v1`               | settings.rs                             | eval, neutrality, tests                                | **flag**                                                                    |
| `--api-key`                                  | env `HUMANIFY_API_KEY` > `OPENAI_API_KEY` | settings.rs                             | eval, tests                                            | **delete**; the key comes from `HUMANIFY_API_KEY` only (§3.3)               |
| `-m, --model`                                | `gpt-4o-mini`                             | settings.rs                             | eval                                                   | **flag** (part of "model endpoint"); one default for every verb             |
| `-o, --output-dir`                           | `output`                                  | unified.rs                              | all                                                    | **flag**                                                                    |
| `-v, --verbose`                              | 0                                         | log.rs                                  | eval (`-vv`)                                           | config `[outputs] verbosity`                                                |
| `-c, --concurrency`                          | `DEFAULT_CONCURRENCY`                     | settings.rs                             | eval (`-c 472`)                                        | config `[llm] concurrency`                                                  |
| `--module-concurrency`                       | derived from `-c`, bundler-aware          | util.rs                                 | none                                                   | config `[llm]`                                                              |
| `--max-tokens`                               | unset                                     | unified.rs, split_stage.rs              | none                                                   | config `[llm]`                                                              |
| `--context-tokens`                           | 32768 (in code)                           | split_stage.rs, place/assign/namer.rs   | none                                                   | config `[llm]`                                                              |
| `--retries`                                  | 3                                         | settings.rs                             | tests                                                  | config `[llm]`                                                              |
| `--timeout`                                  | `DEFAULT_LLM_TIMEOUT_MS`                  | settings.rs                             | none                                                   | config `[llm]`                                                              |
| `--llm-cache`                                | unset                                     | settings.rs                             | eval self-hop, neutrality                              | config `[llm] cache_dir`                                                    |
| `--reasoning-effort`                         | server default                            | settings.rs                             | eval                                                   | config `[llm]`                                                              |
| `--skip-libraries` / `--no-skip-libraries`   | skip                                      | settings.rs:174, unified.rs:655, era.rs | one test                                               | **delete** → `[library] unnamed` + `--library-matching`                     |
| `--log-file`                                 | unset                                     | log.rs                                  | eval                                                   | config `[outputs]`                                                          |
| `--diagnostics`                              | unset                                     | unified.rs                              | harness (2)                                            | config `[outputs]`                                                          |
| `--stats-json`                               | unset                                     | unified.rs                              | eval                                                   | config `[outputs]`                                                          |
| `--dump-artifacts`                           | unset                                     | many dump owners                        | tests                                                  | config `[outputs]`                                                          |
| `--dump-asks`                                | unset                                     | unified.rs                              | harness (2)                                            | config `[outputs]`                                                          |
| `--bundler`, `--minifier`                    | detected                                  | toolchain                               | tests                                                  | config `[toolchain]` (force detection's verdict)                            |
| `--batch-size`                               | 25                                        | settings.rs                             | harness (5)                                            | config `[naming]`                                                           |
| `--max-retries`                              | 2                                         | naming/waves                            | surface test only                                      | config `[naming]`                                                           |
| `--rename-retries`                           | 2 (`REASK_LIMIT`)                         | naming/reask                            | tests                                                  | config `[naming]`                                                           |
| `--max-free-retries`                         | 100                                       | naming/waves                            | none                                                   | config `[naming]`                                                           |
| `--lane-threshold`                           | 25                                        | naming/waves                            | none                                                   | config `[naming]`                                                           |
| `--split`                                    | off                                       | unified.rs                              | eval                                                   | config `[split] enabled`                                                    |
| `--split-pure`                               | off                                       | emit, finish                            | tests                                                  | config `[split] pure`                                                       |
| `--split-ledger`                             | auto next to prior                        | split_stage.rs                          | none (auto-discovered)                                 | config `[split] ledger` (an input path; auto-discovered by default)         |
| `--prior-version`                            | unset                                     | unified.rs                              | eval                                                   | config `[inputs] prior_version` (open: Andrew's "input" may include it, §6) |
| `--reconcile-prior-diff` / `--no-…`          | on with a prior                           | settings.rs                             | none                                                   | config `[levers]`; both flags deleted                                       |
| `--naming-floor` / `--no-…`                  | on                                        | settings.rs                             | none                                                   | config `[levers]`; both flags deleted                                       |
| `--naming-floor-sweep` / `--no-…`            | on                                        | settings.rs                             | none                                                   | config `[levers]`; both flags deleted                                       |
| `--rename-ledger`                            | unset                                     | writers.rs                              | tests                                                  | config `[outputs]`                                                          |
| `--sequential`                               | off (relaxed schedule)                    | unified.rs → `Levers`                   | eval (`--pipeline-arg --sequential`), 11 harness files | config `[schedule] sequential`                                              |
| `--relaxed-levers` (hidden)                  | all                                       | unified.rs → `fast.rs`                  | surface test only                                      | config `[schedule] relaxed_levers`                                          |
| `--disable <switches>`                       | none                                      | kill_switches.rs                        | neutrality/eval via `--pipeline-arg`, tests            | config `[switches] disable`                                                 |
| `--probe <probes>`                           | none                                      | kill_switches.rs                        | tests                                                  | config `[switches] probe`                                                   |
| `--simulate-llm-latency`                     | unset                                     | unified.rs                              | 1 script                                               | config `[outputs]`                                                          |
| `--profile`                                  | unset                                     | profiler                                | harness (2)                                            | config `[outputs] perf_trace` (the name clash with "profile" goes away)     |
| new: `--config <file>`                       | compiled-in defaults                      | the parse point                         | —                                                      | **flag**                                                                    |
| new: `--library-matching`, `--library-index` | off                                       | the parse point                         | —                                                      | **flag** (library-matching-config.md)                                       |

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
| `env-reads`    | commander | `--markdown`, `-o`                                                                                                                                                               | its own test and help golden               | **deleted** in step 3 (Andrew, 2026-10-13); its source-walking check folds into the config guard test (§3.5)     |
| `detect`       | clap      | `--profile`, `--toolchain`, `--split-method`                                                                                                                                     | `scripts/e2e.ts`                           | keep; `--profile` → `--perf-trace`                                                                               |
| `unpack`       | clap      | `--prior-version`, `--llm-cache`, `--model` (default `openai/gpt-oss-20b`), `--reasoning-effort` (`low`), `--max-tokens`, `--llm-log`, `--index`, `--carries`, `--webcrack-shim` | `tests/pipeline_stages.rs`                 | keep; `--index` → `--bundle-order`; model/effort read from the same `[llm]` config as the pipeline (one default) |
| `libdetect`    | clap      | `--files`, `--webcrack-shim`                                                                                                                                                     | none found                                 | delete, or fold into `humanify index` as the detector's verdict                                                  |
| `format`       | clap      | `-o`, `--no-transforms`, `--plant`                                                                                                                                               | `experiments/lib/match-truth`              | keep                                                                                                             |
| `format-check` | clap      | `--show` (10), `--plant`                                                                                                                                                         | `scripts/format-golden.sh`                 | keep                                                                                                             |
| `match`        | clap      | `-o`, `--sequential` (**inert**: "never changed the dump"), `--bundler`, `--minifier`, `--work-dir`, `--keep-work-dir`, `--webcrack-shim`                                        | `experiments/lib/match-truth`, a Rust test | keep; **delete `--sequential`**                                                                                  |
| new: `index`   | clap      | library-matching-config.md §4                                                                                                                                                    | —                                          | add                                                                                                              |

### 2.4 Environment and files the binary reads

| input                                | where                          | proposal                                                                                                                                                                                      |
| ------------------------------------ | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `HUMANIFY_API_KEY`, `OPENAI_API_KEY` | `env.rs` via `settings.rs`     | keep `HUMANIFY_API_KEY` only (secrets never go in the config file or a flag that lands in a launch record); delete the `OPENAI_API_KEY` fallback and `--api-key`. Still read only in `env.rs` |
| `.env` in the working directory      | `env.rs` (`dotenvy`, once)     | keep only for the API key; document that it is read                                                                                                                                           |
| `std::env::args`                     | `env.rs::user_args`            | keep (the parse point's input)                                                                                                                                                                |
| any other env read in `crates/`      | none (clippy ban + guard test) | keep it that way                                                                                                                                                                              |

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
   (`--config <file>`, **TOML**, decided 2026-10-13) and the one API-key
   environment variable. A value comes from the flag, else the file, else
   the compiled-in default. **Every check in §3.1 runs here, before
   unpacking starts** — nothing is detected, unpacked or asked of the model
   until the whole configuration has been accepted.
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

### 3.1 Refusals: loud, early, before any work

Andrew's rule: be loud about conflicts and fail early rather than proceed
with a possibly bad config. Each refusal exits non-zero (usage-error code 1)
with one message that names the key, every source involved and the values,
and prints nothing else first.

| case                                                                                                                                                     | behaviour                                                                                             | example message                                                                                            |
| -------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| a flag and the config file set the same key to **different** values                                                                                      | refuse                                                                                                | `config conflict: endpoint = "http://a/v1" (--endpoint) vs "http://b/v1" (config.toml)`                    |
| a flag and the file set the same key to the **same** value                                                                                               | accept; the record says both sources                                                                  | —                                                                                                          |
| unknown key in the file (typo, other tool's key)                                                                                                         | refuse                                                                                                | `config.toml: unknown key naming.batchsize (did you mean naming.batch_size?)`                              |
| wrong type (string for a number, etc.)                                                                                                                   | refuse                                                                                                | `config.toml: naming.batch_size must be an integer, got "25x"`                                             |
| out of range (0 or negative where positive is required, threshold outside 0..1, unknown enum value)                                                      | refuse                                                                                                | `config.toml: library.lookup.cover = 1.5 is outside 0..1`                                                  |
| retired key or flag (`--skip-libraries`, `--api-key`, `--heap-mb`, …)                                                                                    | refuse, naming its replacement                                                                        | `--skip-libraries was retired 2026-10: set [library] unnamed in --config`                                  |
| unknown switch name in `[switches]`                                                                                                                      | refuse, listing the valid set (as `--disable` does today)                                             | —                                                                                                          |
| config file missing or not valid TOML                                                                                                                    | refuse                                                                                                | —                                                                                                          |
| `--library-index` missing, unreadable, or built with other versions (any of: format, unit/literal/module key, serializer presets, known-globals, recipe) | refuse unless `--library-matching off`                                                                | `library index b00fa735…: built with units/2, this binary reads units/3 — rebuild: humanify index build …` |
| `--library-matching names` and the detected recipe has no slice in the index                                                                             | refuse                                                                                                | `library index b00fa735… has no slice for bun@1.4 target=bun cjs minify (has: bun target=node, esbuild)`   |
| `--library-matching report` and the detected recipe has no slice                                                                                         | **loud WARNING**, diagnostics + stats entry `recipeMissing`, run continues with path/banner detection | same text, prefixed `WARNING:`                                                                             |
| `--library-matching` not `off` and no `--library-index`                                                                                                  | refuse                                                                                                | —                                                                                                          |
| `--library-index` given with `--library-matching off`                                                                                                    | refuse (a conflict: the index would be silently ignored)                                              | —                                                                                                          |

**Should `report` also fail on a missing slice? No — in the binary; yes —
in the eval.** `report` is defined as changing no output byte, so a run
that cannot do the lookup has lost nothing but the report, and a user
pointing humanify at an esbuild bundle with a Bun-only index should still
get their output. But an eval label whose library numbers come from a run
with no slice is meaningless, so the eval harness refuses it up front:
`run.sh` runs `humanify index verify <id> --recipe <detected>` per pair
before the first launch and stops the label if any pair has no slice. The
binary warns; the measurement fails.

### 3.2 Order of the checks

argv parsed → retired flags → config file read and parsed → unknown keys →
types → ranges → flag/file conflicts → cross-key rules (library mode vs
index) → library index opened and its versions checked → **then** input
read and detection → recipe slice check (needs detection; still before
unpacking) → work starts. Detection runs before the slice check because the
recipe is a detected fact, and the detection-built-pipeline rule resolves it
exactly once.

### 3.3 Secrets

The API key is the one value read from the environment
(`HUMANIFY_API_KEY`, via `env.rs`). It never appears in the config file,
the argv the harness records, or the run record (redacted).

### 3.4 The record

The resolved config, its sha256 and each value's source (flag / file /
default) are written into the stats file and the run manifest (§3 item 4).

### 3.5 The guard test

One test owns "who reads configuration": it walks `crates/` and fails any
`std::env`, argv or config-file access outside the parse-point module, and
any second TOML/flag parser. It absorbs the `env-reads` command's job
(that command inventoried `process.env` reads in JavaScript; the guard is
what this repo actually needs), and the existing clippy
`disallowed_methods` ban stays as the static layer.

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
   `--no-vendor-score`; `detect --profile` → `--perf-trace`; `CLONE_CENSUS_ROOT` → `--root`.
3. **DONE 2026-10-13 (§2.1). clap for the pipeline** (N). Delete the `env-reads` command and
   `env_reads.rs` with it. Replace `commander.rs` + `surface.rs`
   with clap declarations; exit 1 on usage errors; typed numbers; retired
   flags refused by name (test per retired flag, red first); re-record
   the help goldens. Neutrality because `meta.json`'s `flags` record and the
   stats file are written from the parsed options.
4. **DONE 2026-10-13. One `Config` type** (N). Fold `Settings`, `SwitchState`, `Levers` and
   `CommandOptions` into it; one default per knob (the `unpack` verb reads
   the same `[llm]` defaults). Write the resolved config into stats and the
   run manifest. Add the guard test (red first: plant a `std::env` read and
   a raw option read in core, watch it fail).

   What landed: `humanify_cli::config` owns `Config` — sections `io`,
   `llm`, `matching`, `naming`, `library`, `unpack`, `split`, `switches`,
   `diagnostics`, `perf` (each a future TOML table; `library` is where step
   6 lands). `settings.rs` is deleted; `SwitchState` is a field; the lever
   parser moved out of `humanify-core`; `CommandOptions` holds only "given
   or not" (no clap defaults) and nothing past `Config::build` reads it.
   Every knob is resolved to its EFFECTIVE value (an unset batch size is
   the 25 the processor would use), except `max_tokens`, whose "unset" is
   part of the LLM cache key. Defaults deduplicated: the LLM timeout (two
   crates), the model (`unpack` verb: `openai/gpt-oss-20b` → the
   pipeline's `gpt-4o-mini`; no harness script launches `unpack`, so only
   a manual `unpack --llm-cache` replay of an eval cache must now pass
   `--model openai/gpt-oss-20b --reasoning-effort low`), the reasoning
   effort (`unpack`: `low` → unset, like the pipeline), the retry count
   (the rate limiter's). `--api-key` and the `OPENAI_API_KEY` fallback are
   retired: the key is read once from `HUMANIFY_API_KEY`, and the harness
   exports it. New refusals, all collected and printed together before
   anything is created: zero for a count that must be positive
   (concurrency, module concurrency, timeout, max/context tokens, batch
   size, max retries), a context window the completion reserve fills, an
   empty value for any named option, a non-http(s) endpoint, an unknown
   reasoning effort (now a clap enum), and a missing `--prior-version` /
   `--split-ledger` file (the input was checked only after the log file
   and the cache directory were created). The record (`Config::record`,
   the API key redacted) is the stats file's `config` block; the run
   manifest copies it as `config.resolved`, and the kill-switch warning
   reads it instead of re-parsing the argv. `tests/env_guard.rs` is the
   guard (§3.5). Proof: launch parity — all 53 captured launches, their
   `--api-key` removed, resolve to the captured effective settings (the
   fixture is unchanged); warm neutrality vs `fe008408` on
   2.1.215→2.1.216.

5. **DONE 2026-10-13. The config file** (N, G). `--config <file>`, TOML, compiled-in
   defaults, strict keys. Every flag outside the six operational ones moves
   into it and is deleted. A test proves the compiled-in defaults equal
   today's defaults, value by value. **Refusal tests, each written red
   first** (one per row of §3.1): flag/file conflict with different values
   (refused, message names both sources and values); same value (accepted,
   both sources recorded); unknown key; wrong type; out of range; retired
   key; retired flag; unknown switch name; unreadable/invalid TOML; and a
   test that each refusal happens before the unpack stage writes anything
   (the output directory stays empty).

   What landed: every setting is declared ONCE in the key registry
   (`humanify_cli::config::keys::KEYS`: section, name, type and range,
   default, one-line meaning, and the flag for an operational one); the
   TOML is read only by `config::file` (the env guard's owner for `toml`).
   Flags kept: the input, `-o/--output-dir`, `--prior-version`,
   `--endpoint`, `-m/--model`, `--config`, `-v` (plus help/version, and the
   library spellings until step 6). **33 flags moved** (34 spellings with
   `-c`): each is refused naming its key (`MOVED_FLAGS`,
   `error: unexpected argument '--split' (moved: set split.enabled = true
in --config)`). The operational settings are file keys too
   (`io.output_dir`, `llm.endpoint`, `llm.model`, `matching.prior_version`,
   `diagnostics.verbosity`); a flag and the file that disagree are refused
   naming both (`config conflict: llm.endpoint = "http://a/v1" (--endpoint)
vs "http://b/v1" (cfg.toml)`), agreeing ones are recorded as
   `flag+file`. Refused at the read, all collected and printed with every
   other refusal: not TOML, unreadable, unknown key (with "did you mean"),
   retired key (`llm.api_key`, `perf.profile`, `naming.sequential`,
   `io.input`), wrong type, out of range (counts, `verbosity` 0..=2, u32
   ceilings, enum names, lever names, empty strings, non-http endpoints).
   Messages now name KEYS, not flags (`split.pure requires split.enabled`,
   `switches.disable: unknown disable switch …`); the TS-era invariant
   vectors are checked through a translation of their flag names.
   The stats `config` block gains `configFile: {path, sha256}` and
   `sources` (each non-default value: flag / file / flag+file).
   `humanify.example.toml` is RENDERED from the registry (keys with a flag
   commented out, so a copy never conflicts) and held byte-equal by a test,
   which also proves loading it changes nothing.
   Harness: `experiments/lib/pipeline-config.ts` writes one TOML per
   launch (run.sh's rebase, scored leg and both self-hop legs; walk.sh;
   neutrality.sh; e2e.ts; ask-trace.ts); `eval score`/run.sh
   `--pipeline-arg` is retired for `--pipeline-set <section.key=value>`
   (a key the harness sets itself is refused up front), recorded as
   `pipelineSettings`; each run config carries its `configToml`.
   neutrality.sh and walk.sh ask the binary (`--help`) whether it reads
   `--config`, so a baseline from before this step still gets the same
   settings as flags. Launch goldens re-recorded (the recorder logs each
   launch's file as `humanify-config|` lines). Launch parity: all 53
   captured launches, translated flag-by-flag into keys, resolve to their
   captured effective settings, and every re-recorded golden launch
   resolves to a captured launch's settings.

6. **One library decision** (N). Delete `--skip-libraries` /
   `--no-skip-libraries`; add `--library-matching off|report|names`
   (default off) and `[library] unnamed = "leave"|"model"`. Neutrality on
   the default; an e2e run proves `unnamed = "model"` equals today's
   `--no-skip-libraries` byte for byte. **Refusal tests, red first:**
   missing index; unreadable index; each version mismatch; `names` with no
   matching slice (refused); `report` with no matching slice (WARNING line
   - `recipeMissing` in diagnostics, output identical to `off`); a mode
     other than off without an index; an index with mode `off`.
7. **Harness: one run config** (G). `eval.ts` validates once and writes the
   run config JSON plus the binary's `config.toml` per launch; `run.sh` and
   `neutrality.sh` read them instead of re-parsing argv; launches pass only
   the six operational flags; pipeline.json records the config sha and the
   library block; with library matching on, `run.sh` runs `humanify index
verify --recipe` per pair and stops the label on a missing slice (test,
   red first).
8. **Thresholds into the config, by area** (N each). Library lookup and
   discovery first (they arrive with matching-architecture step 4), then
   any naming/LLM constants a user needs; update tuning-values.md rows to
   point at the config key.

## 6. Decided and open

Decided (Andrew, 2026-10-13): TOML; only the operational flags stay
(input, output, model endpoint, `--library-matching`, `--library-index`,
`--config`), everything else config-file only; loud early refusals (§3.1);
`env-reads` deleted.

Open:

- Does "input" include `--prior-version` (a second input file) and
  "model endpoint" include `--model`? This doc assumes `--model` stays a
  flag and `--prior-version` moves to `[inputs]`; either is a one-line
  change.
- Deleting `--api-key` and the `OPENAI_API_KEY` fallback (proposed here,
  §3.3) changes every launch golden (`--api-key local`).
