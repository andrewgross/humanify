## Addendum — the same test, asked of the WHOLE hop

Task A.2 tested permutation inside single statements. Asked globally — of every
substitution on a hop, is the fresh name also a name being replaced somewhere? —
the two regimes separate completely:

| hop     | overall conservation |  1 sub | 2 subs | 3–5 |  6+ |
| ------- | -------------------: | -----: | -----: | --: | --: |
| 85→86   |            **78.8%** |     8% |    66% | 79% | 84% |
| 118→119 |                37.3% | **0%** |    14% | 53% | 82% |
| 215→216 |                15.4% | **0%** |     9% | 24% | 19% |

Two findings:

1. **Conservation rises monotonically with substitution count.** Rotation is not
   confined to the 6+ bucket that Task A.2 examined — the 2-substitution (66%)
   and 3–5 (79%) buckets on 85→86 are largely redistribution too. That hop is
   mostly rotation at every scale.
2. **A 1-substitution change is never rotation** (8% / 0% / 0%). Those are
   genuine new names, and they are what the calm hops are made of.

So the tractable residue is the calm hops' non-alias single- and
double-substitution renames — roughly **1,460 git lines** after removing the 566
of alias churn — plus 85→86's ~21% non-conserved share. Call it **~2,600 of
7,616 naming lines, about a third**, that is neither exp036-irreducible rotation
nor the failed alias class.

That is the honest target for any future naming work, and it is smaller than
relocation was when exp041 started (15,699). Whether it is worth attacking at
all is a judgement about diminishing returns, not a measurement question.
