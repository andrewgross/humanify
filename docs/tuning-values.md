# Tuning values

Every tuned number the pipeline runs with, in one place: where it lives, what
it was measured on, what it depends on, and why it should work on an app
that is not Claude Code. Written 2026-10-05 from the two Claude-Code-specific
scans (`/work/cc-specific-scan-2026-10-05/`). **No value was changed to write
this page.**

Why it exists: every eval pair is a Claude Code version, so every number here
has only ever been judged on one app. Most were carried over from the
TypeScript pipeline with no reason given. The rule (Andrew, 2026-10-05) is
that nothing in the pipeline may be specific to the test app; bundler-specific
pieces belong in the toolchain. This page is where a reader finds out which
numbers might quietly break that rule.

## How to read a row

Paths are under `crates/humanify-core/src/` unless they start with
`crates/`.

**Measured on** says what evidence picked the value:

- **CC** — measured on Claude Code runs (an experiment or eval pair).
- **CC + model** — measured on Claude Code with the local model
  (gpt-oss-20b on the vLLM measurement server); speed or prompt-size numbers.
- **TS, none** — copied from the TypeScript pipeline, no basis on record.
- **none** — chosen, never measured.

**Depends on** says what the value is really about:

- **app** — the content or shape of the program being decompiled. A value
  here that was tuned on Claude Code is the kind of thing the rule forbids.
- **bundler** — the bundler's output. Should be a toolchain value
  (`toolchain.rs`), chosen from detection.
- **model/server** — the model's context, speed or habits, or the server's
  throughput. Not about the app; the honest note is "measured with
  gpt-oss-20b".
- **code shape** — general facts about JavaScript code (how deep, how
  similar). Probably carries over; only checked on Claude Code.

A value whose "why it should carry over" is **unmeasured** has no argument on
record beyond the fact that it has not visibly failed on Claude Code.

## Naming: batching, lanes, retries (model/server)

| Value                                                              | Where                                                                                         | Measured on                                                                                   | Depends on   | Why it should carry over                                                                                                   |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------- |
| Batch size 25 identifiers per request (`DEFAULT_BATCH_SIZE`)       | `crates/humanify-core/src/naming/waves/batch.rs:38`                                           | CC + model (`docs/rust-port/20-fast-mode.md`: cold wall on 85→86, 118→119; eval-clean)        | model/server | It trades request count against prompt size on one server. Not about the app; re-measure with a different model or server. |
| Lane split: ≤25 bindings → 0 lanes, ≤200 → 4, ≤1000 → 8, else 16   | `naming/waves/batch.rs:82` (`compute_lane_count`); threshold `:43`, `--lane-threshold`        | TS, none                                                                                      | model/server | Unmeasured. Concurrency shape only — changes speed, and which names share a prompt.                                        |
| Free retries: 100, or bindings / 4 if larger                       | `naming/waves/batch.rs:41`, `:95`                                                             | TS, none                                                                                      | model/server | Unmeasured. A budget for how often the model's answers collide.                                                            |
| Re-ask limit 2 (`REASK_LIMIT`, `--rename-retries`)                 | `naming/reask.rs:53`                                                                          | none                                                                                          | model/server | Unmeasured. How often a refused answer is worth asking again depends on the model.                                         |
| Module-level names asked 10 per request (15 under esbuild)         | `toolchain.rs:457` (`BundlerTuning::module_group_size`)                                       | TS, none (TS `processor.ts:1402`)                                                             | bundler      | Already a toolchain value. Why esbuild gets 15 is not on record.                                                           |
| Module names grouped within 100 lines of the group's first binding | `naming/waves/processor.rs:1237`                                                              | TS, none (the TS passed radius 50 and compared against `2 × radius`; the comment now says so) | code shape   | Unmeasured. Keeps neighbours in one prompt.                                                                                |
| Request timeout 300 s, `max_tokens` 6000, concurrency 50           | `crates/humanify-model/src/llm.rs:630`, `:633`; `crates/humanify-cli/src/util.rs:110`, `:114` | TS, none                                                                                      | model/server | About the server, not the app.                                                                                             |

