# Operations runbook: building, running, observing, debugging

How the Rust binary is built, invoked, watched, and debugged — everywhere the
TS pipeline runs today: direct CLI, eval harness, version walk, devcontainer.
The design constraint throughout: **the pipeline's observable surface is a
contract other tooling already scrapes** — flags, exit codes, `ERROR:` lines,
two debug log formats, profiler span names, the cache's on-disk entry count,
and the `-run.json` / `-run-status.json` shapes. This doc freezes that
surface. Architecture is `02-rust-target-architecture.md`; sequencing is
`03-migration-plan.md`; harness integration (`--pipeline-cmd`) is doc 07;
lint/guard enforcement is doc 05; run-metadata schema is doc 09. Claims about
the current TS pipeline are cited; everything about the binary is design.

## 1. Building

One workspace, five crates — `humanify-model`, `humanify-core`, `humanify-llm`,
`humanify-cli`, `humanify-parity` (`02-rust-target-architecture.md` section 2;
amended 2026-09-19: was "three crates"; 02 §2 and 05 §1 win). The
binary is named `humanify`, built from `humanify-cli`.

| profile   | command                           | binary path                 | use                                                                       |
| --------- | --------------------------------- | --------------------------- | ------------------------------------------------------------------------- |
| dev       | `cargo build`                     | `target/debug/humanify`     | iteration, unit tests, parity-gate debugging                              |
| release   | `cargo build --release`           | `target/release/humanify`   | **everything measured or harness-driven**                                 |
| profiling | `cargo build --profile profiling` | `target/profiling/humanify` | release speed plus debug symbols, for samply / perf / Instruments (05 §6) |

_amended 2026-09-19: `profiling` row added; the release profile ships without debug symbols — 05 §6 wins._

