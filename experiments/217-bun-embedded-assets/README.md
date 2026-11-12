# exp217 — Bun's embedded files in a linked code-split app

STATUS: BUILT (TDD), GATED (all 11 stages, see Gate), REAL RUN
2.1.245 -> 2.1.246 and STUB RUN 2.1.322 (see Runs), NOT MERGED. Branch
`exp217-bun-embedded-assets` from main `1c092aa0b`. No eval and no walk
were run. Every number naming Claude Code comes from this host's inputs,
binaries and runs; no Claude Code text is committed.

## The problem

From Claude Code 2.1.246 the compiled binary carries non-JS files beside
its chunks (118 `.md` and 46 `.txt` in 2.1.246; `.zst`, `.mjs` and
`.woff2.asset` later). The code reads them at run time —
a call of the shape `r("/$bunfs/root/<name>-<hash>.md")`, `r` an alias of
`import.meta.require` — and exp216's linker left those specifiers as they
were, so every walk tree from 2.1.246 failed the boot prompt:
`Cannot find module '/$bunfs/root/<name>-<hash>.md'`
(`--version` passed; 2.1.242-245 have no such files).

## What Bun does — read off its source

Bun's source: `git clone --depth 1 https://github.com/oven-sh/bun` at
`/work/tools/bun-src` (read only), tags `bun-v1.4.0`, `bun-v1.4.1`,
`bun-v1.4.4` fetched, and main `ccb3611f2` (2026-11-12). Bun is written in
Rust from 1.4 (`src/standalone_graph/StandaloneModuleGraph.rs` is the old
`StandaloneModuleGraph.zig`). Line numbers are bun-v1.4.1's; bun-v1.4.4 and
main hold the same code at the lines given after the slash.

**The module record** (`src/standalone_graph/StandaloneModuleGraph.rs`):
`File` 613-638 / 4.4: 730, main: 944 holds `loader`, `encoding`,
`module_format`, `side`. On disk (52-byte V4 record) the four bytes after
six `{offset,length}` pointers are `encoding, loader, module_format, side`.
`Encoding` 411-419: `Binary = 0`, `Latin1 = 1`, `Utf16 = 2`.
`encode_text_module` 1188-1228 writes a TEXT module's body as Latin-1 when
every byte is ASCII, else as UTF-16LE at an even offset;
`is_text_module_output` 1176 (a text import emitted as an asset).
`File::utf8_contents` 664-672 (what fs reads return: the UTF-16 body
transcoded to UTF-8), `File::to_wtf_string` 697-730 (what `require` of a
text module returns: the body as a JS string). The virtual root:
`BASE_PUBLIC_PATH` 77-84 (`/$bunfs/root/`, `B:/~BUN/root/` on Windows).

**The loader enum** (`src/ast/loader.rs` 30-53): `jsx 0, js 1, ts 2, tsx 3,
css 4, file 5, json 6, jsonc 7, toml 8, wasm 9, napi 10, base64 11,
dataurl 12, text 13, bunsh 14, sqlite 15, sqlite_embedded 16, html 17,
yaml 18, json5 19, md 20, xml 21`; `should_copy_for_bundling` 117-127
(`file`, `napi`, `sqlite`, `sqlite_embedded`, `wasm`).

**What each loader's import becomes** (`src/bundler/ParseTask.rs`, the
per-loader arms; 1.4.4 and main move by ~30 lines):

