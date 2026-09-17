# 090 — model swap to GLM-5.3-Flash: the A/B record

> **STATUS 2026-09-17: measurement only.** The gpt-oss-20b endpoint died at
> 15:07 UTC; Andrew replaced it with GLM-5.3-Flash-NVFP4 (vLLM, :8100, 1M
> context). pairs.json re-pointed at `a97ab99`. Baseline walk:
> `/work/glm-base-walk` at that commit — pipeline code byte-identical to
> the last gpt-oss walk (`/work/exp088-walk`, f3ebff0), so the delta is the
> MODEL alone.

## Speed (the corrected assessment)

A 32-call burst matches the old server (~12 req/s) — but that probe misled:
reasoning tokens roughly double per-call output, so the cold hop runs ~3h
(was ~1h 9m) and a full walk ~4h. Andrew: fine for now, likely swap back
later — at which point EVERYTHING below re-baselines again.

## Conflicts and retries, cold hop (like-for-like, same code)

| signal                     | gpt-oss |    GLM | change |
| -------------------------- | ------: | -----: | ------ |
| duplicate name suggestions |  23,609 |  7,594 | −68%   |
| rename fallbacks           |  11,580 | ~3,100 | −73%   |
| fully-invalid batches      |   8,350 | ~2,750 | −67%   |
| retry calls (2nd+ attempt) |  17,253 | ~8,100 | −53%   |
| scope-unsafe suggestions   |   3,069 | ~1,900 | −38%   |

The smarter model obeys uniqueness/avoid-list instructions far better; the
2x wall-clock cost is pure reasoning latency, partially offset by half the
retry traffic.

## Walk KPIs (GLM baseline vs last gpt-oss walk, same code)

| KPI                          |           gpt-oss (exp088-walk) |                                GLM (glm-base-walk) |
| ---------------------------- | ------------------------------: | -------------------------------------------------: |
| `novel` / `realLines` busy   |                   986 / 122,066 | **986 / 122,066 — byte-identical across the swap** |
| `novel` / `realLines` calm   |                    146 / 33,135 |                            **146 / 33,135 — same** |
| busy churnExBuild            |                          24,534 |                                  **24,306 (−228)** |
| busy nameOnlyLines           |                           4,314 |                                   **4,144 (−170)** |
| calm churnExBuild / nameOnly | 24,653-era best 245-277 / 16-46 |                      **245 / 16 — best on record** |

The exactness KPIs surviving a MODEL SWAP byte-for-byte is the strongest
evidence yet that they are matcher-owned and model-independent. The model
alone bought ~170 lines of busy rename noise (better word-choice
consistency), ~2x the observed nameOnly draw spread — directionally real,
single-repeat caveat applies.

## Standing rules stamped by this swap

- ALL pre-2026-09-17 walk baselines are gpt-oss numbers. Cross-model deltas
  are meaningless; compare within an era only. Era is identifiable by
  workdir (`glm-*` prefix) and by pairs.json history.
- The walk noise band (35/32) is a gpt-oss measurement. Re-measure on GLM
  (two same-commit walks) before any tight call; until then treat GLM
  deltas under ~200 lines as unresolved.
- exp089's verdict baseline is THIS walk (busy nameOnly 4,144).
