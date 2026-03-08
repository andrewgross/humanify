# File Splitting Experiments

Each numbered experiment follows a cycle:
1. **README.md** — hypothesis, background (citing prior experiments), variables, steps
2. Run the experiment
3. **RESULTS.md** — metrics, observations, what we learned, next steps

## Experiments

| # | Name | Status | Key Finding |
|---|------|--------|-------------|
| 001 | [Baseline BFS Clustering](./001-baseline-clustering/) | Complete | Pure reachability over-fragments; 36.8% shared, MQ 0.205. Source proximity is the strongest unused signal. |
| 002 | [Merge Small Clusters](./002-merge-small-clusters/) | Complete | Shared→0%, but 23/95 functions are completely isolated (no call edges). Need proximity fallback. |
| 003 | [Source Proximity Fallback](./003-proximity-fallback/) | Complete | All targets met: 3 clusters (core/DOM/hooks), 0% shared, MQ 1.0. Algorithm is production-ready. |
| 004 | [File Emission](./004-file-emission/) | In Progress | Emit actual split files with imports/exports for qualitative review. |

## Experiment Workflow

```
1. README.md — Hypothesis, background from prior experiments, variables
2. Run: npx tsx src/index.ts split <input> --min-cluster-size N --proximity --output experiments/NNN/output/
3. Review: Read the emitted files, assess grouping quality, code navigability
4. RESULTS.md — Metrics table, qualitative assessment, next steps
```

### Quality Checklist for File Review

- Do functions that work together end up in the same file?
- Are the file sizes roughly balanced, or is one file a dumping ground?
- Does the code in each file have a coherent theme/purpose?
- Are there circular imports? (file A imports from B which imports from A)
- Is shared.js minimal, or does it contain things that belong in a specific file?
- Would a developer new to this codebase understand the organization?

## Fixture

All experiments use Preact 10.24.3 (core + hooks), scope-hoisted via Rollup.
- 143 total functions, 95 top-level
- LLM-renamed output in `001-baseline-clustering/fixtures/preact-v1/output/`
- Original source structure in `001-baseline-clustering/fixtures/preact-v1/original-structure.txt`
