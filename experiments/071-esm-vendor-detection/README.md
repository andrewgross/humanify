# 071 — the dependencies our vendor detection cannot see

> **This is a BRIEF — a hypothesis, including its cautions.** Its Task 0
> census is ALREADY EXECUTED (2026-08-14, `census.ts`); the numbers below
> are measurements, not guesses.

## Why this exists

exp070's fossil split emits one file per bundled module — and the tree
came out at **3,274 app files** when Andrew's comparison against real
source of a similar version says the project has **~1,900 files in ~300
folders**. The fossil split is not over-splitting: it faithfully
reproduces the bundle's module graph, and that graph contains npm
dependencies **shipped as ES modules**. Our library detection keys on
Bun's CommonJS factory form (banner comments, `__commonJS` wrappers) and
correctly diverts 1,592 files to `vendor/`; an ESM-form dependency is
compiled into exactly the same shape as app source (`__esm` init, one
per module) and is therefore invisible to it.

The cost is not cosmetic. Those modules carry their own require aliases
into `src/` (21,971 alias lines under fossil layout vs 19,015 before)
and sit outside the vendor machinery that holds library code still
across versions — which is why exp070's first fossil-vs-fossil hop
measured hidden churn UP (1,926 ln vs ~1,480), not down.

## Task 0 census — EXECUTED (2026-08-14, on the exp070-r1 2.1.86 tree)

Three independent signals, applied in order. Seeds from content markers,
then two sound graph rules, then cross-version stability:

| signal | rule | result |
| --- | --- | --- |
| content markers | app vocabulary vs package vocabulary | seeds: 829 app / 242 vendor |
| graph rule 1 | a module importing app code is app (deps never import app) | — |
| graph rule 2 | everything a dependency imports is a dependency | APP 1,562 / VENDOR 868 / UNKNOWN 843 |
| graph rule 3 | imported ONLY by dependencies ⇒ dependency | — |
| graph rule 4 | whole import closure is dependencies ⇒ package ENTRY (233 found) | APP 1,562 / VENDOR 1,089 / UNKNOWN 622 |
| stability | structurally identical 2.1.86 → 2.1.216 (130 releases) | app 8.2% stable · vendor 85.9% · unknown 67.5% |

**The classes separate cleanly**: app code churns (92% changed over 130
releases), dependencies do not (86% unchanged). The undecidables behave
like dependencies (67.5% stable ⇒ ~420 of 622 are packages).

**Rollup: app ≈ 1,760, dependencies ≈ 1,510** — independently
corroborating Andrew's ~1,900-file ground truth from real source.

## The lever

Extend library detection to ESM-form modules using exactly this ladder
(markers seed → graph rules bulk → stability resolves the tail), so
~1,500 modules move to `vendor/`, their alias surface leaves `src/`, and
the app tree lands near the real project's file count.

## Cautions pinned before building

- Stability is measured on STRUCTURE (rename-blind statement hashes), not
  bytes — a renamed-but-unchanged app module also reads stable. The
  signal is a discriminator in aggregate, never proof for one module.
- Precision over recall (house rule): a misclassified APP module hidden
  in `vendor/` is worse than a dependency left in `src/`. Require
  agreement of at least two signals to move a module, and COUNT what
  each signal alone would have moved.
- 321 import-less leaves carry no graph signal at all; if stability
  cannot resolve them, they stay in `src/` and are reported, not guessed.
- Vendor identity across versions already exists (content-keyed, exp046)
  — widen that owner rather than adding a second vendor path.
- Sequencing: this ships BEFORE exp070 merges, so the tree relayouts
  once, not twice.
