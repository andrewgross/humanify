# exp184 — the index spec: the default index made by rules

STATUS: MEASUREMENT PENDING.

Andrew's directive (2026-11-02): "it's fine if we have rules like 'for these
20 packages, get the top 200 versions instead of top 20' to cover things
like the aws sdks with lots of releases, but it shouldn't be 'here is a list
of 25 random packages to also add and if you don't have this hardcoded list
then it won't work'." Index creation must be REPRODUCIBLE by a new user
from the public npm registry, driven by RULES — a small explicit list is
acceptable only as POLICY (refusals with reasons, family overrides), never
as the package universe.

## What changed

- **The spec** (`crates/humanify-index/specs/default.toml`,
  schema `humanify-index-spec/1`): ONE committed declarative file — the
  as-of date, the package selections (npm-high-impact's `top` list at a
  pinned, sha256-verified version; the dependency closure of the chosen
  versions), the version rules (the as-of policy; `dependents`; family
  overrides that are rules, e.g. `@aws-sdk/*: latest-200`), the recipes
  and pinned tools, and the refusals (rules with reasons).
- **The resolver** (`crates/humanify-index/src/spec.rs`,
  `src/planner.rs`): the spec resolves against the SAME registry snapshot
  the build reads, into every `pkg@version` with the rule that chose it.
- **The CLI**: `humanify-index build --spec <file> …`. Every legacy
  flag (`--packages`, `--pinned`, `--as-of`, `--recipe`, `--policy`,
  `--refuse`, …) is deleted — no backwards compatibility. `--chunk I/N`
  splits a heavy build across invocations; the final `--merge-only`
  merges.
- **The orchestrator** (`scripts/build-index.sh`): the committed,
  documented chunked build the full default index runs under (both locks,
  chunk 0 fetches the snapshot, the rest replay it, final merge).
- **The plan travels with the index** (`build.json` → `spec` block,
  schema `library-index-spec-plan/1`): every selected package (its rule
  attribution), every planned version (its role and attribution), every
  refused version (its reason), every skipped package; plus the spec's
  sha256, the as-of, the default policy, the `dependents` flag, the tool
  pins. The sidecar never affects the index id.