Any run whose numbers will be read — eval, neutrality, walk, profile traces,
RSS claims — uses the release binary; the `04-performance-model.md`
projections are release-profile numbers. The release profile ships WITHOUT
debug symbols; the separate `profiling` profile (inherits release, `debug =
true`, `05-rust-toolchain.md` §6) keeps line tables so backtraces and perf
profiles resolve to source (amended 2026-09-19: was "`[profile.release]
debug = 1`"; 05 §6 wins).
`cargo run -p humanify-cli -- <args>` is fine for dev invocations; the
harness always gets a built path. `rust-toolchain.toml` pins the compiler so
the laptop and devcontainer build identically.

### Version stamping

A small `build.rs` (or the `vergen` crate) embeds `git rev-parse HEAD` plus a
dirty flag; `humanify --version` prints `humanify <semver> (<sha>[ dirty])`.
This feeds the provenance the harness already records per scored run:
`provenance{commit, dirty, node, bun}` (experiments/lib/run-manifest.ts:32-80).
Today those describe the repo the harness spawned from; a binary can silently
diverge from it — the **stale-binary trap** (edit source, forget to rebuild,
score old code). Mitigation, extending the existing mixed-commit label guard
(`results/<label>/commit.txt` vs HEAD, scripts/eval.ts:72-92): the harness
runs `<binary> --version` before spawning and fails the run if the embedded
SHA does not match repo HEAD (dirty trees warn). Field placement in
`-run.json` is doc 09's schema; the requirement is stated here.

One improvement falls out free: today the walk executes `npx tsx
src/index.ts` per hop, so editing the tree mid-walk changes code between hops
(a standing footgun). A built binary is immutable — every hop runs
byte-identical code, and the embedded SHA in the log proves which.

## 2. CLI compatibility

The TS root command has 34 user-facing options plus the required positional
and one subcommand (src/index.ts:13-14; src/commands/unified.ts:1415-1568).
The binary reproduces all of them with `clap` (derive API) — as migration
scaffolding, not a compatibility promise: flag parity exists so the harness
and walk scripts drive both implementations unmodified through phase 5, and
post-cutover the surface evolves freely (02 §9). Unknown flags
keep failing loud: clap rejects unrecognized arguments by default; we
intercept `try_parse()` errors, print them, and exit 1 so **every failure
exits 1 and success exits 0** (parity with the invariant checks at
unified.ts:214/226 and the settings-resolution exit at unified.ts:1609; clap's native
usage-error exit code is 2, deliberately not kept).

Harness column: E = eval scored leg (experiments/034-eval-harness/run.sh:229-234),
N = neutrality leg (experiments/lib/neutrality.sh:170-175),
W = walk (walk-versions.sh:118-123).

| flag                                            | value / default             | meaning                                                                            | clap mapping                                               | used by   |
| ----------------------------------------------- | --------------------------- | ---------------------------------------------------------------------------------- | ---------------------------------------------------------- | --------- |
| `<input>`                                       | path, required              | minified JS file, or directory (Electron app)                                      | positional `PathBuf`                                       | E N W     |
| `--endpoint <url>`                              | `https://api.openai.com/v1` | OpenAI-compatible endpoint                                                         | `String` + `default_value`                                 | E N W     |
| `--api-key <key>`                               | none; env fallback          | flag > `HUMANIFY_API_KEY` > `OPENAI_API_KEY` (src/commands/settings.ts:126)        | `Option<String>`                                           | E N W     |
| `-m, --model <model>`                           | `gpt-4o-mini`               | model identifier                                                                   | `short`, `default_value`                                   | E N W     |
| `-o, --output-dir <dir>`                        | `output`                    | output directory                                                                   | `PathBuf`, `default_value`                                 | E N W     |
| `-v, --verbose`                                 | count, 0                    | `-v` info, `-vv` debug                                                             | `ArgAction::Count` -> `u8`                                 | E W       |
| `-c, --concurrency <n>`                         | 50                          | function-lane LLM concurrency                                                      | `u32`, `default_value_t = 50`                              | E N W     |
| `--module-concurrency <n>`                      | unset (bundler-aware)       | module-lane concurrency; `None` on purpose (settings.ts:30-34)                     | `Option<u32>`                                              | —         |
| `--max-tokens <n>`                              | unset                       | per-request completion budget                                                      | `Option<u32>`                                              | —         |
| `--ambiguity-probe <path>` (RETIRED 2026-09-26) | unset                       | matcher ambiguity probe JSON                                                       | `Option<PathBuf>`                                          | —         |
| `--disable <a,b>`                               | none                        | kill switches OFF; unknown name fatal                                              | `Vec<String>`, `value_delimiter = ','`, registry-validated | —         |
| `--probe <a,b>`                                 | none                        | instrumentation probes ON                                                          | same                                                       | —         |
| `--retries <n>`                                 | 3                           | failed-API-call retries                                                            | `u32`                                                      | —         |
| `--timeout <ms>`                                | 300000                      | LLM request timeout (`DEFAULT_LLM_TIMEOUT_MS`, src/commands/default-args.ts:16)    | `u64`                                                      | —         |
| `--llm-cache <dir>`                             | unset                       | content-keyed disk response cache                                                  | `Option<PathBuf>`                                          | E (opt) N |
| `--reasoning-effort <l>`                        | unset (server default)      | `low\|medium\|high`; invalid fatal                                                 | `ValueEnum`                                                | E N W     |
| `--skip-libraries` / `--no-`                    | true                        | skip library code                                                                  | paired `SetTrue` + `overrides_with` -> `bool`              | —         |
| `--log-file <path>`                             | unset                       | append debug log; implies `-vv`                                                    | `Option<PathBuf>`                                          | E W       |
| `--diagnostics <path>`                          | unset                       | rename diagnostics JSON; arms trails (unified.ts:1127-1134)                        | `Option<PathBuf>`                                          | E         |
| `--stats-json <path>`                           | unset                       | eval stats JSON (unified.ts:452-501)                                               | `Option<PathBuf>`                                          | E         |
| `--bundler <t>`                                 | detected                    | `webpack\|browserify\|rollup\|esbuild\|parcel\|bun` (src/detection/types.ts:24-31) | `ValueEnum`                                                | —         |
| `--minifier <t>`                                | detected                    | `terser\|esbuild\|swc\|bun\|none` (types.ts:33-39)                                 | `ValueEnum`                                                | —         |
| `--batch-size <n>`                              | 10 (downstream)             | identifiers per LLM batch                                                          | `Option<u32>`                                              | —         |
| `--max-retries <n>`                             | 2 (downstream)              | per-identifier call limit                                                          | `Option<u32>`                                              | —         |
| `--max-free-retries <n>`                        | 100 (downstream)            | cross-lane collision retries                                                       | `Option<u32>`                                              | —         |
| `--lane-threshold <n>`                          | 25 (downstream)             | min bindings for parallel lanes                                                    | `Option<u32>`                                              | —         |
| `--split`                                       | off                         | emit multi-file runnable tree                                                      | `SetTrue`                                                  | E N W     |
| `--prior-version <path>`                        | unset                       | prior humanified bundle; empty file fatal (unified.ts:1065-1079)                   | `Option<PathBuf>`                                          | E N W\*   |
| `--reconcile-prior-diff` / `--no-`              | on when prior given         | snap noise hunks to prior names                                                    | paired -> `Option<bool>`                                   | —         |
| `--naming-floor` / `--no-`                      | on                          | deterministic minted-token closing                                                 | paired -> `Option<bool>`                                   | —         |
| `--naming-floor-sweep` / `--no-`                | on; off when floor off      | LLM-name floor survivors                                                           | paired -> `Option<bool>`                                   | —         |
| `--split-ledger <path>`                         | auto-discovered             | prior split ledger; needs `--split`                                                | `Option<PathBuf>`, `requires = "split"`                    | —         |
| `--split-pure`                                  | off                         | byte-exact review tree; needs `--split`                                            | `SetTrue`, `requires = "split"`                            | —         |
| `--rename-ledger <dir>`                         | unset                       | replayable ledger + `apply.mjs`                                                    | `Option<PathBuf>`                                          | —         |
| `--profile <path>`                              | unset                       | Chrome Trace Event JSON                                                            | `Option<PathBuf>`                                          | —         |

\* W passes `--prior-version` on every hop after the anchor
(walk-versions.sh:101-110).

Mapping notes:

- **Negatable booleans**: commander's `--x`/`--no-x` becomes a clap pair
  (`SetTrue` each way, `overrides_with`) resolved to `Option<bool>`; `None`
  means "not stated". That replaces `getOptionValueSource` explicitness
  detection (unified.ts:1574-1578), so the typed contradiction
  `--naming-floor-sweep` with `--no-naming-floor` stays a post-parse check
  exiting 1 (parity with unified.ts:218-227).
- **Numerics are typed**: commander passes strings, settings parses
  downstream; clap parses at the boundary, so `-c fifty` fails at parse —
  stricter, still loud.
- **`requires = "split"`** moves the `--split-pure`/`--split-ledger`
  invariants (unified.ts:144-186) into the parser; message text changes,
  loud-exit-1 does not.
- **`env-reads`** ports as-is: `humanify env-reads <path>` (file or
  directory), `--markdown`, `-o/--output <file>`
  (src/commands/env-reads.ts:26-60) — analyzes TARGET code, pure oxc walk, no
  LLM.

What remains outside the table — **new surface**, absent from TS, listed
separately so nothing mistakes it for a parity obligation:

| new                      | kind       | purpose                                                                                                                |
| ------------------------ | ---------- | ---------------------------------------------------------------------------------------------------------------------- |
| `--dump-artifacts <dir>` | flag       | stage-boundary decision dumps for parity gates (`03-migration-plan.md` phase 0; the TS side grows the same flag first) |
| `explain`                | subcommand | join two releases' provenance sidecars over a diff hunk (`02-rust-target-architecture.md` section 4b; section 8 below) |
| `--trace-json <path>`    | flag       | structured tracing span/event stream (JSON lines) from the `tracing` subscriber, complementing `--profile`             |

## 3. Environment

**Kill switches are not environment variables.** Generation 2 (2026-08-12)
deleted the `HUMANIFY_NO_*` env vars; switches are typed values of
`--disable`/`--probe`, validated against a single registry — unknown or
wrong-kind names throw listing the valid set (src/kill-switches.ts:1-26,
140-160). The port carries the registry with the **same 14 names** and the
same validation. The full set (src/kill-switches.ts:49-120):

| name                    | kind    | area                  |
| ----------------------- | ------- | --------------------- |
| `family-permute`        | disable | naming (exp048)       |
| `shingle-probe`         | probe   | naming (exp053)       |
| `content-anchor`        | disable | placement (exp041)    |
| `anchor-preempt`        | disable | placement (exp042)    |
| `anchor-nearident`      | disable | placement (exp043)    |
| `allsame-vote`          | disable | placement (exp041)    |
| `empty-decl-hash-guard` | disable | placement (exp058)    |
| `registrar-exemption`   | disable | placement (exp049)    |
| `emit-align`            | disable | emission (exp037/038) |
| `name-align`            | disable | emission (exp050)     |
| `vendor-inherit`        | disable | vendor (exp046)       |
| `manifest-prior-order`  | disable | vendor (exp047)       |
| `post-split-reconcile`  | disable | post-tree (exp054)    |
| `fossil-split`          | disable | placement (exp070)    |

In Rust: one `switches` module owning the registry, `switch_on(name)`,
`active_kill_switches()`, a test-reset hook (mirrors
src/kill-switches.ts:125-191). The harness keeps recording switches from the
child argv unchanged (experiments/lib/run-pipeline.ts:144-156).

**Environment reads collapse to one module.** The pipeline's own process
reads exactly two variables — `HUMANIFY_API_KEY` / `OPENAI_API_KEY` via
`env()` (src/env.ts:5-8) with `dotenv.config()` loading a local `.env`
(src/env.ts:3), resolved CLI > env > default and frozen once
(src/commands/settings.ts:124-162). The binary keeps both names and the
precedence, loads `.env` with the `dotenvy` crate, and confines every
`std::env` access to the one `env` module
(`02-rust-target-architecture.md` section 3). Clippy's `disallowed_methods`
(a `clippy.toml` `disallowed-methods` entry for `std::env::var`/`var_os`)
enforces it outside that module — wiring in doc 05. This is the Rust form of
the existing guard test that fails if `src/` reads a switch elsewhere.

**Two env vars live in the EMITTED tree, not the pipeline:**
`HUMANIFY_STRIP_USING` and `__HUMANIFY_USING_REEXEC` are read by the
generated `run.cjs` at runtime (src/split/runnable-scaffold.ts:160, 172, 184;
the deliberate exception at src/kill-switches.ts:23-25). They are scaffold
text and carry over verbatim. Walk-script variables (`HUMANIFY_ENDPOINT`,
`HUMANIFY_MODEL`, `HUMANIFY_API_KEY`, `CONCURRENCY`, `FORCE`,
`FRESH_ANCHOR`) are read by walk-versions.sh itself (lines 22-30, 96-99,
133-140), not the pipeline — unaffected. One qualifier: `HUMANIFY_API_KEY`
doubles as one of the pipeline's two env reads (settings.ts:126), but the
walk always passes it as `--api-key` (walk-versions.sh:120), so the
pipeline-side fallback never fires here.

**Secrets handling.** The local vLLM endpoint takes a placeholder key
(`apiKey: "local"`, pairs.json) on an unauthenticated LAN address — the
threat there is misconfiguration, not leakage. A REAL key enters the picture
because the CLI's default endpoint is api.openai.com: keys travel only via
flag or env (never files in the repo; any `.env` stays gitignored), the
pipeline never logs the resolved key at any verbosity, and the dump/oracle
artifacts must carry endpoint and model but NEVER the key — the
`--dump-artifacts` config record redacts it, and a parity test asserts the
dump tree contains no key material. CI (05 §9) gets no LLM secret at all:
its jobs are fmt/clippy/test/fixture-parity, none of which touch a live
endpoint by design.

## 4. Logging and observability

Logging is `tracing` + `tracing-subscriber`. Level 0 = renderer only (rich
TTY UI when stderr is a TTY, unified.ts:1598-1600); `-v` = INFO; `-vv` =
DEBUG — the same clamp-to-0..2 as src/verbose.ts:10-12. `--log-file` keeps
its TS behavior: raise the level to at least 2 and redirect the
debug/verbose stream to the file (unified.ts:1584-1594).

**Two line formats are a scraping contract**, reproduced byte-for-byte by a
custom `FormatEvent` implementation and pinned by snapshot tests written
red-first against golden lines from real TS logs. The shared timestamp shape
is `[YYYY-MM-DD HH:MM:SS]` — ISO with `T` replaced by a space, fractional
seconds stripped (src/verbose.ts:34-39; src/debug.ts:136-138).

`[DEBUG:batch-timing]` — a blank line, a header line, then the message
indented two spaces (rendered src/debug.ts:314-325; emitted
src/rename/processor.ts:1171-1174):

```
[<YYYY-MM-DD HH:MM:SS>] [DEBUG:batch-timing]
  <message>
```

with the exact message template (processor.ts:1173):

```
${callbacks.functionId} call=${callNum} prompt=${promptMs}ms llm=${llmMs}ms rename=${renameMs}ms valid=${validThisCall}/${batchRetries.length}
```

`[QUEUE-STATE]` — one line, parts joined by `|` (src/debug.ts:501-526):

```
[<YYYY-MM-DD HH:MM:SS>] [QUEUE-STATE] <event> | ready=<n> processing=<n> pending=<n> done=<done>/<total> | inflight-llm=<n>[ | <detail>]
```

`event` is one of `dispatch | completion | waiting-on-llm | waiting-on-deps |
deadlock-break` (debug.ts:508-513). Note: on the current branch this
formatter is defined but has no call site — the line is dormant, with a
historical example at
test/e2e/output/preact/v10.26.0-shadow-fix-debug/debug.log:148. The port
carries the format as the contract for whenever the wave scheduler re-arms
it; a re-armed emitter must produce exactly this shape.

**`ERROR:` lines are a parsing interface.** The harness extracts
`ERROR:`-prefixed lines from the run's stdout file into `-run.json` (max 10
lines, run-pipeline.ts:51, 235-241) and, with each line's indented
continuations, into `-run-status.json` (max 60 lines total,
experiments/lib/invariants.ts:49, 69-84). The
binary keeps emitting fatal diagnostics in that shape on standard output,
matching the TS sites (unified.ts:1346, 1351, 1372, 1375, 1392).

