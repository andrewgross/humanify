# 074 — deterministic paths for modules that do not match

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
