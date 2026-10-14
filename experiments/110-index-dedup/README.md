# exp110: index-time origin de-duplication (inlined copies go to their origin)

STATUS (2026-10-14): DONE, branch `exp110-index-dedup`, NOT merged and NOT recommended as a default. Gate: ALL 13
STAGES PASSED (bare) at `ed0c7040` and after merging main `a7de41ed`. Only `crates/humanify-index` (the builder) changed, so the pipeline binary is
untouched and no neutrality run applies. All labels are under scorer 6 (main `a7de41ed` merged in). The numbers
come from a LAB post-pass (`scripts/postpass.py` + `reencode/`), not from a `--merge-only` re-merge. The re-merge
is queued under `/work/index-build.lock`, which exp109's `c2-all` build has held all day. When it runs, it writes
`/work/exp110-data/ix/c2-l12m4-dedup` and a byte comparison against the post-pass in
`/work/exp110-data/logs/merge-vs-postpass.txt`.

**Verdict, totals first.** The rule does what it says: claims move from the inliner to the origin, and the inliner
classes named in exp108 flip to right. But F0.5 does not move on the sets the campaign judges:

- test-bun is flat on both indexes;
- test-esbuild is +0.7 on c2 and -0.3 on k2;
- held-out is -0.4 (c2) and -0.5 (k2).

Two things cause this:

1. **Truth gaps.** Where a shipper inlines an unmarked copy, the truth credits the shipper. Naming the real origin
   then counts as wrong. Examples: openpgp's `dist/node/openpgp.mjs` holding `@noble/*`, ora's `index.js` holding
   yoctocolors, tar 7's `index.min.js`.
2. **Lost family claims.** Some claims were only right by family luck, and they lose their evidence. The largest
   is mermaid's claim on a `@mermaid-js/parser` chunk (truth shipper `@mermaid-js/parser`, origin
   vscode-languageserver-types): 131 KB, now unclaimed.

The ~9 MB of wrong-family bytes on each test set is NOT this class: dedup moves it by less than 1%.

## What was built (`crates/humanify-index/src/origin.rs`, applied in `merge::Merger::finish`)

The merge drops the unit row of owner B for a print that owner A also holds when all of these hold:

- B DEPENDS on A. The evidence comes from the build store only:
  - B's shards hold rows OWNED by A (its builds bundled A from `node_modules`), or
  - B's published package.json names A in any dependency field, devDependencies included. This is read once per
    version from the install and recorded in `store/declared/`.
- A does not depend on B.
- A and B are not one family (shipped families plus derived links).
- A's smallest module holding the print is within the lookup's specificity bound of B's (4× + 4 KB). Without this
  check, micromark devDepends on prettier, whose markdown plugin inlines micromark, and so would have made prettier
  the "origin" of micromark (v1 vs v2 below).
- The print has at most 3 owners. Generic code the lookup refuses as ambiguous must not become claimable.

A cycle (every owner depends on another) keeps every row. `dedup.json` in the index records the rows dropped per
slice and every (inliner, origin) pair. Only the unit tables change; the index format stays `/3`. Tests:
`origin_test.rs` (9 tests), written red first.

Rows dropped: c2 136,716 / 133,655 / 165,425 (bun-bun / bun-node / esbuild), about 6.5% of unit rows. k2 232,224 /
225,050 / 283,345. Largest pairs: mermaid → elkjs, cytoscape, langium, chevrotain, katex; @ts-morph/common,
prettier, monaco-editor → typescript; @mastra/core → ai; react-grab → bippy; echarts → zrender.

## Scores (scorer 6, structural + exp106's recommended keys via `--pipeline-set`, binary main `6182666d`; apps `a7de41ed`)

P / R / **F0.5** / F1. "pp v1" is the rule without the size check; "pp2" is the shipped rule.

| index | set               | base                              | pp v1                     | **pp2 (shipped rule)**            |
| ----- | ----------------- | --------------------------------- | ------------------------- | --------------------------------- |
| c2    | held-out 67       | 99.3 / 54.1 / **85.0** / 70.0     | 99.2 / 53.3 / 84.6 / 69.3 | 99.3 / 53.4 / **84.7** / 69.4     |
| c2    | task-master-ai    | 99.2 / 52.3 / **84.1** / 68.5     | =                         | =                                 |
| c2    | wrangler, CC      | =                                 | =                         | =                                 |
| c2    | apps-esbuild (39) | 99.05 / 85.69 / **96.05** / 91.89 | —                         | 99.08 / 85.04 / **95.91** / 91.52 |
| c2    | TEST bun          | 76.1 / 44.3 / **66.6** / 56.0     | 76.0 / 44.3 / 66.5 / 56.0 | 76.1 / 44.4 / **66.5** / 56.0     |
| c2    | TEST esbuild      | 77.6 / 46.5 / **68.4** / 58.2     | 79.2 / 45.8 / 69.1 / 58.0 | 79.2 / 45.8 / **69.1** / 58.0     |
| k2    | held-out 67       | 99.1 / 66.3 / **90.2** / 79.5     | 98.8 / 65.5 / 89.7 / 78.8 | 98.8 / 65.5 / **89.7** / 78.8     |
| k2    | task-master-ai    | 95.7 / 65.4 / **87.6** / 77.7     | 96.7 / 66.2 / 88.5 / 78.6 | 96.7 / 66.2 / **88.5** / 78.6     |
| k2    | wrangler, CC      | =                                 | =                         | =                                 |
| k2    | apps-esbuild (39) | 98.53 / 89.70 / **96.63** / 93.91 | —                         | 98.59 / 89.17 / **96.55** / 93.65 |
| k2    | TEST bun          | 80.6 / 56.0 / **74.1** / 66.1     | 80.6 / 55.9 / 74.1 / 66.1 | 80.6 / 56.0 / **74.1** / 66.1     |
| k2    | TEST esbuild      | 83.6 / 66.2 / **79.4** / 73.9     | 83.3 / 65.9 / 79.1 / 73.6 | 83.3 / 65.9 / **79.1** / 73.6     |

