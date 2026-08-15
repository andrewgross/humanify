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
