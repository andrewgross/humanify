# exp243 — effects ratchet: the checks for "functional core, imperative shell"

STATUS: BUILT (red seen for every new check), GATED (`cargo dev check`, all
13 stages). Branch `exp243-effects-ratchet`, not merged. Step 0 of plan
lane 2 (functional core, imperative shell) in the simplification campaign.
No product logic changed: product files gained only comments and `#[allow]`
attributes, so the release binary behaves exactly as before; no neutrality
run was needed.

## Totals

- **Every effect in the product crates is now listed, and the list for the
  pure crates can only shrink.** The new gate stage `census:effects`
  (pass/fail) lists 416 sites in `dev/census/effects-allowlist.json`. A new
  unlisted site fails, and so does an entry whose site is gone.
- **The pure crates are `humanify-ag-core` and `humanify-ag-model`.** Today
  they hold 67 effect sites to migrate (core 65, model 2), plus 4 sites in
  the profiling module, which is allowed to read the clock.
- **184 non-private functions return `Result<_, String>`**: core 128,
  index 42, cli 14, model 0, llm 0. They are listed as `legacy`.
- **Clippy now bans effects and dropped errors in core and model.** Each
  crate has its own `clippy.toml` with the bans. Every current site carries
  a tagged allow. The tag is `// io: <reason>` for an effect and
  `// default: <reason>` for a Result's error turned into a value. Clippy
  found 180 sites in non-test code: 89 effects and 91 dropped errors. They
  sit in 116 functions. Tests get one allow per test file (43 files).
- **The plan's open assumption is confirmed: clippy does not merge
  configs.** For a crate that has its own `clippy.toml`, clippy ignores the
  root one. Without a check, the root's env and concurrency bans would
  silently stop applying to core and model. The census now checks that
  each pure crate's config contains every root entry and that the two
  configs match.
- **The pure crates' dependencies are pinned to an allowed list.** Today
  that is serde, serde_json, sha1/sha2, rayon, the oxc crates, dragonbox
  and model. Build and target-specific dependencies are refused.

Raw diff lines against origin/main `507ac1fc4` (comments and blank lines
included):

| | added | removed |
| --- | ---: | ---: |
| product code in core and model (tags and allows only, plus the 2 `lib.rs` deny lines) | 256 | 0 |
| product tests (one tagged inner allow per core / model test file; the env guard's new owner) | 103 | 2 |
| the two crate `clippy.toml` files | 196 | 0 |
| harness code (census, shared allowlist module, gate stage, pre-commit, loader, verb) | 1,265 | 158 |
| harness tests | 473 | 35 |
| `dev/census/effects-allowlist.json` | 2,501 | 0 |
| docs (CLAUDE.md stage list, responsibility row, concurrency tag note) | 11 | 4 |

The concurrency census's shared parts (tree walk, verb inputs, entry
matching, the allow-comment check) moved into `census/allowlist.rs` and
`concurrency::Tree`, so the two censuses share them instead of copying
them. That move is most of the 158 removed harness lines.

## Gate

`cargo dev check` on the final tree:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS (the new crate configs are in force: core and model are clean only because every current site is tagged) |
| census:clones | REVIEW (advisory; nothing new in production code: the census twins this lane first made were unified into `census/allowlist.rs`) |
| census:concurrency | PASS (124 sites; the allow-comment check now also covers `io:` / `default:`, inner and multi-line allows) |
| census:effects | PASS (new; 416 sites, 416 entries) |
| census:slop | PASS |
| census:comments | PASS (no new findings) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS |

Red seen before green, for each new check: the precommit and gate-stage
tests failed before the code existed; for the census, the matcher, the
config, lib.rs and dependency checks, and the new tags, each test failed
on a mutant with that check switched off; the multi-line allow test failed
before its fix; the dev-file loader test failed until the allowlist had
its loader.

## What each check bans

### Clippy, in `crates/humanify-ag-{core,model}/clippy.toml` (both identical)

The root's entries, repeated word for word: the complexity ceiling, the
env-reader ban and the concurrency bans. Then the effect bans:

