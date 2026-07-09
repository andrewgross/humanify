# Exp019 — class declarations matchable: noise share 40.8% (2026-07-09)

Branch `exp019-synonym-tail`. Goal metric: shared-lineage diff.

## Headline

| lineage-diff metric | exp018 | exp019    |
| ------------------- | ------ | --------- |
| noise hunks         | 1,504  | **1,288** |
| genuine hunks       | 1,914  | 1,868     |
| noise share         | 44.0%  | **40.8%** |
| rename occurrences  | 2,672  | **2,474** |

Campaign: noise hunks 22,998 → **1,288 (−94.4%)**; occurrence lines
47,761 → 2,474 (−94.8%); share 92.0% → 40.8%. Genuine stable in the
1,868–1,975 band through every round.

## The fix (`2564656`) — user-reported family

`ProcessEventManager→ProcessExitEmitter`, `BaseLifecycleModule→
BaseHandler`: `buildBindingMatchFingerprint` returned null for any
non-declarator binding path, so exp016's class visibility change put
classes in the NAMING pool but never the MATCHING index — both legs
fresh-LLM'd identical class bodies into synonyms every run, rippling
to every extends/new site (votes can't reach them either: extends
clauses and class-property initializers live outside matched
functions' slot walks).

A ClassDeclaration's own body is its hashable content — hash the node
like a binding init. Identical classes exact-match; empty/drifted ones
fall to the exp017/018 context and ordinal stages.

**Class-decl noise: 230 → 9 occurrences (6 pairs).** The asymmetric
bucket also dropped 199 → 145 (`index→i` left the top-10).

## Residual (2,474 occ, 1,093 bindings)

Top-10 is now entirely LLM synonym/decoration drift on close-match
remainders and unequal-count clone groups: `identityVar→identityVal`
(24, groups the ordinal gate correctly refuses),
`lastAssistantMessageHash→messageItem`, `queryString→queryText`,
`inputFilterSensitiveLog→inputFilter`. Per-identifier prior HINTS
can't fix these — a hint needs an identifier→prior-name mapping, which
only exists where alignment already transferred or a unique stem
already snapped. The remaining tail is diffuse LLM choice; each
candidate lever is worth ≲50–100 occurrences:

1. Prefix-aware stems (`init`/`initialize`) in snapping — narrow.
2. Asymmetric residue autopsy (145 occ).
3. Reroll floor: 58 occ.

## Reproduce

```bash
CHAIN_OUT=/tmp/exp019-chain bash experiments/016-diff-noise-convergence/run-chain.sh
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp019-chain/runtime-diff.txt 10
python3 experiments/016-diff-noise-convergence/classify-decl-kinds.py /tmp/exp019-chain/runtime-diff.txt /tmp/exp019-chain/cc-119-lineage/runtime.js 3
```

## Future (user, 2026-07-09)

Runtime verification: fetch recent npm versions (plain runnable
cli.js, unlike the binary decompiles), humanify with the same
pipeline, and execute the output as end-to-end proof the rename flow
preserves behavior — complements the structural invariant.