### `--profile` and the span-name registry

`--profile <path>` writes Chrome Trace Event JSON to exactly that path, as
today (unified.ts:1397-1413), viewable at chrome://tracing or ui.perfetto.dev.
Today span names are free-form strings at call sites with no central registry
(src/profiling/profiler.ts:53-78); the port makes the list a compiled
constant table, and **the names below are preserved verbatim** so existing
habits and docs transfer. Complete inventory:

| span name(s)                                                                                                                                                                                | category      | site                                                                        |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- | --------------------------------------------------------------------------- |
| `detection`, `split`                                                                                                                                                                        | pipeline      | unified.ts:1092, 990                                                        |
| `unpack`, `library-detection`                                                                                                                                                               | pipeline      | src/unminify.ts:45, 66                                                      |
| `file-io:read`, `file-io:write`                                                                                                                                                             | io            | src/unminify.ts:109, 142                                                    |
| `babel-transforms`                                                                                                                                                                          | pipeline      | src/plugins/babel/babel.ts:133                                              |
| `graph-build:functions`, `:callees`, `:scopes`, `:modules`                                                                                                                                  | graph         | src/analysis/function-graph.ts:55, 104, 111, 771                            |
| `parse`, `graph-build`, `prior-version`, `rename:functions`, `rename:library-prefix`, `generate`, `validate-output`, `rename:naming-floor`, `reconcile-prior-diff`, `rename:deferred-sweep` | pipeline      | src/rename/plugin.ts:862, 888, 939, 975, 988-991, 1051, 295, 371, 423, 469  |
| `prior-version:apply`                                                                                                                                                                       | pipeline      | src/rename/prior-transfer.ts:531                                            |
| `prior-version:graph`, `:module-bindings`, `:statement-twin`, `:match-functions`, `:close-match`, `:parse`                                                                                  | pipeline      | src/prior-version/prior-version.ts:280, 347-350, 403-406, 540-543, 584, 677 |
| `fn:<id>` (dynamic)                                                                                                                                                                         | rename, tid 2 | src/rename/processor.ts:1402-1406                                           |
| `mb:<sessionIds>` (dynamic)                                                                                                                                                                 | rename, tid 3 | src/rename/processor.ts:1503-1507                                           |

