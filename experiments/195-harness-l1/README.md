# exp195 — harness migration lane L1: the `humanify-ag-dev` crate skeleton

STATUS: DONE (2026-11-07), branch `harness-l1-skeleton`. Scope is
`docs/dev-harness.md` §5 lane L1 (with Andrew's 2026-11-06 amendments),
nothing more: the skeleton every later lane (L2a, L2b, L3, L4) plugs into.
No instrument moved yet; the TS / Python harness still runs every eval.

## Totals

- **Two new crates**, about 6,000 lines of Rust (1,300 of them tests):
  `crates/humanify-ag-dev` (binary `humanify-ag-dev`, run as `cargo dev`)
  and `crates/humanify-ag-testkit` (test-only). Neither is part of the
  release artifact; no new external dependency (Cargo.lock adds only the
  two crates).
- **One product refactor**: the settings-registry machinery moved out of
  the CLI's `config/file.rs` and `surface.rs` into `humanify_ag_cli::registry`,
  which both binaries use. It changed **no product output**: warm
  neutrality NEUTRAL (0 differing files, 0 differing lines, baseline leg +0
  cache writes, both exit 0), and `humanify.example.toml` plus the three
  help goldens regenerate with zero diff.
- **JSON spellings: 28,444 / 28,444** committed `.json` files under the two
  results trees round-trip byte for byte (28,003 Python, 395 JS, 45
  prettier-reflowed JS, 1 compact JS).
- **Backfill**: every committed eval label (43) and library label (535,
  26,000+ cards) gets an identity per card; the frozen reference cards
  backfill with their naming mode, base mode, index id and binary.
- **Gate**: see §8.

## 1. Crate layout

```
crates/humanify-ag-dev/            publish = false; [[bin]] humanify-ag-dev
  src/main.rs                      the verb table (31 verbs) + dispatch
  src/settings/{mod,keys}.rs       HARNESS_KEYS + the one parse point
  src/committed/                   one typed loader per committed file
  src/identity/{mod,backfill}.rs   the run identity, the ONE guard, the legacy backfill
  src/json/{mod,py,prettier}.rs    the JSON spellings of every legacy record
  src/env.rs                       the one environment read
  src/proc.rs                      the one subprocess helper (deadlines)
  src/bin_build.rs                 build the pipeline for any checkout
  src/paths.rs                     where the committed files are TODAY
crates/humanify-ag-testkit/        publish = false: the one stub LLM + the shuffled fake
.cargo/config.toml                 [alias] dev = "run --profile dev-harness --locked -p humanify-ag-dev --"
Cargo.toml                         [profile.dev-harness] inherits release, lto = false, codegen-units = 16
```

`cargo dev` prints the table; a cold `cargo dev` build took 33 s.

## 2. The verb table

Every verb of §2.4 is an entry with `usage`, `proves`, `cannotProve`, the
settings it reads and, until its lane lands, the lane and what runs today.
A test fails an entry missing `proves` or `cannotProve`, or a §2.4 verb
missing from the table. A verb not yet ported prints
`not yet ported (lane Lx). Until it lands: <today's command>` and exits 2.
`build-bin [<checkout-dir>]` is the one verb L1 implements.

| Lane | Verbs left to port                                                                                                                     |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------- |
| L2a  | eval rescore, summarize, leaderboard, bands, diff                                                                                      |
| L2b  | eval vendor-precision, cjs-placement; library score, leaderboard, determinism, corpus, layers                                          |
| L3   | eval score, neutrality, walk; run stub; index build; boot-gate                                                                         |
| L4   | check, e2e, format-golden, census clones / concurrency / slop, ask-trace, diff-asks, match-truth, env-check, precommit, stub-llm serve |

## 3. The settings registry

- `humanify_ag_cli::registry`: `Key`, `Kind`, `Val`, `Registry`, `resolve`
  (flag / file / default, the conflict rule), `check_range`, "did you
  mean", the rendered example, and `registry::flags` (clap arguments from a
  table). The pipeline's table is `config::keys::PIPELINE`; nothing was
  copied. New for the harness: repeatable list flags (`--pipeline-set`,
  one item per use, order kept), a registry's own retired flags, the
  nearest flag for an unknown one, and `key=value` text typing.
- `HARNESS_KEYS` (`settings/keys.rs`) has the §2.2 sections `run`, `eval`,
  `neutrality`, `libscore`, `llm`, `boot`, `index`, every key a flag. The
  spellings people type are kept (`--workdir`, `--pairs`, `--endpoint`,
  `--cache`, `--index`, `--rule`, ...). Defaults that live in a file are
  unset, and `unset` says which file (`pairs.json llm.model`,
  `.humanify.local.json`).
- One parse point, `settings::parse(verb, accepted, harness_owned, args)`.
  Refused before anything runs, all at once: retired spellings with their
  replacement (`--no-vendor`, `--pipeline-arg`, `--heap-mb`,
  `--seeded-base`, `--archive-prior`, `--skip-preflight`), unknown flags
  with "did you mean", a flag that is not a setting of this verb, wrong
  types and ranges, and every `--pipeline-set` entry the PIPELINE's
  registry refuses: unknown key (with "did you mean"), retired key, wrong
  type or range, or a key the verb sets itself.
