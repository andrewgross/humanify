# exp218 — run a linked Bun code-split program in Bun's real order

STATUS: BUILT (TDD), GATED (all 11 stages, see Gate), LINK + BOOT SWEEP
2.1.242-2.1.322 (68 of 68 versions boot both halves, from 5), MATCHING
MEASURED on 2.1.241 -> 2.1.242 (three stub runs, see Matching). MERGED
WITH THE BUN-SHAPE VARIANT (Andrew's plan asked for the strict rule; the
variant is equally order-free, boots the same 68 of 68 and costs much less
matching, so it is what main carries; the variant commit's full gate: all
11 stages pass). Branch `exp218-bun-chunk-order` from main `6477bf247`
(strict rule), variant branch `exp218-variant-bun-shape` on top of it. No eval, no walk, no
real-model run. Every number naming Claude
Code comes from this host's inputs and runs; no Claude Code text is
committed.

## Totals

| | main `6477bf247` | exp218 |
| --- | ---: | ---: |
| versions linked (2.1.242-2.1.322 on this host) | 68 | 68 |
| `bun run.cjs --version` passes | 19 | **68** |
| `bun run.cjs -p "say exactly: boot-ok" --model claude-opus-4-1` passes | 5 | **68** |
| gate (`cargo dev check`, 11 stages) | green | green |
| 2.1.241 -> 2.1.242 stub: still ambiguous / functions asked | 4,202 / 13,553 | 5,127 / 14,020 (variant: 4,546 / 13,634) |

`run.cjs` is the RAW linked program (the linker's output, no naming, no
split) with its embedded files beside it, run by Bun 1.4.1.

## The problem

exp217 linked every version and ran the raw linked program under Bun. Two
failures (exp217's probe ran Bun 1.3.14; this sweep uses Bun 1.4.1 and
reads the same two):

1. From 2.1.248 to 2.1.268 the prompt fails: `<x> is not a function`,
   thrown inside Bun's `__toESM` interop helper.
2. From 2.1.269 to 2.1.322 the program fails at load: `<helper> is not a
   function`, a top-level `var m = helper((exports) => …)` statement.

## What causes each failure (confirmed before fixing)

Both come from one design choice in the exp216 linker: a lazily loaded
chunk kept its "pure" declarations at the top level and moved the rest
into its init — but it decided purity per DECLARATOR and placement per
STATEMENT, and it ordered the init's own pieces by hand.

**Failure 1 — external loads ran before the chunk's imports.** A lazy
chunk that default-imports a Node built-in (`import os from "os"`) gets
the linker's own Bun-CommonJS shape, `os = __toESM(require("os"))`, using
the first `__toESM` helper written in the program. That helper is a pure
arrow, so it stayed at the top level, but the values it reads
(`Object.create`, `Object.getPrototypeOf`, …) are not pure, so they moved
into the runtime chunk's init. The importing chunk's init was written as
"external loads, then the inits of the chunks I import, then my body" —
so `__toESM` ran before the runtime chunk's init had set what it reads.
(From 2.1.248 the runtime chunk is reached only lazily; before that it
was eager and the bug could not show.) The decision at fault:
`emit::chunk` putting `lazy_loads` before the static deps' init calls.

**Failure 2 — a factory call stayed, its helper moved.** From 2.1.269 a
chunk carries a dependency that ships pre-bundled, with its own copy of
the CommonJS helper declared in ONE `var` together with values that read
`Object` (`var {create, …} = Object, hasOwn = …, helper = (cb, mod) => ()
=> …;`), and its wrapped modules in later statements
(`var m = helper((exports) => …)`). The whole first statement moved into
the init (any impure declarator moved the statement), while `var m =
helper(…)` stayed at the top level because `graph::is_factory` saw that
`helper` has the thunk-factory shape — without checking that the helper's
own statement had stayed. The decision at fault: `graph::moves` /
`graph::pure` (per statement) against `graph::is_factory` (per binding).

## What was built

Option 1 of the approved plan: a lazily loaded chunk follows ESM
evaluation exactly (`graph::placement`).

- Every statement of a lazily loaded chunk runs in its init, in order.
  Its top-level names are declared at the top level (`var x;`) and
  assigned in the init. Classes, literals, objects, arrows and `export
  default` expressions all move now (the "pure stays at the top level"
  rule is gone).
