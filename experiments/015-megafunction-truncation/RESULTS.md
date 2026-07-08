# Exp015 results — megafunction truncation coverage (2026-07-08)

Branch `exp015-megafunction-coverage`. Two A/B rounds: run 1 validated
the truncation/context fixes and **falsified the brief's causal model
for the headline noise families**; the autopsy found the real mechanism
(same-named sibling-block bindings), and run 2 confirmed its fix:
**reroll bucket −51%, every headline family gone, noise hunks
6,206 → 5,788, both legs cheaper than baseline.**

## Changes under test

Run 1 (commits through `979941d`):

1. **Declaration-anchored code windows** (`src/rename/code-window.ts`):
   oversized function code (>500 generated lines) is no longer
   flat-truncated — each batch identifier gets a window around its
   declaration line (input locs map 1:1 onto generated lines, verified
   for all 137 oversized functions; runtime-guarded fallback to flat
   truncation). Header + closing line always shown; windows merge with
   elision markers; padding shrinks to the 500-line budget. Retry
   snippets extract from the windowed text.
2. **Whole-identifier regex** (`identifierRegex`): `extractRetrySnippet`
   used `\b<id>\b`, which cannot match `$`-names and false-matches
   inside longer ones — `$`-family retries pulled wrong/empty snippets.
3. **Context caps**: close-match prior code capped at 500 lines (was:
   whole prior function, up to 3,512 lines); module-binding declarator
   text capped at 10 lines. Round 3 had 54+63 batches 400-failing at
   the model's 32K context — concentrated exactly in the megafunction
   noise-family functions, silently un-naming whole batches.
4. **Diag fix**: shadowed-pass reports merge instead of overwriting —
   round-3 diags silently dropped main-pass outcomes for any function
   with a shadowed second pass (the brief's "477 in unrenamed.missing"
   was this artifact; true missing was 62).

Run 2 adds (`bb2efdc`, `e390d56`):

5. **Uniquify same-named sibling bindings** — the actual fix for the
   headline families; see "The real mechanism" below.
6. Module used-names cap (200) — hardening; the surviving 45K-token
   batch turned out to be a different bomb, fixed after run 2:
7. **(post-run-2, `4e57dbf`) char-cap module declaration text**: the
   one batch that 400-failed in every run embeds
   `var MF5 = "<205KB single-line base64 blob>"` — a line cap passes it
   whole. Char cap (1,000) drops the reconstructed prompt 208KB → 4.3KB
   (verified offline via `inspect-module-prompt.ts`); affects ~10
   module bindings, not re-A/B'd.

## Headline (run 2 vs round-3 baseline)

| metric               | round 3 | run 1  | run 2      | vs baseline |
| -------------------- | ------- | ------ | ---------- | ----------- |
| noise hunks          | 6,206   | 6,298  | **5,788**  | −7%         |
| noise share of hunks | 76.2%   | 76.6%  | **75.0%**  | −1.2pp      |
| genuine-change hunks | 1,936   | 1,922  | 1,934      | stable ✓    |
| rename occurrences   | 11,596  | 11,891 | **10,206** | −12%        |
| distinct bindings    | 3,479   | 3,553  | 3,366      | −3%         |

Buckets (occurrences):

| bucket          | round 3       | run 2             | delta    |
| --------------- | ------------- | ----------------- | -------- |
| transfer-gap    | 7,283 (62.8%) | 7,536 (73.8%)     | +3%      |
| asymmetric      | 1,414 (12.2%) | 1,259 (12.3%)     | −11%     |
| minifier-reroll | 2,899 (25.0%) | **1,411 (13.8%)** | **−51%** |

Success criteria from the brief:

