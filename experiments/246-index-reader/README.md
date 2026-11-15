# exp246 — the library index is read through a seam the shell fills

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on both
pairs (warm, 0/0 cache writes). Branch `exp246-index-reader`, not merged. Step E of the effects
ratchet plan (experiments/243-effects-ratchet/README.md): functional core,
imperative shell.

## Totals

- **Core no longer opens any library-index file.** `LibraryIndex::load`
  takes the index's files (`Box<dyn IndexFiles>`) instead of a directory.
  The names-mode answers reader and library detection read through the
  same kind of seam. The cli provides the file-backed side.
- **The large tables are still read lazily.** A table is now opened as a
  `ReadAt` reader (read bytes at a position). Core keeps its page cache
  over that reader, so a run still reads only the 4 KiB pages a lookup
  touches. `modules.<slice>.jsonl` is still streamed line by line.
- **7 core `legacy` entries leave `dev/census/effects-allowlist.json`**
  (6 disk sites, 1 `Result<_, String>`). **5 `shell` entries** list the new
  file-backed module in the cli. On origin/main `d69a2615a` the allowlist
  goes from 417 to 415 entries, and the pure crates' effect sites outside
  profiling (all in core now) go from 65 to 59.
- Product behaviour does not change: neutrality on both pairs (below).

## What changed

| where | before | after |
| --- | --- | --- |
| `core::libindex::files` (new) | — | the seam: `IndexFiles` (`read_text`, `open` by file name), `ReadAt` (`size`, `read_exact_at`), `MemoryIndexFiles` (in memory), `Sequential` (a `ReadAt` as `std::io::Read`) |
| `libindex::table::Source` | `Source::open(path)` held a `std::fs::File` and called `pread` | `Source::paged(reader)` holds a `Box<dyn ReadAt>`; the page cache is unchanged |
| `LibraryIndex::load` | `load(dir: &Path, ..)`, read the manifest and opened tables itself | `load(files: Box<dyn IndexFiles>, ..)`; the index keeps `files` in place of `dir` |
| `module_answers::read_module_answers` | opened `<dir>/modules.<slice>.jsonl` | reads it from `&dyn IndexFiles` |
| `discovery::ModuleAnswersSource` | `dir: &Path` | `files: &dyn IndexFiles` |
| `libdetect::detect_libraries` | read each unpacked file itself | takes `read: ReadText` (the shell's `index_files::read_utf8`) |
| `humanify-ag-cli::index_files` (new) | — | `IndexDir` (a directory on disk), `FileReadAt` (`pread`, length read at open), `read_utf8` |
| dev `library/copy_credit.rs` | `Source::open(&bin)` | `Source::paged(FileReadAt::open(&bin))` |

Core and index tests now load indexes through `MemoryIndexFiles`. The
core tests that write an index directory read it into memory with a test
helper (`libindex::files_test::dir_files`). New tests cover the seam: a
name the in-memory set lacks is `NotFound`, the same as a missing file; a
reader refuses a read past its end; a sequential reader streams every
line; a table read through pages answers the same as the same bytes in
memory, across several pages.

Error texts are unchanged with one exception. A table that does not open
still reads `<name>: <path>: <io error>`, and a missing small file still
reads `<name>: <io error>`. A missing `modules.<slice>.jsonl` now reads
`modules.<slice>.jsonl: <path>: <io error>`, where before it had no path.
That text appears only on a refusal.

Line counts (`git diff --numstat` over `crates/`, comments included): code
+266 / −92 (two new modules: the seam, about 95 lines, and the file-backed
side, about 75), tests +227 / −96.

## Gate

| stage | result |
| --- | --- |
| shims, no-private-addresses, rust:fmt, rust:clippy | PASS |
| census:clones | REVIEW (advisory; every group predates this branch) |
| census:concurrency, census:effects, census:slop | PASS |
| census:comments | PASS (the first commit's 5 new "never" lines were reworded) |
| rust:unit, rust:build, format-golden, e2e | PASS |

Re-gated after rebasing onto `d69a2615a` (the model-crate lane): all 13
stages pass. The rebase had no conflicts and brought no product change of
this branch's, so the neutrality verdict below stands.

## Neutrality

Candidate `b563522e1` against baseline `3ea3a7baf` (origin/main), warm
cache `/work/neutrality-cache`, priors `/work/neutrality-priors`:

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118 -> 2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197 -> 2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

## Open items

- Library detection still reads unpacked files from disk, now through the
  shell's reader. Step F (the in-memory output tree) makes those files
  values, so the `ReadText` parameter can go.
- `PagedFile::page` still panics on a failed read (the file changed during
  the run) and carries its `// default: legacy` tag. Step H triages it.
- The other 25 `Result<_, String>` signatures in `libindex` are step C.
