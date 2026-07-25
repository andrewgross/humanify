# exp037 Lever B — resume handoff

Branch `exp037-noise-decomposition`. Read `FINDINGS.md` first (full decomposition

- validated results table).

## State

- **Committed `1b46fdb`**: Lever B v1 = stable within-file emit order. Functions
  (hoisted, safe) reorder to the prior file's emission order; non-functions stay
  in bundle order. In `stable-split.ts` (`alignEmissionOrder`/`alignFileStatements`)

  - `cjs-emit.ts` (`orderedIndexesByFile` replays it into the RUNNABLE tree via
    `ledger.hashes`). `HUMANIFY_NO_EMIT_ALIGN=1` toggles off. `npm run check` green.

- **v1 result (full 4-pair sweep, on-disk git churn OFF→ON):**
  215→216 −29%, 85→86 −24%, 197→198 −13%, 118→119 **+2.3% (regression)**.
  Aggregate −18.4%. All boot, all pure reorders (0 content-mismatch).
  Cause of the 118→119 regression: same-structural-hash stub functions
  (`noop`/tiny getters, different names) get FIFO-mispaired by hash → text churn
  where bundle order had none. Sweep artifacts under `/tmp/eval-work/leverb-sweep/`,
  table `grep TABLEROW /tmp/eval-work/leverb-sweep.table`.

## NEXT: build v2 (unique-hash precision guard) — kills the regression

Only reposition a function if its structural hash is UNIQUE in the file on both
sides. Edit `alignFileStatements` in `src/split/stable-split.ts`: after the
`priorSeq` empty-guard, replace the movable checks with a `movable()` predicate:

```ts
const freshCount = new Map<string, number>();
for (const s of slots)
  freshCount.set(hashes[s], (freshCount.get(hashes[s]) ?? 0) + 1);
const priorCount = new Map<string, number>();
for (const h of priorSeq) priorCount.set(h, (priorCount.get(h) ?? 0) + 1);
const movable = (s: number): boolean =>
  isMovable[s] &&
  freshCount.get(hashes[s]) === 1 &&
  priorCount.get(hashes[s]) === 1;
if (slots.filter(movable).length < 2) return [...slots];
// ...then use `!movable(s)` where it currently uses `!isMovable[s]`.
```

Add a unit test: two same-hash function stubs stay in bundle order while unique
neighbors align. Then `npm run typecheck` + `npx tsx --test src/split/stable-split.test.ts`.

## Validate v2 (targeted, ~30-60 min, needs the LLM endpoint 192.168.1.234:8000 up)

Do NOT edit src/ while any pipeline run is in flight (`pgrep -f leverb-sweep.sh`,
`pgrep -f src/index.ts`). Then, for the regression pair + a win pair:

```
# one pair, align ON then OFF, same prior+cache, then measure:
IN=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.119/binary-decompiled/src/entrypoints/index.js
PRIOR=/tmp/eval-work/leverb-sweep/2.1.119-rebased/.humanify/humanified.js   # reuse the sweep's rebased prior
NODE_OPTIONS=--max-old-space-size=14336 npx tsx src/index.ts "$IN" --split \
  --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b --api-key local \
  --reasoning-effort low -c 32 -o /tmp/eval-work/leverb/119-on2 --llm-cache /tmp/eval-work/llm-cache \
  --prior-version "$PRIOR" -vv --log-file /tmp/eval-work/leverb/119-on2.log > /tmp/eval-work/leverb/119-on2.stdout 2>&1
# repeat with HUMANIFY_NO_EMIT_ALIGN=1 -> 119-off2, then:
bash experiments/037-noise-source-decomposition/leverb-measure.sh /tmp/eval-work/leverb/119-on2 /tmp/eval-work/leverb/119-off2 /tmp/eval-work/leverb-sweep/2.1.119-rebased/src "2.1.118>2.1.119"
```

Success = 118→119 git churn no longer regresses AND 85→86/215→216 wins survive.

## Then

- Full 4-pair sweep for the final v2 table: `experiments/037-noise-source-decomposition/leverb-sweep.sh`.
- User wants a view command per hop: `git diff --no-index <TO>-rebased/src <TO>-on/src`.
- Add a within-file-order (reorder/disk-churn) KPI to `run.sh`/`analyze.ts` — the
  eval's existing KPIs are order-blind and can't see Lever B.
- Complementary axis = Lever A (echo-root name pinning) for the NAMING noise; see
  FINDINGS "Lever A" — needs a --diagnostics trace + a direction gate first.