| loader (byte) | the importer gets (Bun source) | what the output tree must do | in CC 2.1.246-322 |
| --- | --- | --- | --- |
| `text` (13) | from 1.4.1 in an executable: `require("/$bunfs/root/<name>")` (973-1013; `require_embedded_asset` 720-740 — in an ESM chunk an alias of `import.meta.require`); the run-time loader answers with the body as a string (`src/runtime/jsc_hooks.rs` 3769-3781, `is_text_module`). 1.4.0 inlined the string (1.4.0 ParseTask 886) | write the read as the string literal (decoding Latin-1 / UTF-16) — what Bun writes for a text import in one file | YES: `.md`, `.txt`, `.mjs` (2.1.259+) — encoding 1 and 2 both |
| `file` (5) | the path string `"/$bunfs/root/<name>"` (1357-1400) | copy the file byte for byte into the tree, rewrite the string to `__dirname+"/embedded/<name>"` | YES: `.asset` (1-3), `.js` (3: zstd blobs), `.zst` (97-137, 2.1.251+) — encoding 0 |
| `wasm` (9) | as `file` (same arm, 1357) | as `file` | no |
| `napi` (10) | `require("/$bunfs/root/<name>")` (1163-1197); Bun writes the addon to a temp file and `dlopen`s it (`jsc_hooks.rs` 4618, `resolve_embedded_file_to_buf`) | copy; the `require` stays, of the copy | YES: `.node` (2-3) |
| `sqlite_embedded` (16) | `import.meta.require(name, {with:{type:"sqlite"}}).db` (1057-1160); served by a generated module that reads the blob (`jsc_hooks.rs` 3747-3767) | REFUSED by the linker (a require with more than the name) | no |
| `sqlite` (15) | the same, with the on-disk path (not embedded) | — | no |
| `json`/`jsonc` (6/7), `toml` (8), `yaml` (18), `json5` (19), `xml` (21) | the parsed value written into the importer (843-972) | nothing: never an embedded file | no |
| `md` (20) | the file rendered to an HTML string, inlined (1015-1056) | nothing | no |
| `dataurl`/`base64` (12/11), `bunsh` (14) | an inlined string (1354-1356) | nothing | no |
| `html` (17), `css` (4) | HTML imports / CSS chunks (client-side assets for `Bun.serve`) | not handled | no |
| `js` etc. (0-3) | chunks (exp216's linker) and entries | linked; a Bun-built `.js` named by PATH (a worker entry) is left as is | YES (2.1.322: one worker-URL constant, 315 mentions) |

**Which run-time calls serve `/$bunfs/root/` paths** — what code may do
with a `file` path: `src/runtime/node/node_fs.rs` `access` 4448,
`exists` 5143, `lstat` 5363, `stat` 7557, `readdir` 6079 /
`readdir_standalone` 6662, `read_file_with_options` 6774 (returns
`utf8_contents`); `Bun.file` / Blob (`src/runtime/webcore/Blob.rs` 3504),
`Bun.embeddedFiles` (`src/runtime/api/BunObject.rs` 1853; non-text,
non-JS files, `appears_in_embedded_files_array` 642-646), `Bun.Glob`
(`src/runtime/api/glob.rs` 240-320), workers
(`src/jsc/web_worker.rs` 740), `process.dlopen` (above). The tree's copy is
a real file under a real path, so every one of these reads it as Bun did
(Claude Code reads its paths with `readFile`/`readFileSync` and a zstd
decompress, and `require`s its addons). Claude Code never calls
`Bun.embeddedFiles`, and its only other bunfs strings are the bare roots
`"/$bunfs/root/"` / `"B:/~BUN/root/"` (a "running compiled?" check), left
alone.

**Present in Claude Code** — the module records' `(extension, encoding,
loader)` read from the linux-x64 binaries (downloaded to
`/work/exp217-runs/bin/`; bun-decompile `debug` gives the offsets, a
52-byte V4 record walk the bytes — bun-decompile does not print them):

| version (Bun) | text (13) | file (5) | napi (10) | js (1) |
| --- | --- | --- | --- | ---: |
| 2.1.246 (1.4.1) | md 67+51, txt 21+25 (Latin-1 + UTF-16) | asset 1, js 3 | node 3 | 1,404 |
| 2.1.251 (1.4.1) | md 47+13, txt 10+2 | asset 1, js 3, zst 97 | node 3 | 1,794 |
| 2.1.259 (1.4.1) | md 47+13, txt 10+2, mjs 2 | asset 1, js 3, zst 103 | node 3 | 1,634 |
| 2.1.283 (1.4.3) | md 50+15, txt 22+4, mjs 2 | asset 3, js 3, zst 133 | node 2 | 2,136 |
| 2.1.322 (1.4.4) | md 62+12, txt 26+2, mjs 2 | asset 2, js 3, zst 137 | node 2 | 2,901 |

Every text record's bytes match its encoding byte: encoding 1 = pure ASCII,
encoding 2 = UTF-16LE (2.1.246: 88 / 76 files; 2.1.322: 90 / 14).
bun-decompile writes the stored bytes, so a UTF-16 body reaches us as
UTF-16 with no BOM and no encoding byte. (The committed inputs hold 5
`.node` files where the linux binary has 3: they were unpacked from
another platform's binary.)

Synthetic proofs (scratchpad, Bun 1.4.0 and 1.4.1 in `/work/tools/`):
1.4.0 inlines a text import; 1.4.1 writes `var t=e("/$bunfs/root/note-<hash>.md")`
and the executable prints the text; a `file` import is the path; an ASCII
text is stored as-is, `café` and `中文` as UTF-16 (the latter with no NUL
byte), CRLF kept.

## What was built

`unpack::chunk_link::embedded` (the linker's new step, after naming):

- every `"/$bunfs/root/<name>"` string in expression position that is not
  a chunk specifier is a use of an embedded file (`analyse`, per chunk —
  only names the code holds, never the directory listing);
- the first argument of a call whose callee is `import.meta.require` or
  resolves (across chunks) to an alias of it is a READ: a `.node` file
  keeps its `require`, anything else is a TEXT read and the whole call is
  replaced by the text as a string literal (Bun's printer's quote choice;
  `decode_text`: a NUL byte means UTF-16LE, else UTF-8 if it decodes —
  ASCII, or an already-decoded file — else UTF-16LE);
- every other use is a PATH: the string becomes
  `__dirname+"/embedded/<name>"` (the wrapper's `__dirname`, the tree root
  in a split tree) and the file goes on `LinkReport.embedded`;
- a path naming a Bun-built `.js` (first line `// @bun`) is code and left
  alone (`bunModulePaths`);
