# 041 — Content-anchor file inheritance (the top remaining diff-noise lever)

> **STATUS (2026-07-26): SHIPPED and superseded.** The content-anchor tier and the
> all-same vote tier landed and cut cross-file relocation **50.5%** (15,699 →
> 7,764 git lines), down on every hop including the 118→119 canary (791 → 16).
> See [RESULTS.md](./RESULTS.md).
>
> The title is no longer true: relocation is now the SMALLEST axis (1,390 lines)
> after [042](../042-anchor-preempt/) and [043](../043-name-family/) took it a
> further 82%. **Refuted inside this document:** the "outer names only" variant,
> which regressed the 118→119 canary — do not revive it. The gate script
> (`gate-verdict.sh`) and `replay-lib.ts` here are still the current tools.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
research-log entries read _Idea → Evidence (table) → Conclusion_; outcomes are
**landed** or **failed** with numbers; totals-first tables; **ceilings measured
before builds**; every hop judged **on its own** — a big hop masks a regression
on a small one, which is exactly how Lever B v1's 118→119 regression hid.

Read [exp040 FINDINGS.md](../040-diff-census/FINDINGS.md) first, then
[exp038 RESULTS.md](../038-dependency-aware-reorder/RESULTS.md).

## Why this experiment exists

exp038 removed emit-order churn. exp040 then censused what is actually left in
the diff a human reviews, per hop, in git lines. Cross-file **relocation** is the
largest remaining noise source **on every hop** — ahead of naming:

| hop     | **relocation** | naming | reorder | alias | share of hop |
| ------- | -------------: | -----: | ------: | ----: | -----------: |
| 85→86   |      **7,583** |  5,740 |   1,816 |    28 |    **15.0%** |
| 197→198 |      **6,205** |    780 |   1,956 |    44 |    **11.8%** |
| 215→216 |      **1,842** |    754 |   1,950 |    74 |     **5.8%** |
| 118→119 |        **791** |    456 |     258 |    54 |     **2.1%** |

A statement that changes file is the most expensive kind of noise: git renders it
as a delete in one file and an add in another, and it drags its `require` headers
and `Object.defineProperty` export lines with it.

## What is happening, with proof

85's `session/plan-review/status-message.js` and 86's
`completion/decision/decision-reason.js` share **263 byte-identical lines** — the
whole `exitPlanMode` approval-tool object, prose included. The marker string
`"Wait for the team lead to review your plan"` is present in 85's
status-message, present in 86's decision-reason, and **absent from 86's
status-message**. Verify it yourself in 30 seconds:

    W=/tmp/eval-work/exp040-ledger
    grep -c "Wait for the team lead to review your plan" \
      $W/2.1.85-rebased/src/session/plan-review/status-message.js \
      $W/2.1.86/src/completion/decision/decision-reason.js \
      $W/2.1.86/src/session/plan-review/status-message.js

### Why the existing tiers let it move

The code lives in a **minted-name lazy-init block**:

    var initializeApp256 = (0, resourceLifecycle.lazyInitializer)(() => { … });

`assignWithPrior` (`src/split/stable-split.ts`) inherits a statement's file from,
in order: the **hash tier** (identical structural hash, all prior occurrences in
one file), the **name vote** (its declared names' prior files), the **identity
tier**, then **residue by locality** (follow the preceding neighbour).

- The hash tier abstains: the statement was edited slightly between releases
  (263 of ~280 lines identical), so its structural hash flips.
- The name vote abstains: the name is a minted counter (`initializeApp256`), and
  it re-mints to a different number next release, so there is nothing to vote on.
- So it falls to **locality** — and when upstream reshuffles the bundle, its
  neighbours change and it lands in a different file.

Population on 85→86: the split reports
`inherited 19044/19966 (10954 via hashes, 466 via ordinals, 922 residue by locality)`.
**922 statements per hop are placed with no identity evidence at all.** 2.1.86 has
3,273 `lazyInitializer` blocks, 1,868 of them minted-named.

## The idea

Add a **content-anchor tier** between the name vote and locality: a statement's
CONTENT identifies it even when its hash flipped and its name re-minted.

Index the prior release's statements by every **rare string literal** they carry
(12+ characters, occurring in ≤1 statement per side). For a fresh statement with
no hash/name/identity verdict, look up its own rare literals. If they resolve to
**exactly one** prior statement, and that statement is plausibly the same code,
inherit its file. Anything ambiguous abstains to locality — precision over
recall, the same rule every other tier uses.

### The similarity gate is load-bearing — do not omit it

A shared rare literal is **necessary but not sufficient**. Measuring this without
a gate paired a **5,073-line** prior statement with a **7-line** fresh one because
they shared one string, and charged 5,080 lines for it; 215→216 read 8,956
instead of 1,842. Use the project's existing rule (`tokenSet` from
`experiments/034-eval-harness/diff-ledger.ts`): **≥50% token overlap**, the same
test `diff-composition.ts` uses to decide "this is an edited version of that".

## The work, in order

### A. Ceiling first — do NOT build before this

Extend `experiments/040-diff-census/relocation-churn.ts` (it already does the
matching) to answer, per hop:

- of the **922 locality-placed statements**, how many get a **unique** rare-literal
  anchor that passes the similarity gate?
- how many git lines does that recover, and how many statements would the tier
  place **differently from where they are now** (the churn it would prevent)?
- how many would it place into a file that DISAGREES with a tier that did fire —
  that number must be 0 by construction, since the tier only runs when the others
  abstained, but measure it to be sure the ordering is right.

**That table is the ceiling.** If the anchorable share is small, stop and write it
up as the conclusion.

### B. The anchor index

