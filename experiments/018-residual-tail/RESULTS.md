# Exp018 — neighbor-statement context for bindings: noise drops below genuine (2026-07-09)

Branch `exp018-residual-tail`. Goal metric: the shared-lineage diff
(`experiments/016-diff-noise-convergence/run-chain.sh`).

## Headline — the reviewability line crossed

| lineage-diff metric | exp017 end | exp018    |
| ------------------- | ---------- | --------- |
| noise hunks         | 2,516      | **1,504** |
| genuine hunks       | 1,890      | 1,914     |
| noise share         | 57.1%      | **44.0%** |
| rename occurrences  | 4,499      | **2,672** |

**Noise is below genuine change for the first time** — a reviewer
reading the diff now sees real changes as the majority. Campaign:
noise hunks 22,998 → 1,504 (−93.5%); occurrence lines 47,761 → 2,672
(−94.4%); noise share 92.0% → 44.0%. Genuine stable throughout.

## The fix (`e87ccb7`)

The user-reported family (`initModule13→initializeDx8`,
`initOx8→initUIK`): module bindings whose inits are structurally
identical thunks — `var X = lazyInit(() => { a = b; })` where the
assignment operands are binding slots, so every clone hashes the same.
Referenced exactly once (below the two-vote floor), unreachable by the
cascade → fresh-LLM-named in both legs every run, with names derived
from the leg's own minified token (`initOx8` embeds v119's `Ox8`,
`initUIK` embeds v120's `UIK`) — guaranteed divergence.

Module bindings now get a context hash: the NEIGHBORING statements of
the declaration (the declaration itself is the clone; the statements
around it carry the identity — the binding analog of exp017's
enclosing-statement stage for functions). `buildBindingFingerprintIndex`
carries its nodes; the same resolver + ordinal-pairing stage applies
unchanged. Binding-side policy tests updated to the exp017 pinned
policy (equal-count identical-context clones pair by source order).

The −40% step is far larger than the visible init family because each
transferred binding also stabilizes its referencing context (usedNames,
collision landscapes, vote evidence) downstream.

## Residual (2,672 occurrences, 1,163 bindings, ~2.3 occ each)

- `index→i` (32) + `H→builderContext` (14): asymmetric one-leg
  coverage holes — 199 occ bucket total.
- `identityVar→identityVal` (24): the unequal-count /
  drifted-context clone groups the ordinal gate correctly refuses.
- Synonym drift on close-match remainders
  (`lastAssistantMessageHash→messageItem`, `queryString→queryText`).
- Prefix-variant snapping gap: `initSupportedPlatforms →
initializeSupportedPlatforms` — stem-stripping handles suffixes
  only; init/initialize prefix variants don't snap. Small, noted.
- Reroll floor: 57 occurrences.

## Remaining levers (all small)

1. Per-identifier prior hints in FUNCTION prompts (synonym drift).
2. Prefix-aware stems in prior-name snapping.
3. Asymmetric autopsy (`index→i`).

## Reproduce

```bash
CHAIN_OUT=/tmp/exp018-chain bash experiments/016-diff-noise-convergence/run-chain.sh
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp018-chain/runtime-diff.txt 10
```
