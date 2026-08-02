# post-consolidation-rebased — cold 4-pair reference, 2026-08-02

Current `main` after the gate/library consolidation, scored COLD (every prompt
live) on the four gate pairs, **against the rebased priors in
`/work/exp050-cold/<v>-rebased`** — the same base every `exp05x` label used.

## Why this directory exists separately from `post-consolidation`

`run.sh` defaults its prior to the ARCHIVE tree in
`unpacked-claude-code/versions`, which an older pipeline produced. Scored that
way the same run reads **noise 21,016 on 85→86 against a 5,614 reference** —
3.7x, and entirely an artifact of the base. These JSONs are the same fresh
trees re-scored by calling `analyze.ts` directly against the rebased priors,
which costs minutes rather than the hours `REBASE_PRIOR=1` would.

Sibling `post-consolidation/` holds the raw run, including its archive-prior
numbers and the boot verdicts. Its `2.1.216.stats.json` is from a re-run: the
first attempt OOM'd at the old 14 GB heap.

## Numbers, with exp058-on (draw-pinned, same priors) in brackets

| pair    |          treeLn |         noise |  realLn | novel | vendorLn | relocSt |
| ------- | --------------: | ------------: | ------: | ----: | -------: | ------: |
| 85→86   | 32,046 [30,806] | 7,004 [5,614] |  78,791 |   787 |       82 |       0 |
| 118→119 | 37,187 [39,505] |   608 [2,658] |  79,124 | 1,154 |      153 |       0 |
| 197→198 | 52,294 [49,595] |   1,128 [540] | 136,396 | 1,261 |    3,004 |       0 |
| 215→216 | 25,776 [24,630] |     968 [240] | 122,066 |   986 |    1,889 |       1 |

Every `treeLn` delta is inside the **±2,800/hop** cold draw band. `realLn`,
`novel` and `relocSt` are **identical to the reference on all four pairs** —
including the single `relocSt` on 215→216, which is exp058's documented
shape-refusal witness.

**This is a cold run compared against a DRAW-PINNED reference.** The reference
has its variance suppressed, so this comparison cannot resolve an effect
smaller than the band (rule 11). It supports "nothing resolvable moved"; it does
not support "nothing moved".

Boot gate green on all four versions, both halves (`--version` and a live
`-p "say exactly: boot-ok"`).