| group | banned |
| --- | --- |
| disk | `std::fs::{read, read_to_string, write, read_dir, create_dir(_all), remove_file, remove_dir(_all), copy, rename, hard_link, canonicalize, metadata, symlink_metadata, read_link, set_permissions}`, `File::{open, create, create_new}`, `OpenOptions::open`, `Path::{exists, try_exists, is_file, is_dir, is_symlink, metadata, symlink_metadata, read_dir, read_link, canonicalize}` |
| processes | `Command::new`, `process::{exit, abort, id}` |
| environment | `env::{temp_dir, current_dir, set_current_dir, current_exe, args, args_os, vars, vars_os}` (plus the root's `var`, `var_os`), `std::path::absolute` (it reads the working directory) |
| clock | `Instant::now`, `SystemTime::now` |
| threads | `thread::spawn`, `thread::Builder::spawn` (core's `par` uses scoped threads, which the concurrency census covers) |
| terminal | `io::{stdout, stderr, stdin}`; `println!`, `print!`, `eprintln!`, `eprint!`, `dbg!` (`disallowed-macros`) |
| dropped errors | `Result::{ok, unwrap_or, unwrap_or_default, unwrap_or_else}` |

`Option::unwrap_or*` stays allowed. Most of those calls are normal
idioms: a default for an empty aggregate or a missing key.

Each pure crate's `src/lib.rs` also has
`#![deny(clippy::map_err_ignore, clippy::let_underscore_must_use)]`. That
denies `map_err(|_| ..)` and `let _ = <Result>`. These two lints live in
the source because the crates' `[lints]` table is the workspace's, and
Cargo does not allow adding to it per crate.

An allow of any of these lints must carry a reason tag on its own line or
in the three lines above it. The accepted tags are `// concurrency:`,
`// env:`, `// io:` and `// default:`. The concurrency census's
allow-comment check enforces this. It now also covers inner `#![allow]`
attributes and the lints `disallowed_macros`, `map_err_ignore` and
`let_underscore_must_use`.

### `census:effects` (pass/fail; also in the pre-commit hook on staged `.rs`)

It scans the non-test sources of the five product crates (cli, core,
index, llm, model) with the concurrency census's scanner. Comments,
string literals and `use` lines are blanked, `#[cfg(test)]` items are
removed, and a site is keyed as (file, enclosing function or item,
construct). It looks for:

| construct | what counts |
| --- | --- |
| `fs` | `fs::*`, `File::open/create`, `OpenOptions`, `.exists()`, `.is_file()`, `.is_dir()`, `.read_dir()`, `.metadata()`, `.canonicalize()` … |
| `process` | `Command::new`, `process::exit/abort/id` |
| `env` | `env::var*`, `env::temp_dir`, `env::current_dir`, `env::args*`, `env::current_exe`, `path::absolute` … |
| `clock` | `Instant::now`, `SystemTime::now` |
| `thread` | `thread::spawn`, `thread::Builder` |
| `stdio` | `println!` and friends, `dbg!`, `io::stdout/stderr/stdin`, a bare `stdout()`. A child's `.stdout(..)` setting does not count. |
| `net` | `TcpStream`, `TcpListener`, `UdpSocket`, `reqwest::*` |
| `static-state` | `static mut`, `thread_local!`, a `static` whose type holds an `Atomic*`, `Mutex`, `RwLock`, `OnceLock`, `OnceCell`, `Cell`, `RefCell` or `UnsafeCell`. A `LazyLock` constant does not count. |
| `result-string` | a `pub` / `pub(crate)` / `pub(super)` fn whose return type holds a `Result<_, String>` |

Each entry has a role. `legacy` is for an effect in a pure crate or a
`Result<_, String>` in any product crate. `observation` is for the
profiling module's files only. `shell` is for an effect in cli, index or
llm, which is an inventory. A role that does not fit the file fails.

It also checks:

- each pure crate's `clippy.toml` holds every entry and value of the root
  `clippy.toml`, and the pure crates' configs are equal;
- each pure crate's `lib.rs` keeps the deny line;
- each pure crate's `[dependencies]` stays within
  `effects::rules::PURE_CRATES`, with no `[build-dependencies]` or
  `[target]` tables.

What it cannot see:

- an effect reached through a helper in another crate (core calling a cli
  function cannot happen: core does not depend on cli);
- a Result's error dropped by `if let Ok` or `let Ok(..) else`;
- whether a `legacy` entry was added on purpose. The JSON diff is the
  review.

## The starting allowlist (416 entries)

| crate | role | effect sites | `Result<_, String>` |
| --- | --- | ---: | ---: |
| core | legacy | 65 | 128 |
| core | observation (profiling) | 4 | — |
| model | legacy | 2 | 0 |
| cli | shell / legacy | 63 | 14 |
| index | shell / legacy | 75 | 42 |
| llm | shell | 23 | 0 |

Core's legacy effect sites by module: unpack 16, finish 13, artifact_dump 8,
naming 6, libindex 5, place 5, scratch 5, subprocess 4, emit 1, libdetect 1,
prior 1 (the `PRIOR_PARSES` counter static). Model has two:
`time::iso_now` reads the clock, and `js::node_path_resolve` reads the
working directory through `std::path::absolute`. The plan had not listed
the second one.

`Result<_, String>` by module (top): core libindex 25, finish 19, unpack 18,
naming 11, format 8, place 8, rename 7; index merge 7.

Clippy's tagged allows in non-test code: 57 functions are tagged
`// io: legacy …`, 64 are tagged `// default: legacy, untriaged …` (some
functions carry both tags), and 11 profiling functions are tagged
`// io: the profiling clock …`. The 91 dropped-error sites are
`Result::ok` 42, `let _ =` 19, `unwrap_or_else` 11, `unwrap_or` 10,
`unwrap_or_default` 6, and `map_err(|_| ..)` 3. None of them is triaged
yet. The tag says so, and step H below triages them.

The allows sit on the enclosing function, not on each call, because Rust
has no stable way to put an attribute on an expression. The cost: inside a
function that already has an allow, clippy will not flag a second call of
the same kind. The census still keys sites by (file, function, construct),
so a new kind of effect in that function, or any effect in a new function,
still fails.

## What I learned that changes the migration plan

1. **Model is almost pure: 2 effect sites, 0 string errors and 7
   dropped-error sites.** It can be finished first and alone: the clock
   becomes a parameter of `iso_now`, and the working directory a parameter
   of `node_path_resolve`. After that, make model strict: change one line
   in `rules.rs` so a pure crate with no legacy entries refuses the
   `legacy` role.
2. **The cheapest core wins are the moves**, not the rewrites.
   `subprocess.rs` and `scratch.rs` (9 entries, including three kinds:
   thread, clock and env), the `diff` call in `reconcile/hunks.rs` and the
   webcrack shim are self-contained.
3. **`Result<_, String>` is the biggest list (184).** It needs its own
   lanes per crate. Core's three heaviest modules (libindex, finish,
   unpack: 62) line up with the disk sites, so typed I/O errors land
   naturally with the `PriorTree` / `OutputTree` steps.
4. **Test code needs file-level allows.** Clippy has no allow-in-tests
   option for `disallowed_methods`, so the 43 test files each carry one
   tagged inner allow. A later step can move fixture reading into a small
   test helper module and drop most of them.
5. **`par::ORDERED` is already gone** (the naming-tiers lane removed it).
   The global-state entries in core are `prior::PRIOR_PARSES`, the
   profiling recorder (`observation`) and the counter static in
   `scratch::temp_dir`.

### Refined step order (each a branch, each `cargo dev check`; each step deletes its allowlist entries and tags)

| # | step | entries it deletes (today) | proof |
| --- | --- | --- | --- |
| A | model strict: clock and cwd as parameters; triage its 7 dropped errors; refuse `legacy` in model | model 2 effects + 7 default tags | warm neutrality (the stats timestamp and resolved paths must stay byte-identical) |
| B | processes out of core: `subprocess.rs`, `scratch.rs` → a small shell crate (index, dev and cli import it); `Differ` trait for `reconcile/hunks.rs`; the shell runs the webcrack shim | about 14 core entries (process, thread, clock, env, static) | warm neutrality |
| C | typed errors, core first: one error enum per stage with `Display` that reproduces today's text; index `NpmError` instead of matching error text | core 128, then index 42, then cli 14 `result-string` | warm neutrality, plus a golden of the error texts the e2e failure fixtures print |
| D | `PriorTree`: the shell loads the prior tree once; delete `find_prior_tree_root`, `load_prior_vendor`, `prior_vendor_naming`, `read_ledger`, the split's prior reads and `PRIOR_PARSES` | unpack/bun, emit, naming/vendor_bodies, place/ledger + placement_dump reads, prior static: about 12 | warm neutrality |
| E | library index `ReadAt` trait | libindex 5, libdetect 1 | warm neutrality |
| F | in-memory `OutputTree`: unpack returns files, finish transforms them, the shell writes once (also on the exit-1 path) | finish 13, unpack writes, vendor_bodies writes: about 20 | warm neutrality + e2e (fresh, prior, failure fixtures); the riskiest step, alone |
| G | diagnostics writers out of core: `artifact_dump`, `placement_dump` (keep `DispatchLog` streaming) | artifact_dump 8, placement_dump 2 | `--dump-artifacts` byte diff + warm neutrality |
| H | dropped-error sweep: each of the 91 sites is propagated or gets a precise `// default: <why absence is data>` on the smallest item; drop the function-level `legacy` allows | all `// default: legacy, untriaged` tags | warm neutrality (a byte-identical run proves none of the converted paths fired on the pairs) |
| I | strict core: once core has no `legacy` effect entries, refuse the role there, so the only remaining tag is profiling's `observation` | — | the census |

A, B and E are small and can run in parallel after this lane. C can start
anywhere but should go module by module. F waits for D.

## How to read a red

- `EFFECTS CENSUS: N unlisted site(s)`: the message names the file, the
  function and the construct, and prints the entry to paste. In core or
  model the advice is to move the effect to the shell. Listing it as
  `legacy` makes a list that should only shrink grow, and the JSON diff
  shows that.
- `stale allowlist entr(y|ies)`: you migrated a site. Delete its entry.
- `lacks the root entry` / `differs from`: someone edited only one
  `clippy.toml`. Copy the change to every pure crate's config.
- `dependency X is not on the pure crate's allowed list`: an I/O, async or
  CLI dependency belongs in a shell crate.
