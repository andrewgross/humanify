# 20 — Overnight report, 2026-09-26/27

**Status: COMPLETE.** Merge to main waits for Andrew's sign-off.
The perf/fast branch's own verdict doc is [`20-fast-mode.md`](./20-fast-mode.md) (branch `perf/fast` @ 3478dabd);
this file is the whole-night summary on `rust-port`. Raw data: `/work/overnight-notes.md`, `/work/walk-report-0926/report.md`,
`/work/example-diffs/`, `/work/perf-fast/verdict/summary.txt`.

Everything below ran on `bahadur` (64-core), model `openai/gpt-oss-20b @ :8000` (the measurement default).

## 1. Bugs fixed (all merged to `rust-port`)

| fix                    | branch / commit                     | effect                                                                                                                                                                                                                                                                          |
| ---------------------- | ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| #56 cold-run memory    | `fix/cold-run-memory` → b70adb66    | per-function used-name sets were full private copies of the module scope (~21k fns × ~15k names), all retained; now shared scope snapshots. Cold 2.1.182: **175 GB → 6.5 GB** peak RSS; cold 2.1.72: 63.6 → 4.1; warm 10.6 → 8.4. Output byte-identical before/after on replay. |
| #57 replay determinism | `fix/replay-determinism` → 507163d9 | duplicate identical requests in one wave each got their own LLM answer (cache kept the last); now they share one answer via in-flight dedup. Live→replay 85→86: 0 misses, 0 writes, byte-identical. Small noise win: identical code now gets identical names.                   |
| #57 follow-up          | 8ba489ad → c4784295                 | the same one-key-one-answer dedup in UNCACHED runs (AnswerMemo), not only with `--llm-cache`.                                                                                                                                                                                   |

Earlier overnight cold eval of the fully-fixed binary (`rust-fixed-015d062d`): PASS — exit 0 ×4, boot OK ×4, warm self-hop
byte-identical +0, cold self-hop 6 ln, novel 4,188 / realLn 416,377 exact vs `ts-control-fec64e5-r2` and both rust-5b legs.

## 2. The TS-vs-Rust long walks (29 common hops, 2.1.182→2.1.216)

Both walks cold, same model, same machine, byte-determinism verified per hop. Rust walked from the frozen
`fix-cold-memory` binary b70adb66 (`/work/walk-frozen-b70adb66`); TS from the last TS tree (`fec64e5`).

| total over 29 hops | rust        | ts                             |
| ------------------ | ----------- | ------------------------------ |
| diffLn             | 657,077     | 651,379 (+0.9%)                |
| noiseLines         | 62,119      | 61,968 (+0.2%)                 |
| nameOnly           | 133,870     | 131,456 (+1.8%)                |
| realExBuild        | 449,091     | 438,987 (+2.3%)                |
| novel              | 19,717      | 19,693 (+0.1%)                 |
| **wall**           | **9,208 s** | **27,692 s** — **3.0× faster** |

Per-hop rust wall 106–768 s (ts 683–1,409 s); cold first hop 2,095 s / 6.4 GB; 30/30 hops exit 0, boot OK ×30.

**Rust output is within ~1–2% of TS on every column.** Two hops diverge more than the rest and are flagged, not yet
explained (walk-report §"Diverging hops"): 2.1.207→208 (realExBuild 50,656 vs 41,494 — the largest vendor churn hop)
and 2.1.193→195 (diffLn 25,968 vs 23,558). 2.1.203→204 noise 148 vs 18. **Open follow-up**, not a merge blocker on the
evidence so far: totals match, no column regresses systematically.

## 3. Example diffs (what a cross-version diff looks like from each pipeline)

Generated from the walk trees into `/work/example-diffs/`:

- `src-{rust,ts}-2.1.213..2.1.214.*` — calm hop. UNFILTERED 3,311 / 3,350 ln; **after dropping
  VERSION/BUILD_TIME/GIT_SHA hunks: 88 / 127 ln** — the calm hop is almost entirely build metadata.