- `settings::pipeline_default` reads HEAD's default from the linked
  registry. It replaces eval.ts's hand copy `BINARY_DEFAULT_MATCHING`.
- `settings::run_config` builds the run config a verb writes once (JS
  spelling); its sha goes into the identity.

## 4. Committed-file loaders

One typed loader per file, `deny_unknown_fields`, returning value + path +
sha256, read where the files are today:

- `pairs.json` (the string concurrency `"472"` is typed once);
- `noise-bands.json` (bands per naming mode);
- `self-hop-reference.json` (range per mode);
- all 13 launch goldens;
- all 16 library corpora (manifest + 167 truth layers; each layer kind's
  exact block set is checked);
- library `bands.json`;
- `scripts/concurrency-allowlist.json`;
- every e2e `fixture.config.json`;
- `.humanify.local.json`, with the endpoint resolver that replaces
  `llm-endpoint.sh`: flag, then this checkout's file, then the main
  checkout's file, else an error naming both ways to set it.

The library corpus and layer IDENTITIES are lib096's canonical shas (the
sha of the Python spelling, not of the file bytes). A test holds them equal
to a committed scorer-14 summary's recorded `corpusSha256` and layer shas.

## 5. Run identity and the one guard

`Identity` has the §3.1 fields. A field nobody recorded is
`{"unknown": "<why>"}`. `comparable(purpose, a, b)` is the one guard, and
`Purpose::fields` is its table:

- **AddCard**: everything except the per-card fields.
- **Leaderboard**: base mode, naming mode, index id, scorer, model,
  reasoning effort, pair inputs.
- **LibraryLeaderboard**: corpus name, corpus manifest, layers, entry,
  scorer.
- **Bands**: the leaderboard fields plus the binary sha.
- **NeutralityLegs**: the LLM settings, the cache dir, the pipeline sets,
  the index id, the naming mode.

The rules:

- UNKNOWN never equals anything, not even UNKNOWN.
- A record replaced by `null` (off mode's `libraryIndex`) is ONE mismatch,
  not one per field.
- `guard(..., force_mixed)` prints every mismatch by field name, or
  refuses.
- `label_cards` checks that a label's cards agree (a label is the set of
  its cards).

The backfill reads `commit.txt`, `pipeline.json` and the run manifests
(eval), or `summary.json` (library), with today's rules (`baseModeOf`,
`namingModeOf`, `labelNamingModes`).

## 6. env, proc, bin_build

- `env.rs`: argv, `HOME`, `PATH`, `HELD_LOCK` (one lock layer: a long verb
  must not take `/work/heavy.lock` again), and the working directory.
  `tests/env_guard.rs` now names it as the harness's one reader.
- `proc.rs`: the crate's only `Command::new`, on top of
  `humanify_ag_core::subprocess::run` (pipes drained, one deadline over the
  whole run, group kill). It is allowlisted once.
- `bin_build.rs`: reads the pipeline crate and binary name from the
  checkout being built, or from a commit through `git show`:
  `humanify-ag-cli`/`humanify-ag` since exp186, `humanify-cli`/`humanify`
  before, refused before the cutover. It records sha256, commit and dirt,
  probes a legacy leg (no `--config` in `--help`), and keeps
  `same_commit`. A test reads the crate across the exp186 rename from real
  commits.

## 7. The testkit

`humanify-ag-testkit`, which depends on `humanify-ag-model` only:

- `stub_server`: the one OpenAI-compatible stub. It records bodies,
  headers and the peak number of requests in flight, and can answer with a
  status, extra headers or a hang-up.
- `answers`: the policies ported from `stub-llm.ts`: `<id>Renamed`, the
  four-word module names, and the collision policy.
- `fake::ShuffledFake`: a `NameProvider` whose `run_pipelined` completes
  calls in a seeded random order. The same seed gives the same order, a
  different seed a different order, and the order is never the id order.

Porting the seven hand-written stubs and the 27 in-order fakes onto it is
L4.

## 8. Proof