App claimed is 0 on every label. Test wrong-family bytes: c2 bun 9,104 → 9,117 KB, esbuild 9,288 → 8,286 KB; k2 bun
8,809 → 8,771 KB, esbuild 8,944 → 9,113 KB. The `--corpus bundlers` corpus was not run: c2 and k2 have no webpack or
rollup slice. Labels: `exp110-{c2,k2}-{base,postpass,pp2}-rec`, `test-…`, and `apps-exp110-{c2,k2}-{base,pp2}-rec`.

## Changed claims, read by hand (dev, k2 base → pp2; `scripts/changed_claims.py`)

Held-out verdict transitions (bytes): family → origin 234,664; right → origin 94,573; family → none 182,832;
right → wrongFamily 64,138; wrongFamily → right/origin/none 16,041.

1. mdp: mermaid (family) → none, 131 KB. The unit is vscode-languageserver-types' `__commonJS` wrapper inside
   `@mermaid-js/parser`'s chunk. mermaid's rows went, and the origin does not take the whole unit. Lost recall.
2. mdp: mermaid → chevrotain (origin), 19 KB. Right.
3. mdp: mermaid → langium (origin), 13 KB, and a dozen more langium/chevrotain classes. Right.
4. renovate: openpgp (right) → @noble/curves (wrongFamily), 6.9 KB of weierstrass/edwards code. The code IS noble's;
   this is a truth gap (openpgp's unmarked pre-bundled copy).
5. renovate: openpgp → @noble/hashes (SHA-512, SHA-1, RIPEMD classes). Truth gap, as in 4.
6. renovate: openpgp → @noble/ciphers. Truth gap, as in 4.
7. renovate: tar (right) → none, 21 KB. tar 7's `index.min.js` inlines minipass and friends, so tar's rows went.
   Truth gap, plus lost recall.
8. nestjs: ora (right) → yoctocolors (wrongFamily). The code is yoctocolors' `format(0,0)` table, which ora's
   `index.js` holds inline. Truth gap.
9. dotenvx: ora (wrongFamily) → yoctocolors (right). Fixed.
10. task-master (held-out): @mastra/core (wrongFamily) → ai (right). Fixed (exp108's class 11).
11. task-master (esbuild): npm-check-updates (wrongFamily) → zod (right), 3.4 KB. Fixed (exp108's class 4).
12. lighthouse: @livekit/components-react (wrongFamily) → none. The wrong rxjs claim is removed (exp108's class 6).
13. @anthropic-ai/claude-agent-sdk (right) → zod (origin). Still right.
14. strtok3 (right) → peek-readable (wrongFamily). strtok3 absorbed peek-readable's source. Arguably a truth gap.
15. mermaid (family) → @anthropic-ai/claude-agent-sdk (wrongFamily), 21 claims. Once mermaid's row went, the
    remaining owner of a shared print won. Genuinely new wrong.

## What would make it pay

Judge it on a truth that credits the origin of an unmarked copy. exp104's `copies` rule demands that the shipped
file NAME the origin, which a minified dist never does. The same declared/installed evidence used here would be
the generic test, but it would be a scorer change and a re-freeze. Under the current truth, the rule trades
truth-gap "right" bytes for real origins, at net zero on test.

## Reproduce

```bash
D=/work/exp110-data
humanify-index build <exp101's c2 flags> --offline --merge-only --out $D/ix/c2-l12m4-dedup   # under /work/index-build.lock: $D/merge-c2.sh
# lab post-pass (the same rule on an existing /3 index):
<exp095 venv python> -I scripts/postpass.py <index> /work/exp101-data/store <work[:work]> <out> <exp110-reencode>
$D/score.sh <label> <index> dev|test|apps
<exp095 venv python> -I scripts/changed_claims.py <base label> <candidate label> [N]     # dev only
```