- `src-{rust,ts}-2.1.215..2.1.216.*` — busy hop, ~45k ln filtered.
- `src-{rust,ts}-2.1.207..2.1.208.*` — the diverging hop, ~108–110k ln filtered.

What the same real change looks like from each pipeline (`get-default-slug.js`, 2.1.216 multi-file publish feature):
**both pipelines recover identical real code** (same new flag fns, same export, same early-return block). The visible
differences are naming policy only:

| thing                  | rust                                                               | ts                                                   |
| ---------------------- | ------------------------------------------------------------------ | ---------------------------------------------------- |
| new multi-file flag fn | `isTenguCobaltPlinthBrackenEnabled` (derived from the flag string) | `isMultiFilePublishEnabled` (invented semantic name) |
| older flag fn          | unchanged `isTenguCobaltPlinthOsierEnabled`                        | re-rolled `isSharedScopeListingEnabled` each hop     |
| publish fns            | `publishPlanArtifact` / `publishArtifactWithUsage`                 | `publishPlan` / `publishWithAccessCount`             |

Rust's flag-string-derived names are the more stable policy (a new flag's name is determined by its string, not
re-rolled). One TS misname found while reading: 2.1.216 `force: baseVersion, baseVersion: conditionalBaseVersion`
(two params' names swapped); rust got them right. The calm hop's 127-vs-88 gap was ONE identifier: the TS walk renamed
leftover `h` → `totalOutputString` (4 sites); rust kept `h`. Real change identical both pipelines (realExBuild 55 = 55).

## 4. OPEN (parked by Andrew until after the migration): single-letter identifiers are never renamed

~200+ per tree in BOTH walks (rust 135 non-loop `let/const/var` + 78 fn params; ts 138 + 79; top names r(27) t(18)
n(14) o(12) — not loop indices). rust ≈ ts ⇒ a SHARED eligibility skip or blind spot, invisible to the eval because
both legs skip equally (rule 8). Anomaly: the TS walk DID rename `h` in 2.1.214 — single letters are not categorically
ineligible. Full census + next steps in `/work/overnight-notes.md` (2026-09-27 section).

## 5. Memory fix, then the walks (numbers)

Cold no-prior path was only ever gated at fixture scale; on a real 2.1.182 cold hop it peaked 175 GB and OOM'd the
first walk attempt. After #56: cold hop 2,095 s / 6.4 GB; warm hops 12.3–13.6 GB; the perf bench's biggest pair
peaked ~19 GB. Machine headroom restored.

## 6. Fast-mode speed study (`--fast`, `--fast=relaxed`)

Full write-up: [`20-fast-mode.md`](./20-fast-mode.md) (branch perf/fast). Verdict bench on the rebased tip 3478dabd
(includes #56 + #57), `npm run check` 13/13 before benching.

**Cold wall is SERVER-bound, and pipelining loses on this server.** The naming prompt is ~97% prefill
(3.2M prompt vs 0.1M generated tokens on 85→86), so vLLM is throughput-bound and its per-call latency GROWS under
load. The wave barrier's bursty schedule accidentally feeds large prefill batches — pipelined dispatch (exact) feeds
a trickle of small ones and LOSES on every pair (269→369, 354→496, 484→660, 483→570 s). Against a load-independent
stub the schedule itself saves ~16% — a server that holds latency under load would realize that.

**Warm wall: `--fast` −17–22% on all four pairs, byte-identical output.** The binary's CPU is no longer the story;
remaining serial CPU is decision code (matching ~22 s, transfer ~14 s) whose parallelization would change decisions
(eval-gated matching change, not a speed change).

**`--fast=relaxed`** (window-lanes + defer-shadowed; deterministic, completion-order independent): cuts SIMULATED
LLM wall ~40% (407→227 s on 85→86) — worth nothing on this server; would pay on one that holds latency under load.
Warm cost over exact: +2–6 s.

**The lever that pays cold: `--batch-size 25`** (bigger naming windows amortize the per-window context re-send):
−11% cold on 85→86, −9% on 118→119. Decision-changing → judged by the eval (card below).

## 7. Eval cards (binary 7ec5910e, built from 3478dabd)

**perf-relaxed-3478dabd** (`--pipeline-arg --fast=relaxed`): exit 0 ×4, cache +0 ×4 (all prompts live, rule 10),
warm self-hop byte-identical +0 writes. Leaderboard vs `main-2026-09-18`:

| KPI     | reference | relaxed | verdict      |
| ------- | --------- | ------- | ------------ |
| novel   | 4,188     | 4,188   | byte-equal   |
| realLn  | 416,377   | 416,377 | byte-equal   |
| noise   | 2,732     | 2,729   | −3, in band  |
| noiseLn | 50,015    | 49,572  | ~0 (±514)    |
| reloc   | 1,467     | 1,478   | +11, in band |
| mints   | 81        | 86      | ~0 (±11)     |

**The relaxed tier is eval-clean**: prompts change (2,124–4,446 files/pair differ from default) but the noise/real
profile is statistically indistinguishable from the reference. Bands caveat: measured at db1bbb6, not this commit
(leaderboard prints it); no delta was outside a band, so the caveat does not bite here.

**perf-batch25-3478dabd** (`--pipeline-arg --batch-size --pipeline-arg 25`): exit 0 ×4, cache +0 ×4, warm self-hop
byte-identical +0 writes, cold self-hop 6 ln (dedup-era, see below). Boots re-recorded on opus-4-1 ×4. Leaderboard vs
`main-2026-09-18`: **novel 4,188 (=) / realLn 416,377 (=) byte-equal**; noise 2,729 (−3); noiseLn 49,437 (−578, in
band); reloc 1,474 (+7); relocSt 352 (=); newName 3,591 (~0±15); mints 87 (~0±11); treeLn −1,021; vendorLn ~0±51.
**Eval-clean AND −9–11% cold wall — the cold lever that ships.** It is decision-changing but lands every column
in band, so it can ship as a plain config default (pending Andrew's call) or ride the relaxed recipe.

### Boot-gate model re-pin (2026-09-27)

The API now REFUSES `claude-haiku-4-5-20251001` on the walked CLIs ("may not exist or you may not have access");
sonnet-4-5, haiku-4-5 (alias) and sonnet-5 are also refused. **`claude-opus-4-1` still answers** (probed on the
2.1.85 tree). All four relaxed `*-boot.json` were re-recorded with it (both halves, live prompt `boot-ok` ×4,
note added, then re-summarized). `BOOT_GATE_MODEL`'s default in `experiments/lib/boot-gate.sh` still names the dead
id — **follow-up: change the default to claude-opus-4-1** (one-line edit, needs its own neutrality-free gate).

### Cold self-hop read 0 ln — below the reference range (92–180)

The reference range in `self-hop-reference.json` was recorded on binaries WITHOUT the #57 dedup; the dedup shares one
answer between duplicate identical requests, and a self-hop is full of them, so fewer re-rolls is the expected
mechanism. **Follow-up: re-record the reference range on a dedup-era binary** (3 same-commit cold repeats) before
any verdict leans on that range.

## 8. What is left

1. batch25 eval card (running) + its leaderboard row.
2. `docs/rust-port/20-overnight-report.md` committed on rust-port (this file) after `npm run check` — the gate may
   NOT run while the batch25 eval is on the box (heavy-run rule).
3. Merge perf/fast (3478dabd) into rust-port; then the single cold eval of the merged tree if Andrew wants one label
   for the cutover.
4. **Merge rust-port → main: waits for Andrew's sign-off.**
5. Parked: single-letter rename gap (§4); diverging-hops explanation (§2); wave-barrier redesign (deterministic
   snapshot design needed); batch>25 window collisions; matcher-preflight replacement (19-cutover open item);
   BOOT_GATE_MODEL default; self-hop reference range re-record.
