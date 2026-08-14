# 071 — the dependencies our vendor detection cannot see

> **STATUS (2026-08-14): the planned detector was REFUTED by its own
> hand-check; a two-signal rebuild PASSES and is not yet wired.**
>
> - As planned (seeds + graph rules, no stability): moved ~1,554 modules
>   but the mandatory 20-module hand-check found **60–70% false
>   positives** — app code (`render-help-dialog`,
>   `emergency-tip-component`, `fetch-claude-bootstrap`) filed as
>   third-party. Not shipped. Only `stableSince` shipped (an additive
>   ledger field; see the agent's branch).
> - **Three findings**: (1) the planned wiring target is impossible —
>   `LibraryDetector` sees `__commonJS` factory files, a granularity at
>   which an `__esm` dependency does not exist; the owner must be the
>   split. (2) **Our vendor roster is contaminated** — it mixes real npm
>   ids with names our own vendor namer INVENTED (`http`,
>   `string-utils`, `config-processor`), so seeding on it matches
>   ordinary application English. (3) A leaf app module and a package
>   entry are indistinguishable in the import graph.
> - **The escape route, proven 2026-08-14**: fossil extraction runs on
>   RAW shipped bundles — no LLM, no naming (3,273 modules from raw
>   2.1.86, identical to the processed count). 124 release bundles
>   profiled offline in one background pass.
> - **Stability separates the classes decisively (raw basis)**:
>   dependencies survive a **median 124 releases** unchanged (98% ≥60),
>   app code **16** (17% ≥60). The undecidable middle behaves like
>   dependencies (median 124).
> - **TRAP, cost a false start**: hashes from RAW bundles and from our
>   PROCESSED output are not comparable — our own transforms move them.
>   The first join read "0 releases" for every class. Classification and
>   stability must share one basis, and the runtime story (pipeline runs
>   on processed code; the profile is raw) still needs an answer —
>   either read prior raw inputs at runtime, or let `stableSince` accrue
>   in processed basis.
> - **Two-signal rebuild (≥2 of: package vocabulary, graph position,
>   ≥60 releases stable; ANY app evidence vetoes)**: moves **644**
>   modules, leaves 2,629 in `src/`, and **20 of 20 hand-checked moves
>   are genuine dependencies** (AWS SDK credential providers, Azure MSAL
>   carrying its own version banner, lodash, base64/checksum helpers).
>   Conservative by construction — it declines the ambiguous middle.


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

## Implementation plan (2026-08-14)

### The constraint the census hides

The census's strongest signal — structural stability across **130
releases** — is NOT available at runtime. A live run sees exactly one
prior release, and over a single hop most APP modules are unchanged
too, so one-hop stability barely discriminates. Two ways out, and the
implementation must pick with data:

- **(a) Accumulate it in the ledger**: each release records, per module,
  how many consecutive releases its structure has been identical
  (`stableSince`), carried forward through the existing module match.
  Dependencies climb; app modules reset. Cold start (no history) yields
  nothing, so it must be an ADDITIVE signal, never a precondition.
- **(b) Drop stability from the runtime detector** and rely on the graph
  rules + seeds, keeping stability as an OFFLINE audit that measures the
  runtime detector's precision.

Recommended: build (b) first — it is complete on its own — and add (a)
as a ledger field the same increment, so history starts accumulating
immediately even if nothing consumes it yet.

### Seeds without app-specific vocabulary

The census seeded from hardcoded app words (`claude|anthropic|…`). That
is fine for a census and WRONG for the pipeline — it would only work on
one target. Generic replacements, in preference order:

1. **The app entry**: the bundle's entry module and the eager zone are
   app by construction.
2. **Content identity against the KNOWN vendor corpus** (exp046's
   content-keyed vendor identity, already an owner): an ESM module whose
   content matches a package already detected in CJS form is that
   package. Widen that owner; do not write a second matcher.
3. **In-module package evidence**: surviving license/banner fragments,
   `require()` target strings naming a package, registry URLs, and
   package-shaped export surfaces — all target-independent.

### Ladder, gates, ownership

Run seeds → the four graph rules (all target-independent, all proven:
deps never import app; a dep's imports are deps; imported only by deps ⇒
dep; closure all deps ⇒ package entry) → optional stability. Ship with
**precision over recall**: a module moves to `vendor/` only on ≥2
independent signals, and the run REPORTS what each signal alone would
have moved (rule 11 — an unlogged reclassification cannot be audited).
Wire into `selectLibraryDetector` (stage 4) as bun's fossil-aware
detector; vendor naming stays with `vendorNamer`/`priorVendorNames`.
Kill switch: `--disable esm-vendor-detection`.

### Validation

1. **Offline**: run the detector over the saved exp070-r1 trees; compare
   its classification against this brief's census (app 1,562 / vendor
   1,089 / unknown 622, rollup ~1,760 / ~1,510); hand-check 20 moved
   modules for false positives — the house rule is precision.
2. **Cold scored run, exp070+exp071 together** (one relayout): gates are
   `novel`/`realLn` byte-exact, boot ×4 OK, cache +0, self-hop ≤ 1 ask.
3. **The value question**: on the fossil-vs-fossil hop, `src/` alias
   lines must fall well below 21,971 and hidden name-only churn below
   exp070-r1's 1,926 (the pre-fossil baseline is ~1,480 — beating THAT
   is what makes the whole fossil arc worth merging).
4. App file count should land near ~1,900 (Andrew's ground truth).
