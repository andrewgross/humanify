# exp216 — link a Bun code-split input back into one program

STATUS: BUILT (TDD), STUB-MEASURED (241 -> 242 -> 243, both trees boot),
GATED (all 11 stages), TWO REAL RUNS of 2.1.241 -> 2.1.242 (each exposed a
linker bug, both fixed; the fixed binary was not rerun on the real model),
NOT MERGED. Branch `exp216-bun-chunk-link` from main `16b15d005`. (Commit
hashes are after a history rewrite that removed one quoted Claude Code
import line from a doc comment; the code of each named commit is
unchanged.) No eval
and no walk were run (both need the coordinator's slot). Every number
below that names Claude Code comes from this host's inputs and runs; no
Claude Code text is committed.

## The problem

From Claude Code 2.1.242 (Bun 1.4) the app ships as Bun code-split ESM.
`binary-decompiled/index.js` is a ~20 KB loader; the code lives in
1,375-2,900 sibling chunks (`chunk-<hash>.js`, and `_NNN.js` in 2.1.246-247)
that import each other by absolute specifiers
(`import{a as b}from"/$bunfs/root/chunk-<hash>.js"`, side-effect
imports, `import("/$bunfs/root/…")`, and from 2.1.250
`import.meta.require("/$bunfs/root/…")`). The pipeline takes one file and
follows no imports, so on the 2.1.241 -> 2.1.242 walk hop it read only the
loader and refused the prior: "only 1908 of 91971 prior functions have a
matching structural hash".

## What was built

`humanify_ag_core::unpack::chunk_link` — stage 2's input side. Its
`read_input` is now the one reader of a run's input file (the pipeline and
the `detect`, `unpack`, `discover` and `match` verbs). A file that is not
this layout comes back byte-for-byte. This layout is detected (an ESM entry
under `// @bun`, not `@bun-cjs`, that imports at least one
`/$bunfs/root/*.js` sibling present beside it), its import graph followed
from the entry (never by listing the directory), and the chunks linked
into the shape of the single-file `@bun-cjs` bundle that came before:

- every import of a chunk becomes a direct reference to the exporting
  chunk's binding (re-export chains followed); top-level names that would
  collide, or be captured by a nested scope of an importing chunk, are
  renamed to names no chunk uses;
- the entry's static closure runs at the top level in ESM order; a chunk
  reached only lazily keeps its pure declarations at the top level
  (functions, literals, and calls of Bun's `__esm` / `__commonJS` helpers,
  so the bun adapter still finds every factory) and moves everything else
  into an init with Bun's `__esm` shape, which first runs the inits of the
  lazy chunks it imports;
- `import(chunk)` becomes `Promise.resolve().then(() => (init(), ns))`,
  exactly what Bun writes for an `import()` inside one file;
  `import.meta.require(chunk)` becomes `(init(), ns)`; `ns` is the chunk's
  namespace object in Bun's `__export` form;
- `import.meta.url / dirname / dir / filename / path / file / main /
  require / env` become their CommonJS equivalents, and Bun's runtime
  alias `var k=import.meta.require` becomes the wrapper's own `require`;
  external imports take Bun's CommonJS shape (`ns=require(m)` read as
  `ns.name`; a default import as `.default` of `__toESM(require(m))`),
  loaded at the top of an eager chunk and in the init of a lazy one;
- the chunks are written in one depth-first walk from the entry (eager
  chunks keep ESM order; a lazy chunk lands beside its loader).

It refuses, loudly, what it cannot link faithfully: a bunfs import whose
file is missing, top-level await, `export *`, a bare or unknown
`import.meta`, a name a chunk does not export. The stats file gains an
`inputLink` block (chunks linked, eager / lazy, inits, lazy loads, names
renamed).

Why before detection and not as a fifth adapter in `choose_adapter`:
detection, the library recipe (index kind), the run-time index, the
split's markers and the bun adapter all read the input text. After
detection, five of them would read the 20 KB loader. Linked first, the
whole pipeline runs unchanged, and the toolchain the linked 2.1.242 gets is
the same as 2.1.241's, piece for piece.

## Step 1 — feasibility (read-only, before building)

| measure | 2.1.241 (single file) | 2.1.242 loader only | 2.1.242 linked |
| --- | ---: | ---: | ---: |
| parses (script) | yes | yes | yes, 0 errors |
| `function` nodes (oxc) | 58,741 | — | 69,694 |
| arrow nodes | 55,323 | — | 61,369 |
| prior presence (the refusal check) | — | 1,908 / 91,971 (2%) | passes (see stub) |
| chunks reached / `.js` present | — | — | 1,374 / 1,384 |

