# exp221 — the chunk linker's init and namespace names no longer depend on position

STATUS: BUILT (TDD), GATED (all 11 stages pass, see Gate), BOOT SWEEP
(link + Bun 1.4.1, 3 of 3 versions pass both halves), FOUR REAL HOPS
(live model): a seeded chain 2.1.299 (copy of the walk's tree) -> 2.1.300
-> 2.1.301, and a cold chain 2.1.300 (cold) -> 2.1.301. No eval, no walk.
Branch `exp221-stable-chunk-names` from main `d3fed6a41`. No Claude Code
text, chunk name or file name is committed; shapes are described
generically. Scripts and outputs live on this host only, in
`/work/exp221-runs/`.

## Totals

2.1.300 -> 2.1.301, the steady-state hop, scored by the walk's own
scorer (`eval walk-report`) and decomposed by exp220's script:

| | walk `d3fed6a4` (main) | exp221, seeded chain | exp221, cold chain |
| --- | ---: | ---: | ---: |
| diffLn | 397,813 | **79,701** | **71,081** |
| noiseLines | 35,366 | **7,854** | **7,014** |
| nameOnly | 197,856 | **13,780** | **13,980** |
| realExBuild | 326,338 | **56,490** | **53,906** |
| novel | 1,867 | 1,867 (equal) | 1,867 (equal) |
| realLn | 441,069 | 441,069 (equal) | 441,069 (equal) |
| counter-only lines (exp220 decomposition) | 324,058 | **2,226** | **0** |
| exit / boot (both halves) | 0 / OK | 0 / OK | 0 / OK |

The diff of a steady-state hop falls by 80% (seeded) to 82% (cold
chain), and the columns that must not move (`novel`, `realLn`) are equal
on both chains. exp220 measured the counters at 82% of this hop; they
are gone.

The seeded chain starts from the walk's 2.1.299 tree, so its names
include the walk's old `__chunkInit<k>` names: the prior-version transfer
carries them onto the matched inits, and from then on they are carried
by MATCHING, not by position, so they stop renumbering (see "The seeded
chain keeps the old names"). The cold chain shows what a walk started
with this binary looks like: no counter names at all.

## The cause (exp220)

The chunk linker (`unpack/chunk_link/names.rs`) named each lazily loaded
chunk's init `__chunkInit<k>` and its namespace object `__chunkNs<k>`, k
the chunk's position in the written order. One chunk added early
renumbered every later one. Names that start with two underscores and a
word are bundler helpers to the naming stage
(`rename/eligibility.rs`, `is_word_like_dunder`), so the naming waves,
the module step and the prior-version transfer never replaced them. The
counter also reached file names (`chunk-init<k>.js`) and the module
aliases of those files in every importer.

## The design

Each linker name is now an ordinary, minified-shaped binding derived
from the chunk's content: `i<stem>` for the init, `n<stem>` for the
namespace object, `d<stem>` for a default-export binding, where `<stem>`
is six base-62 characters of a content key.

- **Ordinary binding.** The naming stage treats it like any minified
  name. The module step names each init `init<Name>` (as it named Bun's
  own per-module inits before Claude Code's builds split), the
  prior-version transfer carries that name to the next version, and the
  split names the init's file after it. On the real hops not one
  placeholder reached the output: every init and namespace was renamed.
- **Content key, not position** (`analyse.rs`, `content_key`): SHA-256
  of the chunk's code with masked what varies between two builds of an
  unchanged module:
  - every binding and reference (Bun re-minifies them per build);
  - every bunfs path (a chunk's file name is a hash of its contents);
  - comments (every chunk's header names the release);
  - the chunk's import and export lists. These are the edges of the
    chunk graph, not the chunk's code: each chunk lists a side-effect
    import of every chunk it needs, so the list changes whenever the
    graph around it does. Before this was masked, about 44% of chunks
    changed key between 2.1.300 and 2.1.301; most had changed only
    there.
  Property names, strings and code shape are kept.
- **Chunks that share a key** (`names.rs`, `chunk_keys`): chunks with no
  code of their own (their init only runs the inits of the chunks they
  import, about 440 per version) all have the same content. Their key is
  refined by the keys of the chunks they load, in the chunk's own import
  order, repeated up to four rounds while that tells more chunks apart.
  What is left (about 60 per version, chunks whose code and imports are
  both identical up to names) gets a `$k` suffix in written order — the
  only place position still decides.
- **Determinism**: every map is a `BTreeMap` keyed by the key bytes; no
  hash-map iteration order reaches a name.

A placeholder alone (a positional but eligible name) would also have let
the naming stage take over. The content key costs one hash per chunk and
keeps the name stable even where the naming stage leaves it alone (a
name the model is never asked, a fixture with no module step).

### Stability at the linker level

Linking the raw chunks of three versions (`/work/exp221-runs/link/`):

| | 2.1.299 | 2.1.300 | 2.1.301 |
| --- | ---: | ---: | ---: |
| inits | 2,265 | 2,292 | 2,310 |
| inits with a `$k` suffix | 62 | 62 | 63 |
| init names shared with the previous version | — | 1,546 | 1,616 |

Under main, a position counter kept its number on 16% of the init calls
of this hop (exp220). Linker-level sharing is not what the output shows,
though: the naming stage renames every init and the transfer carries the
name, so the output's stability is the hop table above.

### Cold-run cost

A cold run now names about 2,300 more module wrappers (the module step,
8 per call, about 290 more calls) and about 950 more namespace objects
(the waves). Cold 2.1.300 took 6,725 s (`promptGuard.moduleAsked`
44,371). There is no main-binary cold 2.1.300 to compare against, and
other lanes may have been using the LLM server during the run, so this
is an upper bound, not a measured cost of the change.

## The tests (red, then green)

`unpack/chunk_link/chunk_link_test.rs`:

| test | on main |
| --- | --- |
| `a_chunks_init_and_namespace_keep_their_names_when_a_chunk_is_added_before_it` | FAILS (`__chunkInit0` vs `__chunkInit1`) |
| `the_linkers_init_and_namespace_names_are_left_to_the_naming_stage` (each name is eligible) | FAILS |
| `a_chunks_init_name_ignores_comments_and_minified_names` | passes on main (one chunk, position 0 both times); FAILS on this branch with the comment mask removed |
| `a_chunks_init_name_ignores_its_import_and_export_lists` | passes on main (position 0); FAILS on this branch with the import-list mask removed |
| `chunks_with_no_code_are_named_by_what_they_load_not_where_they_are_written` | FAILS on this branch with the refinement off (`KEY_ROUNDS = 0`) |
| `chunks_with_the_same_code_up_to_names_get_distinct_inits` | guard: two inits, never one |

## Gate

`cargo dev check` on `f10138fc2`:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; no group touches the changed files) |
| census:concurrency | PASS |
| census:slop | PASS |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 fixture pairs, 176 launches equal to the golden) |

**No e2e golden changed.** `dev/eval/goldens/e2e.golden.txt` records the
launch lines (binary, arguments, config), not the output trees, and the
change alters no launch. The three code-split fixtures (`bun-chunks`,
`bun-chunk-order`, `bun-chunk-assets`) still pass every e2e check: fresh,
prior and sequential trees deterministic twice, boot with the input's
behaviour. Their trees now hold `i<stem>` names (renamed by the stub
namer as `<name>Renamed`) where they held `__chunkInit<k>`.

## Boot

`/work/exp218-runs/scripts/sweep.sh` with a link probe built on this
branch (link only, then `bun run.cjs` under Bun 1.4.1):

| version | `--version` | `-p "say exactly: boot-ok"` |
| --- | --- | --- |
| 2.1.242 | PASS | PASS |
| 2.1.277 | PASS | PASS |
| 2.1.322 | PASS | PASS |

Every humanified tree of the four real hops passes `cargo dev boot-gate`
(both halves, `claude-opus-4-1`). Peak memory was not recorded.

## The real hops

Launch: the walk's own (`runs/<v>.toml` and `.runcfg.json`: names mode,
index `77b12699…`, `reasoning_effort = "low"`, concurrency 472, the same
model and endpoint), paths moved to `/work/exp221-runs/`. Binary
`f10138fc2` (sha256 `0a1c0e98…`).

| hop | exit | wall s |
| --- | ---: | ---: |
| seeded 2.1.299 (walk copy) -> 2.1.300 | 0 | 958 |
| seeded 2.1.300 -> 2.1.301 | 0 | 782 |
| cold 2.1.300 | 0 | 6,725 |
| cold 2.1.300 -> 2.1.301 | 0 | 956 |

Scored as walks (`eval walk-report` over a walk-shaped folder): cards in
`/work/exp221-runs/walk/cards/` and `/work/exp221-runs/cold/cards/`.

### The transition hop, 2.1.299 -> 2.1.300 (seeded)

| | walk | exp221 |
| --- | ---: | ---: |
| diffLn | 464,263 | 172,831 |
| noiseLines | 52,471 | 30,761 |
| nameOnly | 193,496 | 32,770 |
| realExBuild | 355,899 | 107,777 |
| novel | 2,276 | 2,276 |
| realLn | 474,159 | 474,159 |

Smaller even on the transition: the transfer carries the seed's counter
names onto the inits it matches (2,135 of 2,292 inits in the 2.1.300
tree keep an old `__chunkInit<k>` name; 157 got an `init<Name>` from the
module step), so the old names do not renumber.

### The seeded chain keeps the old names

The prior-version transfer and the post-split reconcile carry any prior
name onto the binding they match, a helper-shaped one included. So a
walk continued from `walk-d3fed6a4-up` keeps its `__chunkInit<k>` names
and its `chunk-init<k>.js` files (624 files in the seeded 2.1.301) — but
frozen: they no longer move. Each hop, the inits the transfer does not
match get an `init<Name>` (107 in 2.1.300, 134 in 2.1.301), so the old
names thin out slowly. The 2,226 counter lines left in 300 -> 301 are
carried old names that landed on a different init than the one they
named in 2.1.300.

I did not add a rule that refuses to carry a helper-shaped prior name:
real helpers of that shape (`__esm`, tslib's `__awaiter`, …) are
legitimately carried, so the rule would have to name the linker's old
spelling — a migration rule for one walk (no backwards compatibility).
A walk started with this binary never has the old names (the cold
chain).

One oddity of the seeded chain: in 37 cases a post-split pass (the
diff-guided reconcile, as far as I read it) gives a split
file's local init the old counter name while its export keeps the new
one (`exports.initX` reads the local `__chunkInit<k>`), so two files can
declare the same local name. It is valid (each file is its own module)
and boots; it comes from carrying old names, not from the linker.

### Decomposition of 2.1.300 -> 2.1.301

exp220's `decompose2.py` (GNU diff per path, `src/` + `vendor/`):

| | lines | instrument | counters | moves | other names | rest |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| walk (main) | 397,445 | 14 | 324,058 | 20,134 | 16,190 | 37,049 |
| exp221, seeded | 79,455 | 12 | 2,226 | 19,334 | 25,350 | 32,533 |
| exp221, cold chain | 70,809 | 10 | 0 | 17,110 | 23,256 | 30,433 |

Other names rise by about 9,000 lines on the seeded chain and 7,000 on
the cold chain. On the seeded chain part of it is inits leaving an old
counter name for an `init<Name>` (a one-time move per init). On the
cold chain it is inits the module step names again: 2,308 of 2,325
init names in the 2.1.300 tree are kept in 2.1.301, and the ~17 that
change carry their callers with them (the largest has 363 call lines:
a shorter wording of the same name). These are modules the transfer did
not match, the same mechanism that renamed Bun's own inits before the
split; the module aliases of their files follow them. Moves and the rest are a little
lower than the walk's.

### The cold chain

Cold 2.1.300 (no prior) names every init `init<Name>` (2,292 of 2,292)
and writes no counter-named file (2,289 files under `src/`, against the
walk's 2,084 with 637 `chunk-init<k>.js` / `chunk-ns<k>.js`). No
`i<stem>` / `n<stem>` placeholder reached either tree. The 2.1.301 hop
on it asked the module step about as many modules as the walk's hop did
(`promptGuard.moduleAsked` 504 against 499).

## What this does not settle

- One hop of each chain, one draw each. The walk's 300 -> 301 is one draw
  as well, and the bands (rule 11) are for eval totals, not walk hops;
  but an 80% fall against an effect exp220 measured at 82% is far
  outside any draw.
- The giant file and GNU diff's heuristic (exp220's drivers 2 and 3) are
  untouched.
- A walk over the whole split era with this binary would say what the
  per-hop numbers are at scale; it needs Andrew's OK.
