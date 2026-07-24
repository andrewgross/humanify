# 038 — Dependency-aware emit order (the largest remaining diff-noise lever)

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
research-log entries read _Idea → Evidence (table) → Conclusion_; outcomes are
**landed** or **failed** with numbers; totals-first tables; **ceilings measured
before builds**; every hop judged **on its own** (no cross-pair aggregates — a
big hop masks a regression on a small one, which is exactly how exp037 v1's
118→119 regression hid).

Read [exp037 FINDINGS.md](../037-noise-source-decomposition/FINDINGS.md) first.

## Why this experiment exists

exp037 measured what the on-disk diff is actually made of, per hop, in git-line
units (`diff-composition.ts`), on the current trees (Lever B v2 shipped):

| hop     |  churn |  REAL |     NOISE | naming | alias | **reorder** |
| ------- | -----: | ----: | --------: | -----: | ----: | ----------: |
| 85→86   | 59,520 | 57.0% | **42.9%** |   9.7% |  0.0% |   **33.2%** |
| 215→216 | 40,818 | 70.7% | **29.3%** |   2.3% |  0.4% |   **26.6%** |
| 197→198 | 69,574 | 83.6% | **16.3%** |   1.7% |  0.4% |   **14.3%** |
| 118→119 | 39,507 | 96.0% |  **4.0%** |   1.2% |  0.2% |        2.7% |

**Reorder is the largest noise bucket on every hop**, after Lever B already took
the easy half. Naming churn — the thing years of experiments targeted — is
1.2–9.7% of diff lines. (`noiseLn` overstates naming because it charges whole
statement mass: one drifted identifier in a 998-line statement bills 998 lines.)

## What Lever B v2 already does, and why it stops

`alignFileStatements` (`src/split/stable-split.ts`, replayed into the runnable
tree by `orderedIndexesByFile` in `cjs-emit.ts`) orders each file's statements to
the prior file's emission order, matched by `statementHash`. Two gates:

1. **Safety** — only `FunctionDeclaration`s may move. They are hoisted and
   initialized before any statement runs, so textual position has zero runtime
   effect. Every load-order data dependency is between non-function statements,
   so keeping non-functions in bundle order preserves all of them by
   construction. (The naive all-statements version **crashed the runnable tree**:
   `defineModuleExports(m, {...})` ran before `var m = {}` —
   `TypeError: Properties can only be defined on Objects`. The boot gate caught
   it. Do not remove this gate without replacing it with a real dependency model.)
2. **Precision** — a statement may claim a prior position only when its hash is
   unambiguous (exactly one occurrence per side). Pairing same-hash siblings
   (`noop` stubs) is a guess that manufactures churn; this was a measured +2.3%
   regression on 118→119 before the gate.

Everything non-function stays pinned. That pinned set is the residual reorder
churn in the table above.

## The idea

Replace gate (1)'s blanket "functions only" with an actual **load-time dependency
model**, and allow any permutation that preserves it.

For each top-level statement of an emitted file, compute its load-time behaviour:

- **writes** — module bindings it assigns while the module loads.
- **reads** — module bindings it reads while the module loads (initializer
  expressions and bare top-level expressions; NOT bodies of functions/arrows,
  which run later).
- **effects** — whether it performs an observable side effect at load time
  (a call whose callee is not provably pure, a write to a foreign object, etc).

Then the legal permutations are those preserving: read-after-write edges,
write-after-write edges on the same binding, and the relative order of
effect-bearing statements. Order the file to match the prior within that
constraint (topological order seeded by prior rank).

### Verified: the bundler's lazy-init wrapper is pure

The biggest pinned bucket is the `lazyInitializer` registration blocks. In the
real bundle:

    var lazyInitializer = (generator, cachedValue) => () => (generator && (cachedValue = generator(generator = 0)), cachedValue);

Calling it **captures a closure and returns** — no observable load-time effect;
the body runs only when the returned thunk is invoked. So
`var x = (0, resourceLifecycle.lazyInitializer)(() => {...})` is
effect-free at load time and freely movable subject only to its own read/write
edges. The subagent decomposition of `api-query.js` found its MOVED churn (34.9%
of that file) is exactly these blocks. This is the unlock — but **verify the
wrapper's shape per bundle rather than trusting the name** (the pipeline already
does bundler detection; reuse it, do not pattern-match an identifier).

## The work, in order

### A. Ceiling first — do NOT build before this

Extend `experiments/037-noise-source-decomposition/reorder-safety.ts` (its
existing HOISTED / LAZY_DECL / ANCHOR split is a first cut, and its LAZY_DECL
heuristic is name-based — replace that) to classify the **residual** reorder
churn of the current v2 trees by:

- already-movable (function decl) — should be ~0, v2 took these;
- effect-free declaration (literal / function expr / arrow / class expr / bare
  `var x;`);