The 10 unreached `.js` files on 2.1.242 are 8 non-chunk helpers
(`audio-capture.js`, `mermaid.min.js`, …) and 2 re-export-only facade
chunks nothing imports. 242 holds real new code: 2,858 string literals
absent from 241, and ~11,000 more function nodes; 598 of 241's 25,528
unique literals are gone.

Linking across versions (all parse, all link):

| version | chunks linked | eager | lazy | inits | `import()` | `import.meta.require` | renamed | unreached `.js` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2.1.242 | 1,374 | 8 | 1,366 | 1,190 | 1,039 | 0 | 44,729 | 10 |
| 2.1.243 | 1,374 | 8 | 1,366 | 1,190 | 1,039 | 0 | 44,729 | 10 |
| 2.1.246 (`_NNN.js`) | 1,401 | 8 | 1,393 | 1,209 | 1,064 | 0 | 45,258 | 10 |
| 2.1.250 | 1,767 | 6 | 1,761 | 1,655 | 1,095 | 358 | 45,099 | 12 |
| 2.1.280 | 1,982 | 6 | 1,976 | 1,858 | 1,206 | 447 | 54,120 | 11 |
| 2.1.322 | 2,906 | 8 | 2,898 | 2,702 | 1,542 | 738 | 78,132 | 13 |

Linking takes 2-5 s. On 2.1.322 three of the unreached files carry code
(~56 KB, extra entry points (a chunk that only re-exports one tool) that nothing in
the main graph imports); the rest are helpers and facades.

Chunk -> chunk (2.1.242 -> 2.1.243, both linked): the two programs have the
same statement skeleton — identifiers and strings masked, 12 of 1,040,587
statements differ. Linking is stable across a hop (chunk names never reach
the output; the order comes from the import graph).

## Stub runs (no LLM)

The release binary at `b96d6e483` (branch head) run by hand against the
testkit stub (`stub-llm serve`), with the walk's library index
(`77b12699…`), split on, `--stats-json`. Matching is decided before any
answer, so these columns hold for a real model too (exp209; the real run
below reads the same). Results in `/work/exp216-runs/s4/`.

| column (`resolutionStats`, `coverage.functions`) | 240 -> 241 (walk, real model) | 241 -> 242 linked | 242 -> 243 both linked |
| --- | ---: | ---: | ---: |
| prior functions unmatched | 6 | 7,653 | 2 |
| still ambiguous | 40 | 4,202 | 47 |
| structural hash unique | 54,283 | 47,353 | 62,100 |
| enclosing statement | 22,176 | 17,113 | 28,994 |
| fresh functions | 91,971 | 107,937 | 107,937 |
| carried (cached) | 84,163 | 73,345 | 96,398 |
| already named | 7,763 | 6,771 | 11,490 |
| close-matched (asked with the prior's names) | 20 | 3,955 | 22 |
| asked the model | 2 | 13,553 | 0 |
| module bindings carried / asked | 30,890 / 1 | 24,368 / 3,234 | 31,430 / 1 |
| src files / of them the prior's paths | 5,314 | 6,077 / 4,182 | 6,077 / 6,077 |
| exit | 0 | 0 | 0 |
| `node run.cjs --version` | `2.1.241 (Claude Code)` | `2.1.242 (Claude Code)` | `2.1.243 (Claude Code)` |

- **The refusal is gone.** 84,318 of 91,971 prior functions (91.7%) are
  present in linked 2.1.242, against 1,908 (2%) in the loader alone.
- **The chunk -> chunk hop is a normal hop.** 2 unmatched, 47 ambiguous,
  nothing asked, every src path kept, the tree boots: linking is
  deterministic and stable across versions, so from 2.1.243 on the walk
  sees what it saw before 2.1.242.
- **The 241 -> 242 hop is the layout-change hop, and it is expensive.** It
  mixes two things this run cannot separate: 2.1.242's real new code
  (~11,000 more function nodes, 2,858 new string literals) and what the
  change of shape costs the matcher (the chunks' modules are not in the
  single-file order: only 55% of the shared unique string anchors fall in
  2.1.241's order, against 100% on a normal hop; context tiers that read
  neighbours — enclosing statement, ordinals, twins — lose, and 4,202
  twins stay ambiguous). It is paid once.
- Earlier forms, for the record (same stub, same prior): getter-method
  namespaces and the eager-then-lazy order carried 70,581 functions and
  counted 16,877 "nothing to rename" functions (the getters); the
  `__export` form and the walk order carried 73,332 (`/work/exp216-runs/s3/`).

## The real hop (local gpt-oss-20b, the walk's flags)

The walk's own launch for 2.1.242 (`/work/walk-73d01430-up/runs/2.1.242.toml`:
low reasoning, concurrency 472, split on, library index `77b12699…`),
prior `/work/walk-73d01430-up/trees/2.1.241/.humanify/humanified.js`,
output under `/work/exp216-runs/`. No flag beyond the walk's: the input is
the same `binary-decompiled/index.js` the walk passes.

