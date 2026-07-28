# 051 — Is the naming residual real? An audit before a lever

> **This is the BRIEF — a hypothesis, including its cautions.** Whoever finishes
> it stamps a STATUS block here naming which of its claims did not survive.
>
> **This experiment may correctly produce NO CODE.** 049 and 050 each shipped a
> large win; the plausible outcome here is "most of the remaining 7,714 lines are
> real change and classifier error, and the noise arc is done." That is a result,
> not a failure, and it is worth more than a lever built on a number nobody
> checked.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

**Read first:** [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md)
— eleven rules, and this brief opens by paying rule 3 again.

## Standing, after 049 and 050 [cold, 4 pairs]

| bucket               |   lines | share of noise |
| -------------------- | ------: | -------------: |
| **naming**           |   7,440 |      **96.4%** |
| alias (header lines) |     220 |           2.9% |
| reorder              |  **54** |           0.7% |
| **total noise**      |   7,714 |                |
| real change          | 140,728 |                |

**Reorder is closed** — 6,148 → 54 across 049 and 050, a 99.1% reduction. Naming
is now essentially the entire residual, and 85→86 alone carries 5,698 of the
7,440.

## The retraction this brief exists because of

A first pass classified the naming residual by driver and reported **"import
alias cascade, 5,166 lines, 69%"**. That was published before it was tested and
it is **wrong**.

The predicate asked _"is the changed identifier require-bound in this file?"_ —
which does not establish that the two sides name the SAME module. Resolving both
sides to their module paths on 215→216:

| classification                                |  lines |
| --------------------------------------------- | -----: |
| same module, different alias (**pure noise**) | **10** |
| **DIFFERENT module** entirely                 |    312 |
| only one side require-bound                   |      0 |

Genuine same-module alias churn on that hop is **10 lines**, not 322. Rule 3,
third time in this arc: _state in one sentence what the predicate actually tests,
and check that sentence is the claim you are making._

### What the 312 lines actually look like

```
setLimiter(table/config-sync/set-limiter.js)  ->  shellProcessor(lsp/command-processing/shell-processor.js)
headerProcessor(map-utils/retry-policy/…)     ->  simplifyText(table/renderer/simplify-text.js)
```

Those are not renames. They are references to **unrelated modules**, charged to
"naming" because `diff-composition` classifies a statement pair as naming when
the two share a statement hash and differ only in identifiers. Two structurally
identical statements that call different code satisfy that test perfectly.

## The hypothesis: the naming bucket is inflated by STATEMENT PAIRING

`classifyFile` pairs same-hash statements **FIFO within a file**. Where a file
holds several structurally identical statements — and the whole reason the
family/rotation work exists is that it holds many — that pairing is arbitrary. A
wrong pairing manufactures a naming instance out of two unrelated statements, and
charges every differing line to noise.

Two independent signs this is happening:

- The paths above: 312 of 322 "alias-ish" lines reference a different module.
- [`049/per-file-fidelity.ts`](../049-reorder-drivers/per-file-fidelity.ts)
  measured the decomposition's per-file error at **11.6%, cancelling 19.6× in the
  aggregate**, and localised it to **the real/naming attribution** — the
  token-overlap pairing in `editedLineCounts`. This is a mechanism for that.

**The `=` guards cannot catch this.** `novel` and `realLn` come from
`analyze.ts`'s hash-based statement classifier, a different code path; they held
exactly through 049 and 050 while this error was present in both.

## Tasks, in order — measurement only until task 3

### Task 0 — how much of the naming bucket survives a corroborated pairing

Re-run the decomposition with same-hash pairing that requires corroboration
(token overlap above a threshold, or a matching neighbour) instead of FIFO, and
report the naming total both ways, per hop. **The delta is the artifact.**

Do not "fix" `diff-composition` in place first — score both, so the correction is
visible and reversible. Every number in this arc came out of the current rule.

### Task 1 — how much is real change wearing naming's clothes

For naming instances whose two sides reference different modules, decide whether
the statement is genuinely the same statement. Read a sample; the population is
small enough. Report the split as lines, per hop, capped per file at what git
prints (`049/ambiguity-split.ts` has the capping helper).

### Task 2 — separate the shuffle pair from the calm hops

**85→86 carries 5,698 of 7,440** and is the pair where upstream reshuffles the
whole bundle. exp044 measured its biggest naming slice as **87.4% permutation —
exp036-irreducible rotation**. Averaging it with three calm hops hides both.
Report 85→86 and 118→119 / 197→198 / 215→216 separately, always.

### Task 3 — a lever, ONLY if something survives tasks 0-2

State the ceiling in git lines, capped per file, before writing code. If the
surviving reducible mass is under ~500 lines across four hops, **close the arc**
and stamp the STATUS block saying so — the harness cannot resolve smaller than
that (rule 11), and every remaining pass would be building against noise.

## Constraints any lever must answer

- **exp044's alias reservation FAILED**: +3,742 git lines, displacements
  unchanged at six. Refusing a name does not remove a collision, it moves it
  (rule 6). Show why yours is not whack-a-mole before building it.
- **exp044 also refuted the correspondence hypothesis**: the biggest naming slice
  is 87.4% permutation, i.e. rotation with no cross-version identity to recover.
- **Private fields (`#x`) are 422 lines** and no pass reaches them —
  `family-permute` collects TOP-LEVEL bindings only. That is a real, unclaimed
  slice, and the smallest coherent lever left if one is wanted.

## Gate — if anything ships

Unchanged from 050, plus one lesson each from its predecessors:

1. `noiseLn` and git-line noise **down on every hop**, judged per hop.
2. `novel` and `realLn` **unmoved** — and note they cannot see a pairing error,
   so they are necessary, not sufficient, here.
3. Zero NEW pure-rename violations (count the delta; `runtime.js` flakes on
   control legs too).
4. Boot gate green ×4, both legs. `bun` is at `~/.bun/bin`, NOT on PATH.
5. **Self-hop judged PINNED** (`049/pin-selfhop.sh`), never from a cold A/B —
   049 read 16 → 326 cold and it was entirely draw variance.
6. 118→119 is the canary. 050 cut its reorder to zero and its git-line noise rose
   198 anyway; a hop can move both ways at once.
7. **Confirm bundle-neutrality with a draw-pinned run** before reading
   bundle-level columns, which move ±2,800 on their own.

## Tooling

| script                     | question                                                                   |
| -------------------------- | -------------------------------------------------------------------------- |
| `051/naming-drivers.ts`    | naming by driver — **carries the wrong alias predicate; fix it as task 0** |
| `049/per-file-fidelity.ts` | does the decomposition match git per file                                  |
| `049/ambiguity-split.ts`   | the git-capped ceiling helper                                              |
| `048/read-moves.sh`        | what a pass actually shipped                                               |
| `049/pin-selfhop.sh`       | the self-hop, judged honestly                                              |
| `050/cold-ab.sh`           | the cold 4-pair gate                                                       |