## Naming: what the prompt shows (model/server)

| Value                                                                       | Where                                           | Measured on                                                    | Depends on   | Why it should carry over                                                                        |
| --------------------------------------------------------------------------- | ----------------------------------------------- | -------------------------------------------------------------- | ------------ | ----------------------------------------------------------------------------------------------- |
| Code window 500 lines; header 30, pad 20 before / 40 after                  | `naming/code_window.rs:23`, `:26`, `:28`, `:29` | TS, none                                                       | model/server | Unmeasured. Bounded by the model's context, not the app.                                        |
| Used-names list 50 (function), 200 (module); prior-name hints 40            | `naming/prompts.rs:70`, `:74`, `:72`            | TS, none                                                       | model/server | Unmeasured.                                                                                     |
| Snippets 800 chars / 10 lines; call site 200 chars, expanded below 80       | `naming/waves/prompt_text.rs:28`, `:33`, `:35`  | TS, none                                                       | model/server | Unmeasured.                                                                                     |
| Context variables: at most 30, each at most 120 chars                       | `naming/context.rs:112`, `:114`                 | TS, none                                                       | model/server | Unmeasured.                                                                                     |
| Retry snippet 30–80 lines (±2 around each use); retry used-names 25         | `naming/waves/processor.rs:3412`–`:3415`        | TS, none                                                       | model/server | Unmeasured.                                                                                     |
| Split-namer context 32,768 tokens (`DEFAULT_CONTEXT_TOKENS`)                | `place/assign/namer.rs:234`                     | model                                                          | model/server | gpt-oss-20b's window, documented as the local model's default; `--context-tokens` overrides it. |
| Split-namer budget: 3 bytes/token, 75% headroom, 60 completion tokens/entry | `place/assign/namer.rs:238`, `:241`, `:244`     | CC + model (finding #39: refused prompts ran ~3.9 bytes/token) | model/server | Conservative on purpose (3 under-counts bytes per token); a tokenizer fact, not an app fact.    |

## Matching against the prior version (code shape)

These all refuse rather than guess: on another app a wrong value gives fewer
matches (more names asked fresh), not wrong code.

| Value                                                             | Where                                 | Measured on                                                | Depends on | Why it should carry over                                                                                                               |
| ----------------------------------------------------------------- | ------------------------------------- | ---------------------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Close match 0.8 cosine, top 3 candidates per function             | `matching/close.rs:65`, `:62`         | TS, none                                                   | code shape | Unmeasured.                                                                                                                            |
| Shingle similarity floor 0.5                                      | `matching.rs:465`                     | TS, none                                                   | code shape | Unmeasured.                                                                                                                            |
| Single-vote content floor 0.5                                     | `twins/role.rs:28`                    | TS, none                                                   | code shape | Unmeasured.                                                                                                                            |
| Enclosing statement at most 50 lines (`MAX_ENCLOSING_STMT_LINES`) | `matching/statement_context.rs:43`    | **none — the code says "UNMEASURED WHEN CHOSEN"** (exp079) | code shape | Unmeasured.                                                                                                                            |
| Statement alignment depth 16 (`MAX_ALIGN_DEPTH`)                  | `matching/statement_align.rs:189`     | CC (else-if chains 5–6 deep inside try blocks)             | code shape | Each level hashes a strictly smaller subtree and only follows type-unique remainders, so a deeper budget adds no ambiguity on any app. |
| Identity rounds 4, alternation rounds 3                           | `matching/alternation.rs:259`, `:626` | TS, none                                                   | code shape | Unmeasured; each round is monotone, so the caps only bound work.                                                                       |
| Same-program floor: ≥50 prior functions, ≥5% present              | `prior.rs:9`, `:12`                   | TS, none                                                   | code shape | A wrong-file check, not a tuning of matches; a real prior of the same program clears 5% easily.                                        |

## Naming votes and reconcile (code shape)

| Value                                                      | Where                                   | Measured on | Depends on | Why it should carry over |
| ---------------------------------------------------------- | --------------------------------------- | ----------- | ---------- | ------------------------ |
| Vote minimum 2 agreeing votes (`MIN_MODULE_BINDING_VOTES`) | `rename/votes.rs:53`                    | none        | code shape | Unmeasured.              |
| Proximity windowing from 100 bindings, radius 100 lines    | `rename/votes/proximity.rs:58`, `:60`   | none        | code shape | Unmeasured.              |
| Reconcile corpus gate: ≥8 prior lines, 0.5 similarity      | `naming/reconcile/hunks.rs:465`, `:466` | none        | code shape | Unmeasured.              |

## Bundle shape gates (bundler, with app risk)

| Value                                                                               | Where                                                                                               | Measured on                                                    | Depends on       | Why it should carry over                                                                                                                                                                                                                                                                |
| ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A wrapper must declare ≥50 names to count as the bundle wrapper                     | `modules/wrapper.rs:26` (read through `toolchain.rs:306`, `BundleLayout::SingleWrapperFunction`)    | none ("guards against small per-module IIFEs (Webpack style)") | bundler          | Unmeasured. Claude Code has ~15k names in its wrapper, so 50 was never near the line on the eval. A small Bun CLI under 50 top-level names is not treated as a bundle (and `--split` exits 1). Should be a layout (toolchain) property, or trusted when the banner is definitive.       |
| In-file library freezing is OFF whenever a wrapper exists                           | `libdetect/function_carry.rs:306`; `naming/driver/library.rs:11`                                    | none — carried from TS `7b3f5ce6` (2026-03-30) with no reason  | bundler          | Unmeasured. Every Claude Code input has the wrapper, so this path is always off on the eval. Plausible reason: under Bun the unpacker already pulled vendor factories out; nothing says so. A wrapper bundle with banner-marked library code still inside it goes to the model instead. |
| Ledger ordering flips at 5,000,000 chars (`BIG_SOURCE_BYTES`, a copied Babel quirk) | `rename/validated/ledger.rs:215`; read at `naming/driver/era.rs:663`, `naming/reconcile/step.rs:98` | TS (mimics Babel clearing its scope cache)                     | app (input size) | Only the ENTRY ORDER of the opt-in `--rename-ledger` diagnostic changes; no output tree byte. Claude Code is over 5 MB, so a smaller app's ledger is ordered differently. Retire at the next ledger re-cut.                                                                             |

## Placement and the split (B2–B5 of the split scan)

| Value                                                                                            | Where                                                   | Measured on                                                             | Depends on | Why it should carry over                                                                                                                                                                            |
| ------------------------------------------------------------------------------------------------ | ------------------------------------------------------- | ----------------------------------------------------------------------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Module matching: export set 0.6, containment 0.75, Jaccard 0.4, stem 0.7, graded 0.5, margin 1.5 | `place/assign/fossil_match.rs:92`–`:97`                 | CC (exp074, exp085 on CC hops)                                          | code shape | They refuse rather than guess: fewer matches and more renamed files on another app, never wrong code.                                                                                               |
| Folders: dissolve below 2 files; importer consensus 0.5, 3 passes                                | `place/assign/fossil.rs:33`, `:35`, `:37`               | CC (exp076)                                                             | code shape | Graph rules over the import graph only.                                                                                                                                                             |
| Content anchors: rare string ≥12 chars; token overlap 0.5; near-identical ≤0.1 edit              | `place/anchor.rs:39`, `:24`, `:26`                      | CC (exp041–043)                                                         | **app**    | Claude Code is rich in long strings; a library has fewer, so this tier fires less and placement falls back to neighbouring statements. Safe direction.                                              |
| Bad file-name stems (`reactlib\d+`, `initializemodule\d+`, noop families)                        | `place/stems.rs:95`                                     | CC (model habits seen on CC runs)                                       | model      | LLM habits, not app content.                                                                                                                                                                        |
| Fresh grouping targets: 1700 files, ≤2500 lines, 40–100 top folders, 6–25 per subfolder, …       | `place/assign/cluster.rs:48` (`DEFAULT_CLUSTER_CONFIG`) | **CC** (exp029: Claude Code 2.1.88's real source tree, tuned on 2.1.89) | **app**    | Not scaled to input size: a 5K-line app gets one top folder; a bigger bundle is capped at 1700 files. Belongs to the split-method lane (`fix/split-by-bundle-contents`), not this page's to change. |

## Name shape (the minifier name profiles)

| Value                                                                                                        | Where                                                                                     | Measured on                                     | Depends on | Why it should carry over                                                                                                                                                        |
| ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- | ----------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bun profile's extra rules (`$` anywhere, `_` tail, mint head at any length), and Bun as the fallback profile | `rename/floor.rs:155` (`is_bun_token`); `rename/name_profile.rs:130` (`FALLBACK_PROFILE`) | **CC** ("calibrated on the Claude Code corpus") | **app**    | Does not carry over: flags `user$`, `$scope`, `clicks$` as minted on any unsure input. Finding B1 of the naming scan; fixing it changes Claude Code bytes, so it rides an eval. |
| Word lists `SHORT_WORDS`, `DOMAIN_STEMS`, `TECH_TERMS`                                                       | `rename/floor.rs`                                                                         | CC (false positives and recorded answers)       | code shape | General programming vocabulary, open lists; other apps will need more entries (`ed25519`, `p256`). `it2` (iTerm2, app vocabulary) was removed 2026-10-05.                       |

## Vendor modules (code shape)

| Value                                                                                                                                  | Where                                                                                                   | Measured on                                                                                                                          | Depends on | Why it should carry over                                                                                                                                                                                                                                                    |
| -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Content pair: score >= 0.3, margin >= 0.1 on both sides, 5-token shingles, strings > 40 chars split into words, unseen-literal IDF 8.0 | `modules/vendor_pairing.rs` (`MIN_SCORE`, `MIN_MARGIN`, `SHINGLE`, `LONG_STRING`, `UNSEEN_LITERAL_IDF`) | CC (/work/vendor-mapping-2026-10-06/, finding #88: 0 wrong of 475 checked against the grammar modules' own names, 40/40 hand-judged) | code shape | The margin, not the floor, does the work: a module with a lookalike scores within 0.1 of it and is left to the model. On another app a wrong value pairs fewer modules (more model asks), not wrong ones, unless two different libraries are each other's clear best match. |
| App text asset name: lead phrase of the first naming line, at most 5 words / 40 chars, filler words dropped, 8 lines searched          | `modules/text_assets.rs` (`MAX_WORDS`, `MAX_CHARS`, `MAX_LINES`, `FILLER_WORDS`)                        | none                                                                                                                                 | code shape | Readability only: a file name, never a decision. A text with no naming line falls back to `lib_<hash8>`.                                                                                                                                                                    |

## Totals

36 rows (a row may hold several related constants).

| Measured on             | Rows |
| ----------------------- | ---: |
| none / TS with no basis |   24 |
| Claude Code (± model)   |   11 |
| model only              |    1 |

| Depends on   | Rows | Of which tuned on Claude Code |
| ------------ | ---: | ----------------------------: |
| model/server |   13 |                             3 |
| code shape   |   16 |                             5 |
| bundler      |    3 |                             0 |
| app          |    4 |                             3 |

20 rows have no argument on record for carrying over (marked "Unmeasured" or
"not on record"), `MAX_ENCLOSING_STMT_LINES` among them by its own comment.

The four app-dependent rows are the ones that break the rule: the Bun name
profile's Claude Code calibration, the fresh-grouping folder targets, the
content-anchor string length (safe direction), and the ledger's 5 MB quirk
(diagnostic only, copied from Babel rather than tuned on Claude Code).