A pure, unit-tested function in `src/split/` — no assignment changes yet. Given
prior statements (text + file) and fresh statements, return `Map<freshIndex,
file>` for unique, similarity-passing matches only. Property tests: an ambiguous
literal yields no verdict; a literal shared with a dissimilar statement yields no
verdict; a statement with no rare literal yields no verdict; the result never
depends on iteration order.

Reuse, do not re-implement: `tokenSet` for the gate, and check whether
`src/prior-version/statement-twin.ts` already builds something close (it does
unique-twin bridging by hash — the same shape, different key).

### C. Wire it into `assignWithPrior`

A new tier between the name vote and locality, behind
`HUMANIFY_NO_CONTENT_ANCHOR=1` so it can be A/B'd and killed. TDD — red test
first. The prior statement TEXT is needed at assignment time; the pipeline
already parses the prior bundle for the reconcile pass, so thread it rather than
re-parsing (check `--prior-version` plumbing in `src/commands/unified.ts`).

### D. Adjacent, cheap: minted names on new code

exp040 spotted `function doNothing3()` in genuinely new code on 215→216. The
minted-name census is a separate lever, but note any instances you see — the
project goal is zero minted tokens.

## Guardrails (each was learned the hard way)

- **Precision over recall.** A statement placed in the WRONG file is far worse
  than one left to locality: it churns two files plus every importer. Unique
  match or abstain.
- **Never trust a match you have not eyeballed.** Four hypotheses were refuted in
  the session that produced this brief, one of which fit the arithmetic perfectly
  (`3,833 ÷ 276 ≈ 14 lines each`) and was still wrong. List the top pairs and read
  them before believing a number.
- **Boot gate is mandatory** — `cd <out> && bun run.cjs --version` must echo the
  version.
- **Self-hop must stay byte-identical in BOTH artifacts** — bundle _and_ split
  ledger. It only just went green (exp040); a regression there is a blocker.
  exp037's sweep compared only bundles, which is how a real defect hid.
- **Per hop, never aggregated.** 118→119 is the regression canary (2.1% noise,
  almost nothing to win); 85→86 is the biggest prize.
- **`npm run check`** + `npx biome check <file>` before committing. Pre-commit
  biome is stricter on complexity than `check`.
- **Never edit `src/` while a pipeline run is in flight** (`pgrep -f 'src/index.ts'`).
  The eval spawns fresh processes per pair, so an edit mid-sweep silently splits
  the run across two versions of the code.
- 24 GB box: a pipeline run peaks ~14 GB. Do **not** run heavy analysis
  concurrently with a pipeline.

## How to run everything (copy-paste)

    cd /Users/andrewgross/Development/humanify   # branch exp041-content-anchor

    # unit + lint + fingerprint (fast, run constantly)
    npm run check

    # the reference trees this brief measured (prior + fresh per hop)
    W=/tmp/eval-work/exp040-ledger

    # what the diff is made of, per hop
    npx tsx experiments/040-diff-census/line-census.ts \
      "$W/2.1.215-rebased/src" "$W/2.1.216/src" "215->216"

    # relocation sizing + the largest pairs (ALWAYS eyeball --list)
    NODE_OPTIONS="--max-old-space-size=10240" npx tsx \
      experiments/040-diff-census/relocation-churn.ts \
      "$W/2.1.215-rebased/src" "$W/2.1.216/src" "215->216" --list 8

    # real/naming/alias/reorder split, git lines
    NODE_OPTIONS="--max-old-space-size=12288" npx tsx \
      experiments/037-noise-source-decomposition/diff-composition.ts \
      "$W/2.1.215-rebased/src" "$W/2.1.216/src" "215->216"

    # ONE hop end-to-end (~12 min warm cache), then its self-hop + ledger check.
    # PRIOR_TREE is REQUIRED here: the script's default points at the older
    # leverb-sweep naming (<TO>-rebased); the eval names its rebased priors
    # <FROM>-rebased, and those older trees carry a pre-exp040 ledger.
    PRIOR_TREE=$W/2.1.215-rebased \
      experiments/038-dependency-aware-reorder/selfhop-ledger-check.sh 2.1.216 exp041

    # THE GATE: all four pairs + boot + self-hop + layout KPI (~2.5 h)
    REBASE_PRIOR=1 EVAL_BOOT_PROMPT=0 \
      experiments/034-eval-harness/run.sh exp041-anchor
    npx tsx experiments/034-eval-harness/leaderboard.ts \
      baseline-main exp040-ledger exp041-anchor
    npx tsx experiments/034-eval-harness/summarize.ts exp041-anchor   # per-hop layout table

`REBASE_PRIOR=1` regenerates each base version with the current pipeline first.
Keep it: the reference model `exp040-ledger` was scored that way, and comparing a
non-rebased run against it is not like-for-like.

**If `/tmp` was cleared**, the reference trees are gone — re-run the gate once to
regenerate them (`run.sh` writes `$W/<v>` and `$W/<from>-rebased`), then measure.
The LLM cache at `/tmp/eval-work/llm-cache` is what makes reruns cheap and pins
naming across an A/B; losing it means the first rerun is slow and its naming may
drift, which invalidates a same-session comparison.

Committed reference results live in `experiments/034-eval-harness/results/`:
`exp040-ledger` is the model to beat, `baseline-main` and `archive-shipped` are
the long-standing references.

## Success criterion

Relocation churn **down on every hop** (`relocation-churn.ts` total), with

- no regression on 118→119,
- `novel` / `realLn` unmoved in the 034 gate (dropping real change is a
  regression, not a win),
- all four trees booting,
- self-hop byte-identical in bundle AND ledger,
- `reorder` and `naming` not made worse,

— or a measured, written-up ceiling showing the anchorable share is too small,
which is an equally good outcome. Write it into `RESULTS.md` here either way.
