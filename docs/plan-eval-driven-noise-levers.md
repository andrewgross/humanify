# Plan: eval-driven noise-reduction levers

Written 2026-07-21, after the cross-version eval harness (`experiments/034-eval-harness`)
landed. This plan is the forward work; nothing here is implemented yet. Every
lever is graded by the harness — build behind a precision gate, then
`run.sh <label>` and `leaderboard.ts archive-shipped baseline-main <label>`, and
require the **reducible** KPIs (`noise`, `reloc`, `mints`) to fall while **real**
change (`novel`, `realLn`) stays put. A change that "reduces noise" by dropping
real change is a regression.

## Where we stand (the baseline this plan improves)

Totals across the 4 eval pairs (85→86, 118→119, 197→198, 215→216):

| model                         | noise | noiseLn | reloc | novel | realLn |
| ----------------------------- | ----- | ------- | ----- | ----- | ------ |
| archive-shipped (git history) | 5004  | 110447  | 835   | 4188  | 416377 |
| baseline-main (current)       | 3936  | 93970   | 746   | 4188  | 416377 |

The current pipeline already cuts noise ~21% vs what shipped, with `novel`/`realLn`
identical (pure noise reduction). Per-statement the split is ~92.7% clean / 3.5%
noise / 3.8% novel. Determinism: 98% of 64,493 functions are deterministic; only
3.3% reach the LLM (876 close-match + 1,259 cold). The ~20k-line run-to-run floor
is those functions' internal locals.

**How the eval classifies (the ideas these levers reuse):** noise vs novel comes
from the split's rename-invariant `statementHash` (identifier-blind) — a statement
whose hash is in both versions is structurally unchanged (noise if its text
differs, clean if identical); a novel hash is real change. `reloc` is a ledger
`nameToFiles` diff (same name, moved file). Both are the same rename-invariant
hashing the pipeline already runs on — these levers push it further.

---

## Lever 1 (lead): statement-level wholesale name inheritance

**Targets:** the `noise` bucket — the biggest reducible churn. Every noise
statement has a structural twin in the prior (that's why its hash matches), so it
is NOT new code; it got re-named because the matcher works at a finer
function-fingerprint granularity and called it _close_/ambiguous (or a transfer
was rejected), sending it to the LLM.

**Idea:** when a top-level statement's rename-invariant `statementHash` **uniquely**
matches a prior statement, transfer **all** its bindings' names positionally (the
hash walk visits bindings in the same order) — deterministic, no LLM. A coarser,
whole-subtree inheritance tier above the function matcher.

**Step 1 — MEASURE the ceiling (cheap, no LLM, do this first):** of the noise
statements, how many have a _unique_ prior hash-twin (hash count 1 on both sides →
unambiguous transfer)? Extend `analyze.ts` (or a one-off) to report it across the
pairs. That fraction is the safe upper bound; if most noise statements have a
unique twin, this is a large deterministic win.

**Precision gate (critical):** `statementHash` masks callees and free identifiers
too, so two same-shaped statements calling _different_ things hash equal.
Transferring on hash alone would mis-name. Require, in addition to the unique
twin, **callee/content corroboration** — reuse `bindingRolesAgree`
(`src/prior-version/binding-role.ts`, the veto fix-1 already uses). Transfer only
on unique twin **and** role agreement; abstain otherwise.

**Where:** a new tier in the rename pipeline (`src/rename/prior-transfer.ts` /
match application), running **before** the LLM pass — pin the hash-twin
statements' names, let the LLM handle only the residue.

**Validate:** `run.sh`, leaderboard vs `baseline-main` — expect `noise` down,
`%det` up, `novel`/`realLn` unmoved. Spot-check preempted transfers (right name on
right binding). Risk: MEDIUM — a coarse-hash false twin mis-transfers; the role
gate + unique-twin requirement is the defense; measure away-from-reference
regressions.

---

## Lever 2: same-name relocation stability (`reloc`)

**Targets:** `reloc` — same-name bindings whose home file moved, dragging every
importer's `require`-alias. Only ~0.4% of names but power-law (a few heavily
imported barrels dominate the churned lines).

**Idea:** part-4's "approach E" — a positional/neighbor fingerprint for
**non-unique** names in the split's `assignWithPrior`. A name declared in several
prior files makes the name-vote abstain, so the binding drifts to locality and
can land in a different file. Give it a stable positional identity (position in a
stable enclosing structure, neighbor set) to disambiguate.

**Step 1 — measure:** of the `reloc` bindings, how many have a positional/neighbor
signal that uniquely maps to a single prior file? Ceiling, deterministic
(split-only, no LLM — like the B-ceiling harness).

**Precision gate:** unique + unanimous positional match only; abstain to locality
otherwise (the split's existing precision-over-recall discipline).

**Where:** `src/split/stable-split.ts` `assignWithPrior`, a tier between ordinal
and locality. **Validate:** split is deterministic given the humanified input →
measure ON/OFF directly; `reloc` down, concat-equivalence invariant holds. Risk:
MEDIUM (touches split assignment; new signal) — the invariant + gate contain it.

---

## Lever 3: close-match determinism (shrink the floor at its root)

**Targets:** the ~20k-line run-to-run LLM floor (876 close-match + 1,259 cold
functions' local naming). Not a cross-version lever per se, but it makes the noise
**deterministic** so reruns reproduce and the eval's `noiseLn` stabilizes.

**Root cause:** concurrent batch-serving — each LLM call's `usedNames` context
varies with completion order (temperature is already 0). The close-match 876
_have_ a prior; they shouldn't need the LLM at all for the slots we could transfer.

**Idea (two angles):** (a) determinize the context — fixed batch ordering or a
frozen `usedNames` snapshot per batch; (b) transfer more close-match slots
deterministically so fewer reach the LLM — i.e. widen statement-alignment (the A1
lever already on main). (b) overlaps Lever 1 and is likely the higher-leverage
half.

**Step 1 — measure:** run the same pair twice on identical input (the floor); split
the variance into close-match vs cold, and estimate how much determinizing context
/ widening transfer removes.

**Where:** the LLM batch layer (`src/rename/processor.ts`, batch ordering /
usedNames snapshot) and `src/prior-version/statement-align.ts` (widening).
Risk: MEDIUM (concurrency/serving layer; watch throughput).

---

## Suggested sequencing

1. **Lever 1 ceiling measurement** — an hour of no-LLM analysis that says whether
   the biggest noise bucket is deterministically recoverable. Gate the build on it.
2. **Lever 1 build** (gated) — the largest reducible-noise reduction.
3. **Lever 2** (`reloc`) — independent, smaller, deterministic, split-local.
4. **Lever 3** — determinism/floor; overlaps A1 widening, do the transfer-widening
   half first.

Each step ends at the eval gate: `leaderboard archive-shipped baseline-main <label>`,
reducible KPIs down, real change unmoved, precision spot-check clean.