| | run 1 (`c6a2cf6e9`) | run 2 (`c40170a38`) |
| --- | --- | --- |
| exit | 0 | 0 |
| wall | 32.5 min | 33.0 min |
| provider errors | 0 | 0 |
| matching (all exact columns) | as the stub table's 241 -> 242 column | identical to run 1 |
| LLM requests sent | 24,914 | 24,801 |
| split method | module markers, 6,612 markers, 99.76% of app code | same |
| src files / of them 2.1.241's paths | 6,061 / 4,214 | 6,060 / 4,211 (of 5,314) |
| top-level src entries shared with 2.1.241 | — | 2,114 of 2,352 |
| vendor files / packages (241: 2,328 / 284) | 2,427 / — | 2,427 / 301 (280 shared) |
| runnable scaffold | **no** — post-split relink failed | yes (`run.cjs`, `package.json`) |
| `node run.cjs --version` | — | **fails**: `Cannot find module 'ws'` |

(An earlier run, on a build that still turned named external imports
into plain bindings, was stopped after ~49,000 asks; that shape is what
made the external-import fix.)

Both failures were linker bugs, both fixed and gated. A third real run
was not made; the stub run above, on the fixed binary, writes the
scaffold and boots (`2.1.242 (Claude Code)`, and `2.1.243` on the next
hop):

- run 1: Bun's ESM runtime binds `var k=import.meta.require` and the CJS
  factories call `k("os")`. Kept as a bundle binding, 420 vendor factories
  "captured" it (2.1.241: 8), and the relink refused one whose body has a
  parameter named `require`. Fixed in `c40170a38`: the alias becomes the
  wrapper's own `require` (captures 420 -> 52; the rest are
  `(init(), ns)` reads, the same class as 2.1.241's 8).
- run 2: a lazy chunk's `import … from "ws"` (a package Bun resolves
  itself) was a top-level `require`, so the whole tree failed at load
  under Node. Fixed in `b96d6e483`: a lazy chunk loads its externals in
  its init, as Bun's single-file output does.

## Gate

`cargo dev check` in the worktree at `b96d6e483` (also green at `c40170a38` and `c6a2cf6e9`):

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW — the same 50 findings as main, none in the new code |
| census:concurrency | PASS (the linker adds no concurrency site) |
| census:slop | PASS (humanify-ag-core erosion 32.3% -> 32.4%) |
| rust:unit | PASS (21 new linker tests, written before the code and seen red — except the shadowed-alias guard, green on arrival because the old code already kept the binding; most run the linked program in Node. 1 new e2e-runner test) |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS — every fixture as before, plus `bun-chunks` |

The e2e launch golden gains exactly the `bun-chunks` launches (64 lines,
re-recorded from a live run); no other line moved. The three matching pin
tests list `bun-chunks` beside `bun-cjs-app` as added after the pins (its
`index.js` is only the loader). Neutrality by construction: the linker
returns any input that is not this layout byte-for-byte, and the
detection is a text scan (`// @bun` without `@bun-cjs`, a `"/$bunfs/root/`
string, a sibling present) — every single-file Claude Code bundle is
`@bun-cjs` and every other fixture lacks the specifier.

## Open

1. **Order on the layout-change hop.** The linker's written order keeps
   chunks whole; 2.1.241's single file interleaves their modules (one
   big 7 MB chunk spans 3.6M..26M of 2.1.241's bytes). A statement-level
   merge would need a global order key. Bun's cross-chunk export names
   are a counter in (reverse) global order — Spearman -0.93 per chunk,
   -0.78 per binding against 2.1.241 positions — but a first k-way merge
   by it scored worse (56.5% anchors in order) than the chunk walk. Worth
   it only if the walk's 241 -> 242 hop proves too costly; later hops do
   not need it.
2. **Entries nothing imports.** From ~2.1.300 a few chunks are extra
   entry points (chunks that only re-export one tool, ~56 KB on 2.1.322) that the
   main graph never imports; they are not linked (`jsFilesUnreached`
   counts them with the helper `.js` files). Their code is mostly import
   lists of reached chunks.
3. **Synchronous require of an eager chunk ordered after its caller.**
   `import.meta.require(chunk)` executed at load time against an eager
   chunk written later would read its namespace before it is filled. Not
   seen on any input; the linker does not refuse it yet.
4. **Renamed names reach the model.** ~45,000 top-level names are renamed
   apart (`a$4`); a binding the matcher does not carry is shown to the
   model under that spelling, as a minified name would be.
5. **Where it plugs in.** Andrew's plan named a fifth adapter in
   `choose_adapter`; it is the input side of stage 2 instead (reason in
   "What was built"). If a registry is wanted there, the natural one is a
   list of input layouts (`single-file`, `bun-chunks`) before detection.