| Check                                                                                                       | Result                                                                                                                                                                                                                                                                        |
| ----------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo test -p humanify-ag-dev` (unit, incl. the 28,444-file round trip, backfill of every committed label) | 48 + 8 (verb table) passed                                                                                                                                                                                                                                                    |
| `cargo test -p humanify-ag-testkit`                                                                         | 7 passed                                                                                                                                                                                                                                                                      |
| `cargo test -p humanify-ag-cli` (help goldens, example file, env guard, new registry tests)                 | pass                                                                                                                                                                                                                                                                          |
| Red seen first                                                                                              | repeatable flags (flags.rs forced off: 1 fail); census `test-support` confinement (1 fail before the census change); json round trip (46 files failed before the prettier and compact spellings); identity null-record collapse and library-label consistency (failed, fixed) |
| `humanify.example.toml`, `test/golden/help/{humanify-ag,humanify-ag-short,match}.txt` regenerated           | 0 diff                                                                                                                                                                                                                                                                        |
| Warm neutrality of the registry extraction, `ee101f168` vs main `19042bace`, 2.1.85->2.1.86, names mode     | NEUTRAL: 0 files / 0 lines differ, candidate +1767 / **baseline +0** cache writes, exit 0 / 0                                                                                                                                                                                 |
| `npm run check` in the worktree                                                                             | 13/13 PASS (unit 112.4 s, rust:unit 307.8 s, rust:build 135.8 s, e2e 77.1 s)                                                                                                                                                                                                  |
| `npm run check` on a clean checkout                                                                         | CLEAN_RESULT                                                                                                                                                                                                                                                                  |

## 9. What changed from the design, and why

1. **Two more legacy JSON spellings.** The design named two spellings. 46
   committed eval files are neither:

   - 45 were reflowed by the repo's prettier commit hook (every
     `2.1.86-boot.json` and `self-hop.json` but one, and two `*.stats.json`);
   - 1 `self-hop.json` is compact `JSON.stringify(x)`.

   `json::prettier` reproduces prettier 3's JSON printer for these values.
   It needs one input beyond the value: which objects the original kept
   expanded, read from the original text. It is checked against output
   recorded from the repo's own prettier, including fill mode for number
   arrays. The harness never WRITES these spellings.

2. **The per-card fields of AddCard** are spelled out:
   - `corpus.entry` and `corpus.inputs`;
   - `binary.path`;
   - the cache entry counts;
   - the launch TOML sha (it names its own output paths, so it differs per
     pair);
   - `runConfigSha256` (a forced `--pairs` re-run writes its own run
     config; what it SET is compared through `pipelineSettings.sets`).
3. **A label cannot contradict itself about what it never recorded.**
   `label_cards` skips fields that are unknown on BOTH cards. `comparable`
   itself stays strict: unknown never equals anything.
4. **Legacy eval cards have no scorer version**, so the guard refuses
   legacy labels for the leaderboard unless forced. That matches amendment
   (1): the new scorer re-scores the frozen trees. L2a decides whether
   `eval leaderboard` on a legacy label needs `--force-mixed` or a
   re-score.
5. **Scorer 12 put copies v2 under the old key.** It records the copies v2
   layer as `copiesSha256` (truth_audit `copies_sha`). The loader keys
   `copies2` there and the retired v1 layer as `copiesV1Sha256`.
6. **A 12th concurrency pattern, `test-support`.** The testkit is not
   `*_test.rs`, so the census scans it, and none of the 11 patterns fits a
   stub server. The census now refuses `test-support` outside
   `crates/humanify-ag-testkit/` (a new census test, red first).
   `docs/concurrency.md` has the row and a short section.
7. **`build-bin [<checkout-dir>]`** builds a checkout. Building an
   arbitrary REF needs a detached worktree, which is the L3 runner's job;
   `pipeline_crate_at_commit` already answers "which binary" for a ref.
8. **`--seeded-base` / `--archive-prior` became retired spellings** of one
   key, `--base-mode` (§2.2: "one key, not two exclusive flags").
9. **No TOML parser in the harness.** `bin_build` reads Cargo.toml line by
   line: env_guard rule 4 keeps TOML reading to the pipeline's config owner.
10. **A flaky test, caught by the clean-checkout gate.** The first clean run
    failed `rust:unit` once on a legacy-leg test: the test wrote a script
    and executed it, while a sibling test's fork still held the file open
    for writing (ETXTBSY). The scripts are now written by a child process;
    passed 3/3 repeats, then the clean gate below.
11. **One test depends on this box.** The test that backfills the frozen
    reference cards reads `/work/dev-harness-proof/cards`. It FAILS when
    the cards are missing, never passes green (the exp162 lesson).

## 10. What later lanes plug into

- **L2a / L2b**: `identity` and its guard (the leaderboards call
  `guard(Purpose::Leaderboard | LibraryLeaderboard, ...)`), `json` (write
  cards in their spelling), `committed::{eval,library}`, and
  `library::canonical_id`.
- **L3**:
  - `settings::parse` + `run_config` (each runner's verb is already in the
    table with its accepted keys);
  - `bin_build`, `proc`, `committed::local::resolve_endpoint`;
  - `identity` (write `identity.json` beside every card with
    `Identity::to_text`);
  - `HELD_LOCK` from `env`.
- **L4**:
  - `humanify-ag-testkit` (port the 7 stubs and 27 fakes; then delete
    `humanify-ag-llm/src/stub_server.rs`, which the testkit's server
    replaces);
  - `committed::census` and `committed::e2e`.

## 11. What I could not prove

- That the harness's `HARNESS_KEYS` covers walk.sh's flags exactly. The
  walk is ported in L3, which will meet its header.
- That a `--pipeline-set` entry valid for HEAD is valid for an OLD
  binary. As the design says, an old binary's keys are still checked at
  its first launch.
- The prettier reflow is proven on the 45 committed files and on recorded
  vectors, not on prettier's whole JSON grammar (comments, long strings),
  which no harness record uses.

No CLAUDE.md change is needed for L1 (`cargo dev` is not yet the
documented entry point; L5 rewrites CLAUDE.md).