- An init runs, once: the inits of the lazy chunks it imports, then its
  external loads, then its body (Bun's rule; ESM evaluates a module's
  imports before its body).
- When a chunk's external default import calls a `__toESM` whose chunk is
  lazy and not one this chunk imports (eager code can meet one), that
  chunk's init is called first: ESM never calls the helper there, the
  linker does, so the linker orders it.
- What stays at the top level is only what is ORDER-FREE — evaluating it
  earlier cannot be observed:
  - function declarations (ESM hoists them at instantiation, before any
    module body runs);
  - a thunk factory, `var h = (…) => () => …` with `h` never written
    (building a closure reads nothing: Bun's `__commonJS` / `__esm`
    helpers);
  - a factory call, `var x = h(fn, …)` (every argument a function), when
    `h` is a thunk factory written EARLIER in the program — calling it
    only builds a thunk. These are Bun's CommonJS modules and nested
    `__esm` inits.

  - (the variant, merged) a declarator whose value is a literal or a
    closure (function or arrow expression): evaluating it reads no
    binding and runs nothing. Bun's own bundler keeps these at the top
    level too.

  A `var` statement whose declarators differ is split: the order-free
  ones are written at the top level, the rest are assigned in the init.

  The third case is the one exception the plan did not name. Without it
  every CommonJS module in a lazy chunk (7,337 factory declarators in
  2.1.242, 1,650 in 2.1.322) would become an assignment inside an init,
  and the bun adapter extracts a module only from a top-level `var x =
  helper(fn)` (`modules::classify_bun_modules`): `vendor/` would lose
  them. Bun's single-file bundles up to 2.1.241 keep these at the top
  level too.