Thread IDs keep `TRACE_TID = {PIPELINE: 1, RENAME_FUNCTION: 2,
RENAME_MODULE_BINDING: 3}` (src/profiling/types.ts:91-98); summary semantics
stay (stage rows from tid 1, rename p50/p95/p99 from category `rename` tid 2,
profiler.ts:133-179). `babel-transforms` names the beautify stage and is kept
verbatim through the migration even though the stage is no longer Babel;
renaming it is a post-cutover decision (open questions).

**Per-stage wall times are always on.** Today phase timing exists only under
`--profile`, which the eval and walk never pass (`04-performance-model.md`
closing note). The binary records per-stage wall times unconditionally in its
run metadata under `.humanify/` (joining the always-on
`placement-stats.json` and `stage-hashes.json`, src/split/layout.ts:56, 73),
and the harness folds them into `-run.json` the way it folds placement stats
(run-pipeline.ts:222-233), next to the existing `wallSeconds`
(run-manifest.ts:32-80). Field shape is doc 09.

## 5. Resource observability

**Peak RSS keeps its owner and gets simpler.**
experiments/lib/run-pipeline.ts is the named owner of that measurement
(docs/measurement-pitfalls.md rule 3 records the 82 MB-for-132k-calls
incident that made it so): it samples every 250 ms (`RSS_SAMPLE_MS`,
run-pipeline.ts:49) plus one immediate leading sample (run-pipeline.ts:207),
reads `VmHWM`/`PPid` from `/proc/<pid>/status` for every live process
(run-pipeline.ts:105-131), and sums the child's process tree
(run-pipeline.ts:202) — necessary today because `npx` makes the real worker a
**grandchild** (run-pipeline.ts:100-104). With a binary the spawned process
IS the worker; the tree walk still runs and finds a tree of one (plus a
webcrack subprocess when that adapter fires,
`02-rust-target-architecture.md` section 8). No harness change required. On
non-Linux (the laptop) `peakRssMb` stays undefined (run-pipeline.ts:111);
RSS numbers come from the devcontainer.

