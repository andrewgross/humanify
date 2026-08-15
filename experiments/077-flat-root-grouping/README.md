# 077 — group the flat root

> **This is a BRIEF — a hypothesis, and NOT STARTED.** Recorded 2026-08-15
> at Andrew's request ("I don't want 1000 files or folders at a single
> level, I wonder how we can better group those — add it to our todos").
> Its sizing came free from exp076's folder census and is recorded here so
> whoever picks this up does not have to re-derive it.

## The finding

A third of the emitted tree sits flat under `src/` with no folder at all:
**1,040 of 3,261 files** on a first run, 1,081 of 3,273 on the next.
Deepest nesting is 2; the largest folder holds 97 files.

## What those 1,040 files actually are — measured, not assumed

Two questions decide the fix, and they have opposite answers:

**By the signal that placed them:**

| placed by         | files | meaning                                     |
| ----------------- | ----: | ------------------------------------------- |
| flat (no signal)  |   578 | nothing could place it — needs a NEW signal |
| dominant-importer |   162 | placed, then EVICTED                        |
| anchor            |   138 | placed, then EVICTED                        |
| barrel            |   118 | placed, then EVICTED                        |
| co-importer       |    44 | placed, then EVICTED                        |

**So 462 of the 1,040 (44%) were successfully placed and then thrown back
to the root by `collapseSmallFolders`** — the rule that dissolves any
folder holding fewer than `MIN_FOLDER_FILES` (3). Those do not need a new
idea at all; they need a less blunt eviction rule.

I had assumed, from exp074's note that the root was ~1,013 files _before_
collapse, that the root was mostly files no signal could reach. That is
wrong by 44%, and the two halves want different work. Measured with
`experiments/076-statement-placement/folder-shape.ts`.

**By how many other files import them:**

| importers |   files | reading                                       |
| --------: | ------: | --------------------------------------------- |
|         0 |     110 | imported by nothing — entry-ish or dead       |
|         1 | **237** | an unambiguous home exists and we declined it |
|         2 |     107 | two candidates                                |
|       3–5 |     295 | genuinely shared                              |
|        6+ |     291 | genuinely shared                              |

## Two levers, and they are not the same lever

1. **Loosen the eviction rule (462 files, cheap, structural).** A file with
   exactly ONE importer has an unambiguous home; today it lands at the root
   only because the folder it would form was too small to keep. 237 root
   files are in that position. The rule should not be a flat file count —
   a two-file folder that is a parent plus its only private helper is
   structure, while two unrelated files sharing a folder is fragmentation.
   Note the counterweight before touching it: exp074 measured that
   collapsing NOTHING gives 809 folders, and blanket collapse inflates the
   root from 1,013 to 1,590. Both extremes are worse than today.

2. **A grouping signal for genuinely shared files (586 files with 3+
   importers, harder).** These have no structural home by construction —
   many files use them and the importers do not agree. Every signal we
   have is topological, and topology has nothing left to say here. The
   natural evidence is SEMANTIC: what the file is for. That makes this a
   natural extension of the LLM file-naming work (exp076 follow-on) —
   the same model that names a file from its declared symbols could
   cluster files by what they do. Recorded as the obvious pairing, not as
   a decided design.

## Scoring the tree: what the metrics say, and the trap in them

Andrew asked for a "spread-out-ness" score and a benchmark against real
codebases. Both exist now
(`experiments/076-statement-placement/tree-shape.ts`,
`repo-modularity.ts`). Two families, and they disagree:

- **SHAPE** — Shannon entropy of the folder-size distribution normalised to
  Pielou evenness (0..1), Gini of folder sizes, depth. Descriptive.
- **QUALITY** — Newman-Girvan modularity `Q` of the folder partition scored
  against the tree's own import graph: intra-folder edges minus what the
  same degree sequence would give at random. `Q ≈ 0` means the folders say
  nothing about the dependencies.

**Shape metrics are a trap here, and we have the proof in hand.** Measured
on the SAME version, 2.1.86, in both layouts:

| tree                           | files | flat root |  evenness |         Q |
| ------------------------------ | ----: | --------: | --------: | --------: |
| ours, fossil layout            | 3,274 |     32.8% |     0.729 |     0.142 |
| ours, PRE-fossil layout        | 1,533 |      0.0% | **0.978** |     0.118 |
| humanify `src/` (hand-written) |   256 |      8.2% |     0.796 | **0.340** |
| preact (hand-written)          |    42 |      0.0% |     0.947 | **0.425** |
| `@babel/core` (hand-written)   |    55 |     10.9% |     0.867 | **0.245** |

The pre-fossil layout wins every shape column — perfectly even folders, no
flat root, biggest folder 26 files — and it is the layout we ABANDONED for
being 2.2× coarser than the truth. Optimising evenness would walk us
straight back to it.

**The honest reading of Q, including what it cannot settle:**

- Our layouts score **0.06–0.14**; hand-organised repos score **0.25–0.43**.
  That gap is large, consistent across three independent repos, and is the
  real finding: our folders are markedly worse than a human's at reflecting
  how the code depends on itself.
- **Fossil vs pre-fossil on Q (0.142 vs 0.118) is NOT resolvable.** The same
  pre-fossil scheme scores 0.118 on 2.1.86 and **0.064** on 2.1.191 — a
  version-to-version spread of 0.054, twice the 0.024 gap between the two
  layouts. Rule 11: the instrument cannot resolve an effect smaller than its
  own variation, and it will print a sign anyway. Do not cite fossil as a Q
  win.
- Q is bounded by graph structure and has a known resolution limit. A
  dependency graph with one utility imported by thousands of files caps Q
  under ANY partition. Compare Q between layouts of the same program; treat
  the human-repo column as a target to aim at, not a like-for-like score.

So the benchmark to beat is **Q ≈ 0.25–0.43** while keeping the fossil
layout's fidelity, and no shape metric may be optimised on its own.

## Cautions carried forward

- **Stability outranks tidiness.** Anything here must respect exp076's
  rule: an INHERITED path is never re-homed. Re-foldering a carried file
  churns every line in it, which is the cost this whole arc exists to
  remove. That means these levers only bite on a FIRST run, and the folder
  shape then freezes (exp074's carry-forward finding) — so a relayout needs
  either a deliberate one-time migration or scoring without a prior ledger.
- **A tidier tree is not automatically a better diff.** exp074 measured the
  fossil layout as structurally better and NOT yet a win on hidden churn.
  Judge this on the diff, not on folder counts, and read
  `docs/measurement-pitfalls.md` rule 11 before believing a small delta.
- One-time migration churn is a NON-COST here (Andrew, 2026-08-13): judge
  steady state only.