- Refused, loudly: a lazy chunk's statement that declares a top-level
  `var` inside a nested block (`if (x) { var y }`; moved into the init,
  `var y` would become the init's own). None in any release 2.1.242-322.
- Fixed on the way: a lazy chunk with a bare `import "m"` beside other
  external imports wrote `var ,x;` (a syntax error); the empty name is
  dropped. (Latent on main; the `__toESM` init call above uses the same
  path.)
- `export var/let/const/class` keep their own kind (main kept every
  `export <declaration>` at the top level). Bun writes export lists, so
  this changes nothing in Claude Code.

Eager chunks are unchanged: written in ESM postorder at the top level.

On linked 2.1.242: 1,215 inits on main, 1,321 now (106 more chunks run
something at load: literals, classes, objects that used to stay at the top
level); the program is 32.39 MB on main, 32.86 MB now (hoisted `var x;`
beside each assignment).

## Tests (red, then green)

Unit tests (`chunk_link/chunk_link_test.rs`, the linked program run by
Node), each seen failing on main first:

| test | on main |
| --- | --- |
| `a_default_import_of_a_built_in_runs_after_the_interop_helpers_init` (failure 1) | `TypeError: h is not a function` |
| `an_eager_default_import_runs_the_lazy_helpers_init_first` | `TypeError: h is not a function` |
| `a_factory_call_waits_for_a_helper_declared_beside_moved_values` (failure 2) | `TypeError: J is not a function` |
| `a_lazy_chunk_runs_every_statement_in_its_init_but_functions_and_factories` | literals / class stayed at the top level |
| `a_lazy_chunks_var_declared_in_a_nested_block_is_refused` | linked, `var y` kept at the top level |
| `a_lazy_chunks_side_effect_import_declares_no_empty_name` | `SyntaxError: Unexpected token ','` |

`a_lazy_chunks_pure_declarations_stay_at_the_top_level` is renamed
`a_lazy_chunks_functions_stay_at_the_top_level` (its assertions hold).

E2E fixture `test/e2e/fixtures/bun-chunk-order` (new; Bun 1.4.1,
`build.sh`, generator `bun-apps/source/gen.mjs order`): a synthetic app
whose entry loads both modules through `import()` and requires nothing,
so Bun's runtime chunk loads lazily; one module default-imports a Node
built-in and a CommonJS module (failure 1's shape), the other imports a
pre-bundled dependency with its own CommonJS helper declared beside
values that read `Object` (failure 2's shape). On main its split tree
fails at load (`TypeError: <helper> is not a function`, failure 2 hides
failure 1 behind it; the unit tests hold each alone); with exp218 the
fresh, prior and sequential trees print the input's output.

## Gate

`cargo dev check` in the worktree at `572ced4f5` (the code commit): ALL 11
STAGES PASSED (`/work/exp218-runs/gate2.log`; the first run failed
`rust:fmt`, fixed by `cargo fmt`).

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW — no finding in the changed code (the one `chunk_link` finding, the two test runners `run` / `run_with`, is exp217's) |
| census:concurrency | PASS (no new concurrency site) |
| census:slop | PASS (humanify-ag-core erosion 32.6% -> 32.6%, -0.1pp; clone share unchanged) |
| rust:unit | PASS — 6 new linker tests, each seen red on main (above); the matching pin tests list `bun-chunk-order` beside the other fixtures added after the pins (seen failing without it) |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS — every fixture as before (the bun fixtures' trees still boot with their inputs' behaviour), plus `bun-chunk-order`. The launch golden gains exactly that fixture's 64 lines, re-recorded from a live run; no other line moved (launches are commands, so the linker change itself moves none) |

## The boot sweep

Every version present on this host from 2.1.242 to 2.1.322 (68: 2.1.244,
249, 253-256, 262, 264, 279, 297-298, 313 and 320 are not unpacked here),
linked by a probe over the crate (`read_input` + `write_embedded_files`,
as exp217's), the linked file written as `run.cjs` with `embedded/`
beside it, run by Bun 1.4.1: `bun run.cjs --version` and `bun run.cjs -p
"say exactly: boot-ok" --model claude-opus-4-1`. Script and logs:
`/work/exp218-runs/scripts/sweep.sh`, `/work/exp218-runs/sweep-before.txt`
(main), `/work/exp218-runs/sweep-after.txt` (exp218).

| versions | count | `--version` main | `--version` exp218 | prompt main | prompt exp218 | main's failure |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 2.1.242-2.1.247 | 5 | 5 | 5 | 5 | 5 | — |
| 2.1.248-2.1.268 | 14 | 14 | 14 | 0 | 14 | failure 1 (`__toESM`) |
| 2.1.269-2.1.322 | 49 | 0 | 49 | 0 | 49 | failure 2 (at load) |
| **all** | **68** | **19** | **68** | **5** | **68** | |

A link is small: 2.1.322, the largest, takes 5.0 s and peaks at 273 MB.

## Cross-check: Bun's own bundler (option 2)

`bun build` (Bun 1.4.1, splitting off, a resolve plugin mapping
`/$bunfs/root/<chunk>.js` beside the entry;
`/work/exp218-runs/scripts/bunbuild.mjs`) on the unpacked chunks:

| version | `--format esm` | `--format cjs` |
| --- | --- | --- |
| 2.1.245 | bundles; `--version` and the prompt both pass | bundles; fails at load: `import.meta is only valid inside modules` |
| 2.1.251 | bundles; `--version` passes; the prompt fails: `Cannot find module '/$bunfs/root/chunk-<hash>.js'` | — |
| 2.1.269 | as 2.1.251 | — |
| 2.1.322 | bundles; `--version` fails: the same missing chunk | — |

So Bun's bundler can rebuild one file only while every chunk is reached
by `import` / `import()` (up to 2.1.249). From 2.1.250 Claude Code also
loads chunks with `import.meta.require("/$bunfs/root/…")`, which Bun's
bundler leaves as a run-time require (so do the text reads from 2.1.246).
It is not a replacement and was not made a dependency.

Its ORDER, read off 2.1.245's ESM output, is the same evaluation rule:
every lazily reached chunk becomes `var init_<chunk> = __esm(() => {…})`
whose body first calls the inits of the chunks it imports; function
declarations stay outside. Where it differs from exp218's linker, both
choices are order-free and so cannot be observed:

- Bun keeps every declarator whose value is a literal or a closure at the
  top level (`var a, b, f = (x) => …, n = 300;`, with `a = Object.create`
  assigned in the init) — what main's "pure" rule copied. exp218 moves
  them (the approved plan's simple rule).
- Bun moves the factory calls (`m = helper(fn)`: 1,835 of them in the
  init bodies, none at the top level). exp218 keeps them at the top level,
  where Claude Code's single-file bundles had them and the bun adapter
  reads them.

## Matching: 2.1.241 -> 2.1.242

Stub runs (no LLM; matching is decided before any answer, exp209), the
walk's prior `/work/walk-73d01430-up/trees/2.1.241/.humanify/humanified.js`
and library index `77b12699…`, split on, run by hand against `cargo dev
stub-llm serve` with `--stats-json` (exp216's protocol; `cargo dev run
stub` writes no stats file, so it was run first and then repeated by
hand). The three legs ran in parallel: 23-27 GB peak each, 26-28 min.
Outputs: `/work/exp218-runs/hand-242-{main,exp218,variant}/`.

| column | exp216 (`e10432eea`) | main `6477bf247` | exp218 `572ced4f5` (strict) | variant `ed96d4f59` (Bun's shape) |
| --- | ---: | ---: | ---: | ---: |
| prior functions present (of 91,971) | 84,318 | 84,316 | 84,370 | 84,409 |
| prior functions unmatched | 7,653 | 7,655 | 7,601 | 7,562 |
| still ambiguous | 4,202 | 4,202 | **5,127** (+925) | 4,546 (+344) |
| structural hash unique | 47,353 | 47,351 | 47,357 | 47,371 |
| enclosing statement resolved | 17,113 | 17,113 | 16,239 (-874) | 16,846 (-267) |
| fresh functions | 107,937 | 107,937 | 108,043 | 108,014 |
| carried | 73,345 | 73,343 | 73,190 (-153) | 73,623 (+280) |
| close-matched | 3,955 | 3,957 | 4,025 | 3,969 |
| already named | 6,771 | 6,771 | 6,053 (-718) | 6,240 (-531) |
| asked the model | 13,553 | 13,553 | 14,020 (+467) | 13,634 (+81) |
| module bindings carried / asked | 24,368 / 3,234 | 24,363 / 3,234 | 23,950 / 3,834 (+600) | 24,067 / 3,390 (+156) |
| CommonJS modules extracted (`_bun-modules.json` factories) | — | 1,915 | 1,915 | 1,915 |
| `vendor/` files / `src/` files | — / 6,077 | 2,446 / 6,077 | 2,403 / 6,229 | 2,537 / 6,050 |
| exit | 0 | 0 | 0 | 0 |

(Deltas are against main.) Main reproduces exp216 to within 2-5 per
column (exp217's 9 path strings sit in between). Module extraction is
unchanged in both new legs: every factory declaration stays where the bun
adapter reads it.

**The strict rule costs matching.** Moving every literal and closure
declarator into an init takes away the top-level statements 2.1.241 had
(Bun's bundler kept them there), so the "enclosing statement" tier
resolves 874 fewer functions: 925 more stay ambiguous, 467 more functions
and 600 more module bindings go to the model.

**The variant (Bun's own shape) recovers most of it.** It also keeps a
declarator at the top level when its value is a literal (number, string,
boolean, null, a template with no expressions, `!0` / `void 0` / `-1`)
or a closure (function or arrow expression): order-free for the same
reason a function declaration is (evaluating it reads no binding and runs
nothing). Against main: 344 more ambiguous (not 925), 81 more functions
asked (not 467), 156 more module bindings asked (not 600), and 280 more
functions carried. It boots exactly as the strict rule does: the raw linked
program passes both halves on all 68 versions (`/work/exp218-runs/sweep-variant.txt`).

The variant is commit `ed96d4f59` on its own branch,
`exp218-variant-bun-shape` (worktree `/work/exp218-runs/variant-src`),
on top of this branch: `analyse::is_inert` and one line in
`graph::order_free`, and the strict-rule unit test changed to expect a
literal and a closure to stay at the top level. Its full gate (`cargo dev
check`, 11 stages) passes. It is the version merged to main: same boot
result, much smaller matching cost.

## Open

1. **The 344 ambiguous functions the variant still adds** on 2.1.241 ->
   2.1.242 are not broken down; most are likely the chunk re-ordering the
   re-ordering experiment (todo) targets.
2. **A pipeline tree past 2.1.250 is still unbooted.** This sweep runs the
   RAW linked program; the pipeline's bun adapter re-links the factories
   and the finish writes its own helper file. A walk tree from 2.1.248+
   on this linker is the next check (a walk needs the coordinator's slot).
3. **The `humanify-ag match` verb is slower than the pipeline on a
   Claude Code bundle**: on 2.1.242 it was stopped after 70-80 minutes
   unfinished (~11 GB, ~1.2 cores), where the whole stub pipeline takes
   26 minutes. Worth a look before a harness lane relies on it for a
   large input.
4. **exp217's open item 2 (the cold 2.1.321 stub run's memory)** is not
   touched here.