Expected values, so anomalies are recognizable: cold hops today peak at
15-30 GB RSS under a 64 GB heap (`01-current-architecture.md` section 9); the
arena model projects a few GB (well under the 2–4 GB first estimate;
`02-rust-target-architecture.md` §6, `04-performance-model.md`, labeled
estimate; amended 2026-09-19: was "~2-4 GB"; 02 §6 wins). A release-binary hop
at 15 GB is a leak or a retained arena, not normal.

**The heap-size folklore is obsolete for the binary.** Two artifacts exist
only because V8 needed a sized heap:
`NODE_OPTIONS="--max-old-space-size=14336"` in the walk
(walk-versions.sh:118) and `--heap-mb` (default 65536) in the
eval/neutrality stack, injected as `NODE_OPTIONS` by run-pipeline.ts:185-196
(neutrality.sh:60; CLAUDE.md records why 14336 stopped sufficing). The binary
has no tunable heap. Both knobs **stay while TS legs exist** (TS remains the
production pipeline through phase 5, and phases 4-5 run cross-implementation
legs) and are deleted from run.sh, eval.ts,
neutrality.sh, and walk-versions.sh at phase 6 (`03-migration-plan.md`);
until then, doc 07's `--pipeline-cmd` routing applies `NODE_OPTIONS` only to
TS legs.