- refused, loudly: a missing file, a name leaving the root (`..`), a path
  where `__dirname` is rebound, a read with import attributes
  (`sqlite_embedded`), text that decodes as neither.

`import.meta.require(x)` loads a chunk only when `x` is a Bun-built module
(before: any bunfs specifier was queued as a chunk, so a direct text
require would have been parsed as JavaScript).

The finish stage (`finish::driver::finish_split_output`, runnable trees,
after the scaffold) writes the list with
`finish::scaffold::write_embedded_files` into `embedded/<name>` — Bun's
own name, content hash kept: 13 `README-<hash>.md` share a stem in
2.1.246, so dropping the hash collides, and the hash moves only when the
content does (a real change). `js_files_under` skips the root's
`embedded/` (2.1.246+ ship zstd blobs named `.js`). The eval reads only
`src/` and `vendor/`, so `embedded/` is not scored.

Stats `inputLink` gains `embeddedText`, `embeddedPaths`, `embeddedFiles`,
`bunModulePaths`.

Single-file inputs: byte-identical by construction — the step lives inside
the `bun-chunks` layout, and the `single-file` layout returns the file
unread; the finish writes nothing when the list is empty (it is empty for
every input that is not linked).

### Across versions (linking only)

| version | chunks | text reads inlined | path strings rewritten | files copied | Bun-module paths left |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2.1.242 | 1,374 | 0 | 9 | 9 (asset 1, js 3, node 5) | 248 |
| 2.1.245 | 1,375 | 0 | 9 | 9 | 248 |
| 2.1.246 | 1,401 | 164 | 9 | 9 | 250 |
| 2.1.251 | 1,787 | 72 | 106 | 106 (+ zst 97) | 252 |
| 2.1.259 | 1,639 | 74 | 112 | 112 | 265 |
| 2.1.283 | 2,144 | 93 | 142 | 142 | 276 |
| 2.1.300 | 2,407 | 106 | 147 | 147 | 299 |
| 2.1.322 | 2,906 | 104 | 145 | 145 (asset 2, js 3, node 3, zst 137) | 315 |

