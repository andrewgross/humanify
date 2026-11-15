# exp235 — repo hygiene: runtime data out of experiments/, raw outputs and old docs out of the tree, a comment census

STATUS: BUILT (TDD), GATED (all 12 stages pass, see the gate table). No
product crate changed, so the pipeline binary is unchanged; no eval,
neutrality run or walk was needed or run. Branch `exp235-repo-hygiene` from
main `6c4b4d13f`. Phase A of the simplification campaign. Nothing was
rewritten in git history: every removed file is still readable with
`git show 6c4b4d13f:<path>`.

## Totals

| what | files | lines removed | lines added |
| --- | ---: | ---: | ---: |
| `dev/library/results/` (554 library labels, scorer 13/14, refused by today's scorer 15) | 28,003 | ~3,990,000 | 0 |
| `experiments/` raw outputs (data, scripts, fixtures, generated tables) | 274 | 136,839 | 0 |
| `docs/` (87 dated, TS-era or port-era files) + `PORTING.md` | 88 | 25,430 | 0 |
| kept docs, `CLAUDE.md`, `README.md`, configs: links to deleted files fixed | ~20 | ~60 | ~60 |
| harness code (`crates/humanify-ag-dev`), non-test | 12 | ~40 | ~720 |
| harness tests and test fixtures | 9 | ~50 | ~220 |

The tree loses about 28,400 files and 4.1 million lines. The only new code
is the `census:comments` stage (about 470 lines) and a typed loader for the
moved bun-dev list.

## What changed

1. **Runtime files out of `experiments/`.** `experiments/165-claim-boundary/bun-dev-list.tsv`
   (read by the bun-dev stub batches) is now `dev/library/bun-dev-list.tsv`,
   read by a typed loader (`committed::library::bundev_list`) like every
   other file under `dev/`. A row without a bundle path now fails the load
   instead of being skipped. The vocabulary moved from
   `experiments/034-eval-harness/VOCABULARY.md` to [docs/vocabulary.md](../../docs/vocabulary.md);
   38 links to it were rewritten.
2. **Raw experiment outputs removed.** Every non-Markdown file under
   `experiments/` (186 JSON, 29 TS, 24 TXT, 15 JS and others) and the
   generated `tables/` folders went. Each experiment keeps its README and
   write-ups. `.gitignore` now ignores everything under `experiments/`
   except Markdown (and `tables/`), so a run's output cannot be committed
   by accident. The stale `experiments/fixtures` and `092` ignore rules went.
3. **Library labels removed.** `dev/library/results/` is gone and ignored
   (the library scorer still writes labels there, locally). Four tests read
   committed labels; they now read three labels' `summary.json` and
   `timing.json` kept as fixtures under
   `crates/humanify-ag-dev/src/library/fixtures/labels/` (60 KB). The
   leaderboard golden for the two-corpus pair whose second corpus is the
   large CLI corpus was dropped; the bun-dev golden and the two-corpus
   refusal (now bun-dev against apps) stay.
4. **Docs cut.** The keep list of the lane 6 review: 18 files stay, plus
   the moved vocabulary. Deleted: `docs/spec/` (28), `docs/rust-port/`
   (25), the `plan-*`, `issue-*`, `review-*`, `code-review-*`, `audits/`,
   `e2e/`, `research/` files and the other dated notes, and `PORTING.md`.
   Every link or mention in a kept doc, `CLAUDE.md`, `README.md`,
   `Cargo.toml`, `clippy.toml`, `rust-toolchain.toml`, the webcrack shim
   and one fixture README now points at `git show 6c4b4d13f:<path>` or was
   dropped. `docs/flow/cross-version-matching.md` lost its "supersedes"
   section (the document it compared against is gone) and keeps the list
   of matching rules with their code locations.
5. **`census:comments`, a new ADVISORY gate stage** (`cargo dev census
   comments [--base REF] [--all]`, `crates/humanify-ag-dev/src/census/comments.rs`).
   Five rules:
   - C1: a comment in `crates/` cites an experiment id, `finding #N` or an ISO date;
   - C2: a comment in `crates/` names a file that is not in the tree (a deleted
     TS, Python or shell file, a deleted doc); output file names of a run
     are not counted;
   - C3: a comment in `crates/` says never, must or "do not" without an
     `(enforced: <name>)` tag naming a gate stage, a `clippy::` lint or a
     test function;
   - C4: a Markdown link or `docs/….md` reference in `CLAUDE.md`, `README.md`
     or `docs/` resolves to nothing;
   - C5: a tracked `.md` file matches no pattern in
     `dev/census/docs-allowlist.json` (root README and CLAUDE, `docs/**`,
     any `README.md`, `test/**`).

   It counts every rule over the whole tree and asks for review (exit 1,
   `REVIEW` in the gate) only for findings the merge-base with `main` did
   not have, judged against today's tree. `--all` lists every finding: the
   work list for the per-crate comment cleanup. The comment lexer is the
   concurrency census's (`scan::comments` beside `sanitize`).

## What census:comments finds today (main `6c4b4d13f` plus this branch)

| rule | findings | how sure |
| --- | ---: | --- |
| C1 provenance | 1,895 | exact pattern; every sample read was a citation |
| C2 file not in the tree | 986 | a sample of 14 read: all name a deleted TS / Python file or deleted doc |
| C3 directive without a tag | 1,726 | exact word match; whether each needs a check or a rewrite is the cleanup's call |
| C4 dead link in kept docs | 0 | the docs cut fixed all of them |
| C5 Markdown outside the allowlist | 79 | experiment write-ups other than README (RESULTS, BRIEF, ...) and the TS-era eval label notes |
| **total** | **4,686** | new on this branch: 0 |

The C2 references to deleted docs inside product crates (about 25 lines,
e.g. `docs/rust-port/20-fast-mode.md`) were left for the per-crate comment
cleanup, because another lane owns those crates now.

## Gate

`cargo dev check`, all 12 stages, exit 0:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (49 groups; none touches a file this branch changed) |
| census:concurrency | PASS |
| census:slop | PASS |
| census:comments | PASS (0 new findings) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS |

## Open items

- A local tag for the deleted docs (`docs-final`) was not created; the
  pointers use the commit `6c4b4d13f`, which needs no tag. Purging the
  library labels from history needs Andrew's explicit confirmation.
- Two committed JSON files still cite deleted docs in their comment field:
  `dev/eval/self-hop-reference.json` (`docs/rust-port/00-control.md`) and
  `dev/census/concurrency-allowlist.json` (`docs/audits/...`, and
  `scripts/concurrency-census.ts`). The first is hashed into run identities,
  so it was left alone.
- `dev/eval/results/` (the TS-era eval labels) belongs to exp237; its
  `family-rotation-ceiling/RESULTS.md` links a vocabulary file that never
  existed there.
- `docs/dev-harness.md` still describes the port's file table; its rewrite
  is a later lane. The FOLD docs (`config-consolidation.md`,
  `perf-inventory.md`) are kept as they were.
- The pre-commit hook does not run the comment census yet; flipping it to
  pass/fail per crate follows each crate's comment cleanup.