One cache-adjacent contract lives here because it is how the harness
observes LLM liveness: **cache entries are counted as files on disk** —
run-pipeline.ts counts the cache dir recursively before spawn and after exit
and records `written = entriesAfter - entriesBefore` (run-pipeline.ts:75-86,
182, 217, 274), and neutrality fails on a nonzero baseline-leg count
(neutrality.sh:275-278). docs/measurement-pitfalls.md rule 10 names cache
WRITES as the only valid live-call signal. The Rust cache therefore keeps
one file per entry in the same directory layout
(`02-rust-target-architecture.md` section 7 freezes the format for replay;
this adds why the COUNT is load-bearing).

## 6. Running it

### Direct CLI

Fresh bundle, local vLLM endpoint (walk defaults, walk-versions.sh:22-30):

```
target/release/humanify bundle.js --split \
  --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
  --api-key local --reasoning-effort low -c 32 -o out/2.1.89
```

Cross-version hop — the prior is the previous run's
`.humanify/humanified.js` (src/split/layout.ts:42):

```
target/release/humanify bundle.js --split \
  --prior-version out/2.1.89/.humanify/humanified.js \
  --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
  --api-key local --reasoning-effort low -c 32 -o out/2.1.90
```

Directory input (extracted Electron app, branch `feat/electron-unpacking`):
pass the app directory as `<input>`; detection routes it via
`detectElectronApp` and the electron adapter (`01-current-architecture.md`
section 2), with loud failure for any flag the format cannot honor.

### Eval harness and neutrality