- ✅ **Every truncation family left the top contributors**: `v6→X6`
  (114), `$_→w_` (111), `vH→T_` (104), `J_→W_` (96), `g→F` (62),
  `s→o` (57), `C→E` (56) — all gone. The remaining top rerolls are the
  wrapper-scope class families (`y6→C6` 98, `HK→qK` 57 — experiment
  #13's territory) and small residuals.
- ✅ Reroll dropped past the ~2,300 target to 1,411.
- ✅ Reroll inside oversized functions: **1,210 → 125 (−90%)**.
- ✅ Genuine change 1,934 (must stay 1,900–2,000).
- ✅ Parse clean both legs, structural invariant clean, no
  context-length failures in the incremental leg (round 3: 63), diag
  `unrenamed.missing` 64 (fresh) / 3 (incremental) with an
  honest-counting diag.
- ✅ Cost: fresh 46.2M tokens / 12m50s (round 3: 49.4M / 13m18s),
  incremental 4.6M / 7m19s (round 3: 5.9M / 8m27s) — **both legs
  cheaper and faster** while LLM-naming +24K more identifiers
  (127,749 vs 103,767).

## Run 1 — windows + caps alone did NOT move the families

Run 1 (windows, `$`-regex, context caps, diag fix — no uniquify):
noise 6,298 hunks; buckets 7,632 / 1,343 / 2,916; the truncation
families at IDENTICAL counts (`v6→X6` 114, `$_→w_` 111...). Coverage
and cost wins were real (0 incremental context failures, −6% fresh
tokens, +20K identifiers named), but the brief's "truncation → noise
families" attribution was **wrong** — visibility was necessary, not
sufficient.

## The real mechanism: same-named sibling-block bindings

The families' bindings were RENAMED per the diag (`$_ →
messageSessionIdentifierVal` in fn `516027:5`) yet survived in the
output. The diff hunks resolve the paradox: `let $_ = …` appears at
lines 516097, 516140, 516167 and `catch ($_)` at 516159 — **multiple
distinct bindings sharing one minified name across sibling block
scopes**. Bun reuses tiny names aggressively inside big functions.

The batch protocol keys identifiers by NAME, so collection dedups:
`collectOwnedBindingInfos` takes the first `$_`, the shadowed second
pass recovers at most one more (its per-name `Map` collapses the
rest). N−2 of every group stayed minified in BOTH legs → Bun rerolls
the token between builds → the reroll families. They concentrate in
megafunctions (more sibling blocks), which made truncation look
causal.

Sizing (`count-duplicate-names.ts`, v119): **7,017 duplicate bindings
across 2,082 functions; 3,018 beyond the reach of both passes.** The
worst offenders are exactly the families: `521806:2122` `v6`×36 (the
114-occ family), `516027:5` `$_`×34 (111 occ), `264855:2` `s`×12,
`294647:12` `m`×15.

Fix: the shadowed pass mechanically uniquifies duplicates first — the
k-th binding of a name group becomes `<name>_<k>` via
`attemptValidatedRename` (AST order → version-stable suffixes,
recorded as decisions) — then every binding is individually
LLM-nameable with its own declaration-anchored window. If the LLM
misses one, both legs still agree on the mechanical name instead of
rerolled minified tokens. Zero uniquify rejections at bundle scale.

## What moved, what didn't, what's next

**Moved:**

- Minifier-reroll −51% (2,899 → 1,411 occ); −90% inside oversized
  functions. The megafunction reroll problem is essentially closed.
- Asymmetric −11%. Noise hunks −7%, occurrences −12%, both legs
  cheaper/faster, coverage +24K identifiers, incremental context
  failures 63 → 0.

**Didn't move (and why):**

- Transfer-gap +3% (7,283 → 7,536): newly-named duplicate bindings get
  DIFFERENT names across legs on first contact (e.g.
  `toolExecutionResult → o_10` — v120's LLM missed one and its
  mechanical name survived). This is expected on the first run after a
  coverage change: named-in-both-legs bindings become transfer
  candidates for every later incremental run, which is what actually
  kills the noise (the brief's own coverage-over-determinism logic).
- `serializeWithHelper → serializeData` (423 occ): still the single
  biggest binding — close-match LLM naming instability, fifth
  different name in five runs. Separate workstream.
- Wrapper-scope class families (`y6→C6` 98, `HK→qK` 57): out of scope
  (experiment #13).

**Next:**

1. **Name-keyed transfer collapse** — the same dedup bug pattern lives
   in the transfer path: close-match `nameTransfers` is keyed by old
   name (`prior-version.ts` positional pairs, `buildOwnedBindingMap`
   "first name wins"), so only ONE of N same-named siblings can
   receive its prior name; the rest go fresh-LLM every incremental run
   (a slice of today's transfer-gap). Slot/position-keyed transfer for
   block locals would let the uniquified population converge run over
   run.
2. Re-run the A/B once more after (1) + the `MF5` char-cap to measure
   convergence — transfer-gap should absorb most of what reroll gave
   up.
3. Close-match naming instability (`serializeWithHelper`, 423 occ) and
   wrapper-scope classes (~1,100 occ) remain the two big structured
   noise sources after that.

## Reproduce

```bash
# offline diagnostics (no LLM)
npx tsx experiments/015-megafunction-truncation/truncation-coverage.ts <bundle|prepared> [--prepared] [--json out]
npx tsx experiments/015-megafunction-truncation/simulate-windows.ts <prepared>
npx tsx experiments/015-megafunction-truncation/count-duplicate-names.ts <prepared>
npx tsx experiments/015-megafunction-truncation/inspect-module-prompt.ts <prepared> <id...>
# A/B (needs the LLM box)
git worktree add --detach /tmp/humanify-run-015 <commit>
ln -s "$PWD/node_modules" /tmp/humanify-run-015/node_modules
PHASE2_OUT=/tmp/exp015-rN bash /tmp/humanify-run-015/experiments/013-bun-cjs-classification/run-phase2.sh
python3 experiments/013-bun-cjs-classification/classify-diff.py /tmp/exp015-rN/runtime-diff.txt
python3 experiments/014-rename-noise-elimination/attribute-noise.py /tmp/exp015-rN/runtime-diff.txt 20
python3 experiments/015-megafunction-truncation/attribute-to-megafunctions.py /tmp/exp015-rN/runtime-diff.txt <truncation.json>
```

Artifacts: `/tmp/exp015/` (run 1), `/tmp/exp015-r2/` (run 2), round-3
baseline at `/tmp/exp014-round3/`.
