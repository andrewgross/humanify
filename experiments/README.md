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

## Fixture

All experiments use Preact 10.24.3 (core + hooks), scope-hoisted via Rollup.
- 143 total functions, 95 top-level
- LLM-renamed output in `001-baseline-clustering/fixtures/preact-v1/output/`
- Original source structure in `001-baseline-clustering/fixtures/preact-v1/original-structure.txt`