2.1.246's 164 text reads are its 118 `.md` + 46 `.txt` exactly. 2.1.242-245
change too (their 9 path strings now point at copies): they are linked
inputs, so no neutrality claim covers them.

## Fixture

`test/e2e/fixtures/bun-chunk-assets` (Bun 1.4.1, `build.sh`, generator
`bun-apps/source/gen.mjs assets`): bun-chunks plus a lazy `notes` module
with three text imports (one UTF-16, one with backticks / `${` / a
backslash / CRLF) and one `file` import. Node cannot run the unpacked
input (no `import.meta.require`), so the fixture sets
`"recordedBehavior": true` and the e2e compares the split trees against
the executable's recorded stdout (`build/v<ver>/compiled.stdout`).

## Gate

`cargo dev check` in the worktree at `1c934bbbd`: ALL 11 STAGES PASSED.

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS (first run failed on two lints in `decode_text`, fixed) |
| census:clones | REVIEW — the same 50 findings as main, none in the new code |
| census:concurrency | PASS (no new concurrency site) |
| census:slop | PASS (humanify-ag-core erosion 32.4% -> 32.6%) |
| rust:unit | PASS — 16 new linker tests (`chunk_link/embedded_test.rs`), 3 finish tests (`finish/scaffold/embedded_test.rs`), 1 e2e-runner test; every one written first and seen red. The matching pin tests list `bun-chunk-assets` beside `bun-chunks` as added after the pins (seen failing without it) |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS — every fixture as before, plus `bun-chunk-assets`: fresh, prior and sequential split trees print the compiled executable's stdout. The launch golden gains exactly its 64 lines (re-recorded from a live run; no other line moved) |

## Runs

### 2.1.245 -> 2.1.246, real model (local gpt-oss-20b, the walk's flags)

The walk's launch (`/work/walk-1c092aa0-up/runs/2.1.246.toml`: low
reasoning, concurrency 472, split on, library index `77b12699…`), prior
`/work/walk-1c092aa0-up/trees/2.1.245/.humanify/humanified.js`, binary
`1c934bbbd`; tree `/work/exp217-runs/real-tree/2.1.246`, stats
`/work/exp217-runs/real-246.stats.json`. Beside it, the walk's own 2.1.246
run (main `1c092aa0b`, same input and prior), whose tree failed the boot
prompt.

| | walk (main) | exp217 |
| --- | ---: | ---: |
| exit | 0 | 0 (17.9 min) |
| inputLink: chunks / text reads inlined / files by path / Bun-module paths | 1,401 / — / — / — | 1,401 / 164 / 9 / 250 |
| functions total / carried / close-matched / asked | 107,211 / 90,067 / 2,840 / 4,083 | 107,211 / 90,064 / 2,844 / 4,084 |
| prior functions unmatched / still ambiguous | 4,218 / 6,051 | 4,220 / 6,052 |
| module bindings total / asked | 30,875 / 1,206 | 30,879 / 1,092 |
| split | module markers, 99.92% | module markers, 6,930 markers, 99.92% |
| `embedded/` | — | 9 files (the asset, 3 zstd `.js`, 5 `.node`) |
| `bun run.cjs --version` | `2.1.246 (Claude Code)` | `2.1.246 (Claude Code)` |
| `bun run.cjs -p "say exactly: boot-ok" --model claude-opus-4-1` | `Cannot find module '/$bunfs/root/<name>-<hash>.md'` | `boot-ok` |

The matching barely moves: unmatched +2, ambiguous +1, close-matched +4,
carried -3. Those columns are decided before any model answer, so the
move is the change itself — 164 call expressions became string literals
(and 9 path strings became `__dirname+…`), which changes the shape of a
few statements the matcher reads. No `/$bunfs/root/*.md|txt` read
is left in `src/` or `vendor/`.