Every harness spawn site hardcodes `npx tsx .../src/index.ts` today — all
seven: run.sh:198 (rebase leg), run.sh:369 (self-hop leg),
run-pipeline.ts:187 (scored leg), experiments/lib/selfhop.sh:77,
experiments/lib/gate.sh:109, neutrality.sh:170, and
experiments/076-statement-placement/walk.sh:76 (the mini-walk driver; amended 2026-09-19: was six; 09 §1 wins). Doc 07's `--pipeline-cmd`
routes all seven through one configurable command; nothing else changes,
because the instruments interact with the pipeline as a subprocess plus file
trees (`02-rust-target-architecture.md` section 8). The measurement rules
bind unchanged: warm-cache-only neutrality verdicts, cache writes as the
liveness signal, no cache for LLM-dependent verdicts
(docs/measurement-pitfalls.md rules 10-11; CLAUDE.md "Validating
cross-version changes").

### The version walk

Current invocation (walk-versions.sh:118-123), quoted:

```
( cd "$HUMANIFY" && NODE_OPTIONS="--max-old-space-size=14336" npx tsx src/index.ts \
    "$input" --split \
    --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY" \
    --reasoning-effort low -c "$CONCURRENCY" \
    -o "$out_dir" ${prior_args[@]+"${prior_args[@]}"} \
    -vv --log-file "$log" > "$log.stdout" 2>&1 )
```

Exact edit at cutover — replace the launcher, drop `NODE_OPTIONS`, keep every
flag (all are in the compatibility table):

```
( "$HUMANIFY/target/release/humanify" \
    "$input" --split \
    --endpoint "$ENDPOINT" --model "$MODEL" --api-key "$APIKEY" \
    --reasoning-effort low -c "$CONCURRENCY" \
    -o "$out_dir" ${prior_args[@]+"${prior_args[@]}"} \
    -vv --log-file "$log" > "$log.stdout" 2>&1 )
```

(The `cd "$HUMANIFY"` subshell existed for `npx` resolution; an absolute
path does not need it.) Resume semantics are untouched because they read
artifacts, not the launcher: an output dir already containing
`.humanify/humanified.js` is skipped and reused as the next prior unless
`FORCE=1` (walk-versions.sh:96-99); a hop finishing without `humanified.js`
stops the walk so `--prior-version` never points at a hole (lines 124-127);
the anchor pre-exists and `FRESH_ANCHOR=1` rebuilds it (lines 17-18,
133-140).

### Devcontainer

Standing project constraint: the container's paths mirror the laptop
(`/Users/andrewgross/Development/humanify` resolves identically), load-bearing
for every script above. Consequences:

- Install `rustup` in the container; the first `cargo` invocation picks up
  `rust-toolchain.toml`. Build **inside** the container — macOS and Linux
  binaries are not interchangeable.
- Keep the default in-repo `target/` so `$HUMANIFY/target/release/humanify`
  resolves identically on both machines. `target/` is per-machine output:
  exclude it from any sync between the laptop and the `/data4` canonical
  copy; each side builds its own.
- The pipeline binary needs no Node. Node stays for the harness — the
  measurement stack remains TypeScript by design
  (`02-rust-target-architecture.md` section 8) — so the container's existing
  Node setup (including the `npm ci --ignore-scripts` + esbuild/@swc rebuild
  workaround for the dead `isolated-vm` dependency) persists for
  `experiments/` and `scripts/`, not for the pipeline.
- RSS measurement works only here (Linux `/proc`), so devcontainer runs are
  where memory claims get their numbers (section 5).

**Disk management** (both hosts; the container mirroring the laptop doubles
every exposure). Retention rules per artifact class, TOTAL exposure first:
the growing classes are walk output trees (~93 MB × 124+ hops if all kept),
oracle dumps (~1–2 GB per commit, retention owned by 07 §10: current +
predecessor), the warm LLM cache (append-only; historically ~24k entries),
and neutrality workdirs. Rules: walk TREES are regenerable — the durable
record is the history repo's commits, so old `versions/` dirs beyond what
resume needs are deletable at will; the LLM cache is never pruned during a
walk or a parity phase (a pruned entry silently converts a warm leg to a
partially cold one — rule 10's failure shape), archived per oracle label
otherwise; neutrality's `$WORK` dirs are per-run scratch, deletable when no
leg is running; criterion baselines and fixture-scale parity dumps are tiny
and committed. Anything not in this list that grows gets a rule before it
gets big.

## 7. Failure behavior

**Exit codes.** Success is 0. Every failure is 1: flag-invariant violations
(TS parity: unified.ts:214, 226), settings-resolution errors (unified.ts:1609
— a mid-pipeline throw in TS exits 1 via Node's unhandled-rejection default,
uncaught by any handler),
and the marked-failed validation paths that set the exit code while still
writing output for inspection — output-parse failures (unified.ts:1353),
**rename-invariant violations** (unified.ts:1377), internal rename errors
(unified.ts:1394). The rename-invariant path keeps its full TS shape: the
rejected file, its pre-rename source, and the validated form preserved under
`.humanify/failed/` (src/failed-output.ts:39, 80-100; invoked before split,
unified.ts:1244-1249), the `ERROR:` lines printed, exit 1. Exit-code parity
is not cosmetic — neutrality compares the two legs' exit codes
(neutrality.sh:275-278), and cross-implementation legs run throughout phases
4-5.

**Panics are loud failures.** A top-level panic hook prints an
`ERROR:`-prefixed line with the panic message and location (so the
`-run-status.json` scraper catches it), a pointer to re-run with
`RUST_BACKTRACE=1`, and exits 1. A `profiling`-profile build (section 1;
`05-rust-toolchain.md` §6) makes those backtraces resolve to lines — the
release binary carries no symbols, so a release backtrace names frames only
(amended 2026-09-19: was "`[profile.release] debug = 1`"; 05 §6 wins). This
is the existing house rule — fail loudly instead of silently half-working
(`02-rust-target-architecture.md` section 10; amended 2026-09-19: was
"section 9", which is now the compatibility posture) — applied to the one
failure mode Rust adds.

**Where errors surface is unchanged.** `-run-status.json` keeps
`{version, exitCode, errors[]}` (experiments/lib/invariants.ts:37-44), fed by
the `ERROR:` contract of section 4; `-run.json` keeps `outcome{exitCode,
errors, peakRssMb, artifacts}` (run-manifest.ts:32-80). A binary that panics,
a binary that exits 1 with a rename-invariant report, and a TS leg that did
the same are indistinguishable to the harness — which is the point.

**Post-cutover regression recovery** (a bug that cleared the gates and
surfaces at hop N, months after `src/` is deleted). The recovery instrument
is the Rust fix plus a rebase, never resurrecting the TS pipeline — the
archived TS stack is archaeology (03: oracle dumps + git history), and
against months of drifted node_modules it is not a runnable fallback.
Triage: (a) formatter-class regression — fix the Rust emit, then
`REBASE_PRIOR` regenerates the affected base exactly as any formatting
change is handled today; if hash semantics moved, the ledger's hashVersion
bumps and the next run re-derives the prior's tables from the prior tree
(`12-layout-and-diff.md` §2's standing re-derivation mechanism — permanent
infrastructure, not a one-time step; amended 2026-09-19: was mis-cited as
"07 §7", which is eval integration, and 12 §2 wins); (b) decision-class regression — the sidecar +
trails localize it (section 8), the fix ships behind the standard gates, and
the walk re-runs from hop N (hops are ~2–3 min post-port, so re-walking a
segment is cheap); (c) if the regression corrupted committed history-repo
hops, rebuild those commits from re-runs — the history repo is derived
output, never the primary record of anything but itself.

## 8. The explain workflow (worked example)

This section is design, not description: it exercises the provenance sidecar
of `02-rust-target-architecture.md` section 4b, emitted per output file under
`.humanify/` (path proposal: `.humanify/provenance/`), joining data that
already exists as trails — `resolutionStats` (stats JSON, unified.ts:452-501)
and `strategyTrails`/`placementTrails` (diagnostics report,
src/rename/diagnostics.ts:367-390).

A walk hop lands and the release diff shows a hunk you did not expect:

```
$ git -C ~/Development/unpacked-claude-code diff v2.1.215..v2.1.216 -- src/
--- a/src/permissions/rules.js
+++ b/src/permissions/rules.js
-function getToolPermissions(rule, ctx) {
+function resolveToolPermissions(rule, ctx) {
```

Ask both releases why:

```
$ target/release/humanify explain \
    --prior ~/walk/2.1.215 --fresh ~/walk/2.1.216 \
    src/permissions/rules.js:142
```

Output (sketch — field names follow the existing trail vocabulary):

```
fresh 2.1.216  src/permissions/rules.js:142  span 4812..5390
  statementHash  ik:9f3a…   binds: resolveToolPermissions (+2 locals)
  match   close-match (cosine 0.87, shingle 0.61) -> prior fn #4411  [resolutionStats]
  name    LLM wave 5, prompt #1231; close-match context carried
          prior name getToolPermissions                              [strategyTrail]
  place   name-votes 3/3 -> src/permissions/rules.js                 [placementTrail]

prior 2.1.215  src/permissions/rules.js:138  span 4700..5241
  statementHash  ik:77c1…
  match   exact structuralHash (tier 1)
  name    transferred: exact-match slot table
  place   hash tier
```

Reading: the content changed upstream (statement hashes differ, match
degraded exact -> close-match), so the name went to an LLM wave with the prior
name as context and the model re-drew it — a naming-drift candidate, not a
placement or mechanical-transfer bug. The verb prints both chains;
classification stays with the human. It replaces the offline reconstruction
loop (grep the diagnostics JSON, cross-reference the stats file, re-derive
spans by hand) that answers this question today.

## 9. Distribution

**During migration, `cargo build --release` from a checkout is the only
supported path.** Every consumer in this doc takes a filesystem path to the
binary, so nothing needs packaging to complete phases 0-6.

The npm story is an **open decision**, deliberately deferred. Today the
package publishes `bin: {"humanify": "dist/index.mjs"}` (package.json:38-40).
Options: (a) napi-rs bindings keeping a Node entry point — maximum
compatibility, but it drags Node back into the process model; (b) a
sidecar-binary npm package in the esbuild/biome style — platform binaries as
optionalDependencies behind a JS shim, a proven pattern with real
release-engineering cost (per-platform CI matrix); (c) stop publishing and
distribute via GitHub releases — cheapest, right if the user base is this
repo's own workflows. Recommendation: **defer past phase 6**; the decision
gates no migration work. Whichever option is chosen, licensing rides along:
shipping compiled binaries adds per-artifact notice/attribution obligations
beyond the source-graph allowlist, and option (b) multiplies them per
platform — see the forward note in `05-rust-toolchain.md` §5.

## Open questions

- Usage-error exit code: this doc pins ALL failures to exit 1 by intercepting
  clap's default of 2. Confirm no tooling distinguishes usage errors from
  runtime failures before phase 6 locks it.
- `babel-transforms` span name outlives Babel: rename after cutover (breaking
  trace-reading habits once) or keep the fossil name?
- `[QUEUE-STATE]` is dormant (formatter defined, no emitter). Does the Rust
  wave scheduler re-arm it with the preserved format, or is the format
  retired with a note in doc 09?
- `--heap-mb` end state at phase 6: delete from `SCORE_FLAGS` (loud
  unknown-flag failure for old habits) or accept-and-warn for one window?
- Per-stage wall-time file name and schema under `.humanify/` — doc 09 owns
  it; section 4 states only the always-on requirement and the fold-in path.
- Devcontainer `target/` on the mirrored path vs a container-local scratch
  disk for build speed: mirrored is assumed for path identity; measure build
  times before optimizing.
- npm distribution (section 9): napi vs sidecar binary vs stop publishing —
  decide after phase 6.