- **Doc**: docs/design/library-index.md §2 (the spec CLI) and §2a ("How
  the default index is made"); docs/responsibility.md rows for the spec
  and the resolver.

## What a rule is, and what an explicit list is still allowed to be

A RULE, for this lane's purposes, is a mapping the resolver can recompute
from public, replayable signals: the spec text itself, an npm tarball at a
pinned version (immutable, sha256-verified), and packuments (whose `time`
maps and version lists are stable for everything published before the
as-of date). The default spec's rules:

- `select impact` = every package in npm-high-impact's `top` list at
  version 1.13.0, hash-verified (npm's own definition of high-impact);
- `select impact-trees` = the dependency closure of those packages'
  chosen versions (where the scoped families come from);
- `versions.policy = "as-of"` (§3 of docs/design/library-index.md) plus
  `dependents = true` (the version each planned version's dependency
  ranges resolved to at that version's publish time);
- `override aws-sdks` = `latest-200` for `@aws-sdk/*` (dozens of releases
  a week, bundle surface changes only at major boundaries — Andrew's own
  example of a rule that is allowed);
- refusals: `@types/*`, platform binaries, and three pinned `pinyin`
  versions (measured OOMs, exp152).

The one kind of explicit list still allowed is POLICY — a refusal with a
reason, or an override naming a package FAMILY with a justification. What
is banned is a hand-kept package universe: a list whose absence breaks
coverage with no rule to regenerate it.

## What the resolver does

`planner::resolve` walks the spec's selections in order:

1. A list selection fetches its pinned tarball (`npm pack`), verifies the
   list file's sha256 against the spec, parses the names in order, and
   enters each package (up to `limit`).
2. A `dependencies` selection walks the universe's packages' POLICY-CHOSEN
   versions in rank order: each version's dependencies (regular, optional,
   non-optional peer; `npm:` aliases resolved to their targets) resolve as
   `npm install --before=<that version's publish time>` would, and a
   package not yet in the universe ENTERS — under its own policy, ranked
   after what pulled it in — with the entering edge's range resolving as
   one of its `dependent` versions. Termination: the walk follows each
   member's policy-chosen versions only (a `dependent` version's own
   install tree does not extend the closure — one edge deep, the plan
   stays at today's scale), and every package enters once.
3. Refusal rules apply before a package's packument is even asked for
   (names), and to its chosen versions (platform, size, exact versions).

The result is the plan: every `pkg@version` with the rule that chose it,
every refused version with its reason, every skipped package. The same
plan resolves again offline from the registry snapshot for every build
invocation — the plan is a function of (spec, snapshot).

## Measurement

### (a) Determinism — PROVEN

The smoke spec (top-3 + closure + dependents, one Bun slice) built three
times from clean stores and clean work dirs against the SAME registry
snapshot (the input, fetched once):

| run  | how                                                      | index id                                                           |
| ---- | -------------------------------------------------------- | ------------------------------------------------------------------ |
| run1 | one invocation                                           | `c5fceec1c0d745e94c0ab0a4680e8e1d14946e8d2817dad8e5d1fc03ba1dd4fe` |
| run2 | one invocation, clean again                              | the SAME id                                                        |
| run3 | `--chunk 0/2` + `--chunk 1/2 --offline` + `--merge-only` | the SAME id                                                        |

The three index directories are byte-identical except `build.json`'s
`completed` timestamp and run3's `built 0 / reused 24` (the merge-only
leg's own counters — the chunks built the shards). The index id is a
function of (spec, registry snapshot, shard content); the sidecar never
enters it. Chunked execution produces the same bytes as one invocation.

The snapshot FETCHER was rewritten mid-experiment (npm view → one curl of
the full registry document per package — see the fetcher note under
"Fetch"). The determinism property is fetcher-independent (same snapshot
⇒ same id), and it was RE-PROVEN on the shipped fetcher: run4 (one
invocation) and run5 (chunked 0/2 + 1/2 `--offline` + `--merge-only`)
over the same snapshot, same id
`4aaf546c697e46fa73db5b6fce4d4fc287649e1c99790040189ce54e08677083`
(different from run1–3's because the snapshot's data changed with the
fetcher — that is the property working: the id tracks the inputs).

### Fetch

The registry snapshot is ONE `curl` of the full registry document per
package (`https://registry.npmjs.org/<name>`, what `npm install` itself
resolves against), retried once on transport failure, a 404 or an
unpublished tombstone recorded as an ABSENT packument in the snapshot so
an offline replay skips it exactly. The original two-`npm-view` fetch was
replaced after the first default-spec fetch exposed three defects in
npm view's multi-version output: it omits `dist.unpackedSize` and
`os`/`cpu` entirely (the spec's platform and size refusal rules were
DEAD on that data — fsevents' darwin restriction unread across all 67
versions), and it drops versions that define none of the requested
fields (ms: 19 of 32 versions vanished from the packument's metadata).
The full document carries all of it. Speed: the whole ~17.3k-package
snapshot fetches in ~5 minutes by curl versus ~6 hours of npm views.

### Absence is data; a list's absence is counted

The first default-spec plan aborted on "2,153 of 7,065 fetches 404" — two
defects stacked:

- the prefetch waves collected dependency-map KEYS without resolving
  `npm:` ALIASES (the key `@a2a-js/sdk-v0_3` is a local alias name; the
  real package is the value's target `@a2a-js/sdk`) — the registry 404s
  every ghost key. Fixed: `dep_target()` is the one alias resolver, used
  by the walk AND the waves; a test proves the ghost key is never asked
  for.
- the outage heuristic then misfired on a LEGITIMATE shape:
  `after-heavy-web3-son` (a real backlink-spam package) declares ~700
  fake dependency names, mostly unpublished — real dependency edges whose
  404s are genuine absence-as-data. 2,153 absent of 7,065 closure names,
  every one real. The heuristic is gone: fetch ERRORS fail loud (one
  retry inside `registry::load`), ABSENCE is data everywhere, and the
  planner judges it — a closure member's absence is skipped and recorded
  (2,313 on the default plan, dominated by the spam cluster), a LIST
  member's absence is counted against the list's total by
  `list_absence_outage`: more than 2% of a list absent is an outage (the
  bug shape a mis-encoding fetcher cannot avoid — a class of names 404s),
  at or under it the legitimate tail (packages removed since the pin).
  The default list's tail measured 213 of 17,338 = 1.2%, hand-verified
  real (removed/unpublished packages: `@contenthook/*`, `@deca-ui/*`,
  spam-looking names) — a hard per-name failure would have made the
  default spec permanently unbuildable.
- **enter() is the only decision point.** With the outage rule in, the
  live dry-run and its `--offline` replay still DIFFERED by exactly 505
  records: `drain()` recorded what a prefetch wave fetched-and-found
  absent, but the offline replay's snapshot already holds those absent
  packuments, its waves fetch nothing, and the walk never consults
  those deps (their edges are git URLs — the wave's `want` does not
  parse ranges). The plan's skipped records now come ONLY from enter()
  (the walk's actual decision point); the wave records nothing. Test:
  the same absent-dep edge in both registry shapes (the wave reports
  it / the snapshot holds it) must produce the same records. On the
  default spec after the fix: the live dry-run (`plan-default-v5.log`)
  and its `--offline` replay over the same snapshot are BYTE-IDENTICAL
  (193,618 lines — the plan is a function of (spec, snapshot), skipped
  records included).

### (b) Coverage vs the cb64 pins — plan-default-v5

The default spec resolved to **21,674 packages / 165,323 planned
versions** (roles: 77,568 dependent, 48,141 latest, 15,741 major-1,
12,293 major-2, 11,580 patch) against **3,321 refused versions**
(3,318 platform binaries, 3 pinyin OOMs) and **3,299 skipped packages**
(2,313 absent-from-registry, 985 `@types/*`, 1 list sentinel). cb64
(what exp152's pins cover): 66,523 pins over 9,897 packages.

|                                | bun-dev truth (1,934 rows, 232.1 MB) | bun-held truth (864 rows, 60.4 MB) | cc truth (43 rows) |
| ------------------------------ | ------------------------------------ | ---------------------------------- | ------------------ |
| exact-version bytes: spec plan | **83.0%** (1,705 rows)               | **92.1%** (792 rows)               | 0.0%               |
| exact-version bytes: cb64      | 80.9% (1,705 rows)                   | 84.5% (771 rows)                   | 0.0%               |
| package-level bytes: spec plan | **95.7%**                            | **97.7%**                          | 96.9%              |
| package-level bytes: cb64      | 92.7%                                | 95.8%                              | 97.3%              |

The plan is BIGGER than cb64 (the closure walk reaches where the
archive-era walk did not) and covers MORE of the truth bytes at BOTH
granularities on both Bun corpora. What the plan DROPS that cb64 had:
1,531 packages, and the top of them by bun-dev truth bytes is small —
`compact-encoding` 0.51 MB, `vsce` 0.32 MB, `hypercore` 0.30 MB,
`@audio/decode-mp3` 0.25 MB (dat/hyper and vscode-extension ecosystem
packages several edges deep from any top-impact package — outside the
one-edge-deep closure, the spec's designed scale limit). cc loses 2
packages (~3 KB: `@anthropic-ai/sandbox-runtime`,
`@pondwader/socks5-server`). The `pinyin` versions are refused
intentionally (measured OOMs, exp152).

### (d) Installability (npm `--before` ETARGET)

cb64's build ETARGET'd 270 pins (`npm install <pin> --before=<its publish
time>`): in every case the failure is a DEPENDENCY younger than the
pin — npm's `--before` applies one GLOBAL date to the whole tree, while
the spec's `dependents` rule resolves each edge at its own dependent's
publish time (per-edge, the semantics `ranges.rs` documents). The plan
selects 136 of those 270 pins (different resolver, same npm artifact
will recur at build time); a failed install records `FAILED <pin>` and
the build continues — the same behavior exp152's build had. The
plan's own selection never names a version the snapshot does not list.