### 2.1.321 -> 2.1.322 stub run: STOPPED, not completed

`cargo dev run stub` on 2.1.321 cold (no prior; the walk's library
index), meant as 2.1.322's prior: after 1 h 53 min its RSS was 101 GB and
growing ~6 GB a minute (the verb's footprint says 10-17 GB), so it was
killed before it could take the host down; the 2.1.322 leg never ran. No
`src/`-stage output or message says which phase was growing (the log had
reached the naming answers). Whether this is the cold pipeline on the
largest bundle or something new is NOT known; it needs the coordinator's
slot to rerun with a prior (e.g. a chain from a walk tree), not a guess
here.

### Instead: the linked program itself, run by Bun (no naming, no split)

The linker's output for a version (a `@bun-cjs` file) plus
`write_embedded_files` into `embedded/` beside it, run with
`bun linked.js` (a scratch probe over the crate; `/work/exp217-runs/linked-*`):

| version | text reads / files copied | `--version` | `-p "say exactly: boot-ok"` |
| --- | --- | --- | --- |
| 2.1.246 | 164 / 9 | `2.1.246 (Claude Code)` | `boot-ok` |
| 2.1.248-2.1.265 (248, 251, 257, 259, 265) | 2.1.265: 74 / 110 (asset 1, js 3, node 3, zst 103) | boots | 251, 259, 265 FAIL: `m$1 is not a function` (2.1.251) — Bun's `__toESM` helper reads `Object.create` etc. bindings whose values the linker moved into the runtime chunk's init, called before it ran |
| 2.1.274, 2.1.280, 2.1.283, 2.1.322 | 2.1.322: 104 / 145 | FAIL at load: `J$57 is not a function` (2.1.274) — a helper used before the chunk that defines it is written | — |

Both failures are the exp216 LINKER's chunk ordering, not embedded files:
main's linker (`1c092aa0b`, the same probe built against it) fails
identically (2.1.251 prompt: `m$1 is not a function`; 2.1.283 load:
`K$66 is not a function`), and the failing names are interop helpers,
not reads. A raw linked program is not the pipeline's output either: the
bun adapter extracts the factories and the finish re-links them with its
own helper file, so a walk tree may boot where the raw link does not
(2.1.246's does both). Embedded files are not reached by either failure.

## Open

1. **The linker's chunk ordering from ~2.1.251 / 2.1.274** (above): an
   exp216 follow-up, and the first thing a walk tree past 2.1.250 will
   show if the pipeline does not mask it. Not touched here.
2. **The cold 2.1.321 stub run's memory** (above): rerun with a prior, in
   the coordinator's slot, to see whether the pipeline tree for 2.1.322
   boots.
3. **bun-decompile drops the record's `encoding` and `loader` bytes.**
   `decode_text` recovers the encoding from the bytes (exact for every
   Claude Code text file; ambiguous only for UTF-16 text with no ASCII
   character whose bytes also form valid UTF-8), and the loader from the
   use (a require is text unless `.node`; a string is a path). Writing a
   manifest of `(name, loader, encoding)` beside the unpacked files would
   make both exact; that is a bun-decompile change.
4. **Not handled because absent from every release**: `sqlite_embedded`
   (refused), HTML imports / CSS client assets, `Bun.embeddedFiles`
   (enumerates the embedded non-text files; a tree has none), Windows'
   `B:/~BUN/root/` specifiers (the linker reads `/$bunfs/root/` only,
   exp216).
5. **A text module named by PATH** (`readFileSync("/$bunfs/root/x.md")`)
   would read `utf8_contents` under Bun; the copy is the stored (possibly
   UTF-16) bytes. Bun's bundler never writes a text module's path (its text
   arm always writes the require), so this is noted, not handled.
6. **Single-file era file paths** (2.1.241 and before: `payload.template.html.asset`,
   `mermaid.min.js`, `*.node` literals) stay unhandled: the single-file
   layout is byte-for-byte by design; copying them would need its own
   owner and would not be neutral.