- pure-wrapper call (`lazyInitializer`-shaped, verified structurally);
- genuinely order-bound (reads a load-time-written binding, or effect-bearing).

Report per hop, in git lines (×2 for delete+add, matching `diff-composition.ts`).
**That table is the ceiling.** If the order-bound share dominates, stop and say so.

### B. The dependency model

Build it as a pure, unit-tested function over a file's statements — no emit
changes yet. Property tests: a statement that reads what another writes never
precedes it; effect-bearing statements never reorder relative to each other; a
file of pure declarations is fully permutable. Reuse the existing scope/binding
analysis (`src/analysis/`), do not hand-roll identifier resolution.

### C. Wire it into `alignFileStatements` behind a toggle

Same shape as Lever B: replace the `isMovable` boolean with the dependency
constraint, keep the unambiguous-hash precision gate unchanged, keep
`HUMANIFY_NO_EMIT_ALIGN=1` working. TDD — red test first.

### D. Adjacent cheap win: require-alias drift (exp037 Finding 4)

Independent of A–C, ~1 hour, pure noise removal. `nsNameIsFree` (`cjs-emit.ts`)
rejects an alias candidate if the identifier appears **anywhere in the bundle**,
including a nested local in a file that does not even import the module. One LLM
local-variable draw (`let fileModTime = ...`) cost **312 lines across 67 files**
on 215→216. Two fixes, ideally both: scope the freeness check to the file where
the alias is declared (precision), and record aliases in the split ledger so a
still-legal prior alias is kept (stability). Guard: aliases are also globally
`claimed` so one path gets one alias tree-wide — that is a deliberate readability
choice; keep it.

## Guardrails (each one was learned the hard way)

- **Boot gate is mandatory** — `cd <out> && bun run.cjs --version` must echo the
  version. The naive reorder passed every unit test and crashed on boot.
- **Pure-reorder check** — per file, sorted lines of ON and OFF must be identical.
  A non-zero mismatch means you changed content, not order.
- **Per hop, never aggregated.** Run 85→86 (shuffle, biggest win available) AND
  118→119 (feature drop, ~0 available — the regression canary). A regression on
  any hop kills it.
- **Same-naming A/B** — ON and OFF must produce byte-identical
  `.humanify/humanified.js`. If they do not, naming drifted and the comparison is
  invalid. (It held on all four hops in exp037; the shared `--llm-cache` pins it.)
- Self-hop must not regress (85→86's 44 lines is a documented pre-existing draw
  flake, ON and OFF alike; 216 is 0).
- `npm run check` + `npx biome check <file>` before committing. Never edit `src/`
  while a pipeline run is in flight (`pgrep -f 'src/index.ts'`).

## How to run everything (copy-paste)

    cd /Users/andrewgross/Development/humanify   # branch exp037-noise-decomposition

    # one hop, align ON (reuse the sweep's rebased prior + warm cache; ~12 min)
    IN=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js
    PRIOR=/tmp/eval-work/leverb-sweep/2.1.216-rebased/.humanify/humanified.js
    NODE_OPTIONS="--max-old-space-size=14336" npx tsx src/index.ts "$IN" --split \
      --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b --api-key local \
      --reasoning-effort low -c 32 -o /tmp/eval-work/leverb/216-on-v3 \
      --llm-cache /tmp/eval-work/llm-cache --prior-version "$PRIOR" \
      -vv --log-file /tmp/eval-work/leverb/216-on-v3.log > /tmp/eval-work/leverb/216-on-v3.stdout 2>&1

    # measure vs the existing OFF baseline (boot + pure-reorder + churn)
    bash experiments/037-noise-source-decomposition/leverb-measure.sh \
      /tmp/eval-work/leverb/216-on-v3 /tmp/eval-work/leverb-sweep/2.1.216-off \
      /tmp/eval-work/leverb-sweep/2.1.216-rebased/src "215->216(v3)"

    # composition of the resulting diff (real vs naming vs alias vs reorder)
    NODE_OPTIONS="--max-old-space-size=12288" npx tsx \
      experiments/037-noise-source-decomposition/diff-composition.ts \
      /tmp/eval-work/leverb-sweep/2.1.216-rebased/src /tmp/eval-work/leverb/216-on-v3/src "215->216(v3)"

    # full 4-hop sweep (long; rebase+ON+OFF+self-hop per hop)
    experiments/037-noise-source-decomposition/leverb-sweep.sh

Existing OFF baselines and rebased priors live under `/tmp/eval-work/leverb-sweep/`
(`<version>-rebased`, `-off`, `-on`); if `/tmp` was cleared, re-run the sweep.

## Success criterion

Residual reorder churn down on the reshuffle-heavy hops (85→86, 215→216, 197→198)
with **no regression on 118→119**, every tree booting, every ON/OFF pair a proven
pure reorder, and naming untouched (byte-identical bundles) — or a measured,
written-up ceiling showing the residual is genuinely order-bound.
