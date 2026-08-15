# 074 — deterministic paths for modules that do not match

> **STATUS (2026-08-15): BUILT — but the brief's premise was WRONG and
> was corrected before code. Validation run in flight.**
>
> **Path churn is not mint randomness.** Matched modules inherit paths
> perfectly (2,182 matched, ZERO moved). Measured decomposition of the
> 4,592 churned require lines on 85→86:
>
> | cause | lines |
> | --- | ---: |
> | sit in genuinely new files | 473 |
> | target exists in prior (alias/name churn, not placement) | 543 |
> | target absent from prior — real path instability | 3,576 |
> | …of which **ONE module** causes | **3,204 (90%)** |
>
> That module grew 17→18 statements: overlap 0.75 against its prior
> self with a single import edge, so the existing tier could license it
> neither way. It minted fresh, landed elsewhere, and all 3,204
> importers churned — **while its stem was identical in both runs**.
>
> **Fix shipped: a stem-corroborated match tier** (stem unique among
> unmatched modules on BOTH sides + overlap ≥ 0.7), priced before
> building:
>
> | threshold | pairs | churned lines held |
> | ---: | ---: | ---: |
> | 0.5 | 45 | 3,244 |
> | **0.7** | **44** | **3,204** |
> | 0.8 | 43 | **0** (the module that matters sits at 0.75) |
>
> Verified on the real bundle: inherited paths 3,092 → **3,136**.
> Mispairing between content-twins is harmless by construction.
>
> **Folder quality (same increment):** consensus-then-collapse takes
> 809 → **280 folders**, median 4 files/folder, root held at 1,085
> (collapse alone reaches 225 but inflates the root 1,013 → 1,590 —
> order matters). Eager zone renamed to `src/index.js`: it IS the entry
> file. ~1,000 genuinely-shared files stay flat and counted.
>
> **Carry-forward finding:** folder rules only bite on a FIRST run —
> with a prior ledger, 3,136 of 3,273 modules inherit paths (correctly;
> that IS the stability property), so a relayout to 280 folders
> requires scoring without a prior ledger, or accepting that the folder
> shape arrives once and then freezes.


> **This is a BRIEF — a hypothesis.** Its sizing came free from
> exp073's ceiling decomposition (2026-08-15).

## The finding that motivates it

Under fossil layout, **path churn dwarfs name churn**:

| class | churned lines (85→86) |
| --- | ---: |
| require lines pointing at a path absent from the prior tree | **5,728** (26% of all imports) |
| body name churn a name-carry could reach | 944 |
| genuine change | 308 |

And it is extraordinarily concentrated: **179 distinct targets** cause
all 5,728 lines (~32 each), and **one universally-imported helper
(`access-property.js`) accounts for 3,272**.

Path CARRY already works — of 2,182 provably-matched modules, **zero
moved**. The damage comes from modules that do NOT match: ambiguous
twins and genuinely-new modules mint a fresh path each run, and every
importer of a re-minted module churns a require line.

## Hypothesis

Minted paths for unmatched modules can be made deterministic — derived
from content and graph position rather than from discovery order — so
an unmatched module lands on the same path every run, and its
importers stop churning. This needs no naming change and no
classification.

## Design constraints

- **Never positional.** exp035/036 measured positional NAME assignment
  at +50,606 lines; the same trap applies to paths. Derive from
  content (fingerprint), the module's importers, and its stem — never
  from the order the walk discovered it.
- Twins are indistinguishable by content BY CONSTRUCTION, so their
  paths must be derived from something stable that DOES differ: their
  importer set. Two identical modules imported from different places
  are different modules for placement purposes.
- Matched modules keep inheriting verbatim; this only governs the
  unmatched remainder.
- LOG every minted path with its derivation input (rule 11).

## Success criterion (fixed now)

Cold scored run (combined with exp073 and the folder work, one run for
the stack): `novel`/`realLn` byte-exact, boot ×4, cache +0, self-hop
≤ 1 ask. Success = require-line churn on 85→86 falls well below 5,728,
and hidden name-only churn falls below exp070-r1's 1,926.
