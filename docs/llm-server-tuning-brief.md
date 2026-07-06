# LLM server tuning brief — humanify workload

For the agent with SSH access to the LLM box. Goal: increase **aggregate
token throughput** for humanify's rename pipeline. Per-request latency is
almost irrelevant — the client keeps ~50–70 requests in flight and only
total completion rate matters. Everything below was measured 2026-07-05
against a live production run.

## Deployment facts (verify on the box)

- Endpoint: `http://192.168.1.234:8000/v1` (OpenAI-compatible)
- Server: **vLLM 0.13.0** (`GET /version`); Prometheus metrics live at
  `GET /metrics` — use `vllm:num_requests_running`,
  `vllm:generation_tokens_total`, `vllm:prompt_tokens_total`,
  `vllm:gpu_cache_usage_perc` for before/after comparisons.
- Model: `openai/gpt-oss-20b`, `max_model_len` **61,200** (from
  `/v1/models`). GPU type/count, quantization, and the exact `vllm serve`
  flags are unknown from the client side — capture
  `ps aux | grep vllm`, `nvidia-smi`, and any systemd/docker unit first.

## Workload profile (measured)

Two run shapes, produced by `experiments/013-bun-cjs-classification/run-phase2.sh`:

1. **Fresh humanify** (Run A): every function/binding batch goes to the
   LLM. Measured today: **15,340 requests in the first 60 min**
   (~4.3 req/s completed), **19M prompt + 7.4M completion tokens**
   (~5.3K tok/s aggregate), ~70 requests in flight steadily,
   per-request latency 2.4–3.5 s. Full run ≈ 40–55K requests.
2. **Prior-version humanify** (Run B): exact matches skip the LLM;
   after the hash fix ~35K/43K functions transfer, so only ~8–15K
   requests. Same request shape.

Request shape (all requests):

- `POST /v1/chat/completions`, non-streaming, OpenAI Node SDK,
  client timeout 30 s.
- 2 messages: **system prompt is byte-identical on every request**
  (~287 tokens, `BATCH_RENAME_SYSTEM_PROMPT`) + a user prompt with a
  code snippet and ~10 identifiers to name.
- Averages: **~1,240 prompt tokens, ~480 completion tokens** per
  request (19M/7.4M over 15,340).
- Params: `response_format: {type: "json_object"}`,
  `temperature: 0.3`, `max_tokens: 6000`.
- Concurrency from the client: function lane 50 + module lane 20
  (esbuild bundles: 40) → ~70 peak. In the run's tail the ready set
  thins (dependency ordering), in-flight decays to ~20 — that phase is
  client-scheduling-bound, not server-bound.

## Finding #1 — reasoning channel is eating the output budget

Live probe against the box (trivial 3-identifier request):

```
usage: { prompt_tokens: 109, completion_tokens: 152 }
content: {"a":"add","b":"num1","c":"num2"}   ← ~12 tokens visible
```

**>90% of completion tokens are the gpt-oss reasoning/analysis channel**
(harmony format), which vLLM generates and counts but does not return in
`content`. Naming identifiers does not need medium reasoning. Scaled to
the run, most of the 7.4M completion tokens — the decode-bound part of
the workload — are invisible reasoning.

Things to try, in order:

1. Serve-side default: vLLM's gpt-oss integration supports a reasoning
   effort control (`--reasoning-effort low` on newer builds, or the
   harmony `Reasoning: low` system-prompt directive). Verify what 0.13.0
   supports for this model.
2. Client-side: we can add `reasoning_effort: "low"` to the request body
   (one-line change in `src/rename/plugin.ts`/`openai-compatible.ts` —
   coordinate with us; we'll A/B name quality on the preact fixture
   before adopting).
3. Measure name QUALITY before/after on the smoke test below — if names
   degrade at `low`, try the system-prompt directive variant.

Expected effect if reasoning drops to ~10–20% of current: completion
tokens per request fall from ~480 to ~100–150 → decode work drops
60–75%. This is likely the single biggest win available.

## Finding #2 — prefix caching should be free money

Every request shares the identical ~287-token system prompt, and retry
requests re-send large user-prompt prefixes. Check whether
`--enable-prefix-caching` is on (default varies by version/model;
verify via serve flags and `vllm:gpu_prefix_cache_hit_rate` if
exported). With ~50K requests/run this saves ~14M prefill tokens/run
plus KV-cache pressure.

## Finding #3 — structured output backend

`response_format: {type: "json_object"}` engages guided decoding on
every request. Backends differ a lot in per-token overhead
(outlines vs xgrammar vs llguidance; `--structured-outputs-config` /
`--guided-decoding-backend` depending on version). Check which backend
0.13.0 is using and whether xgrammar (usually fastest) is active. If
guided decoding is serializing on CPU, this caps throughput regardless
of GPU headroom. A quick A/B: run the benchmark below with and without
`response_format` client-side (the client parses JSON leniently and has
a regex fallback — it tolerates non-strict mode for a test).

## Standard throughput levers (check current values first)

- `--max-num-seqs` (concurrent sequences): needs ≥ ~96 to keep 70
  in-flight plus headroom. If it's at a low default, in-flight requests
  queue server-side.
- `--max-num-batched-tokens` / chunked prefill settings: our prefills
  are small (~1.2K); many small prefills benefit from larger batched
  token budgets.
- `--gpu-memory-utilization` (default 0.9): if there's VRAM headroom,
  raising KV cache capacity increases achievable batch size. Watch
  `vllm:gpu_cache_usage_perc` during a run — if it's pinned near 100%,
  KV cache is the ceiling; consider `--kv-cache-dtype fp8`.
- `max_model_len` 61,200: our prompts are ~1.2K avg, worst retries are
  a few K. Reducing max_model_len to e.g. 16K frees KV budget per
  sequence and can raise max concurrent sequences substantially. Verify
  our real p99 prompt length in the logs first
  (`/tmp/exp013-phase3/cc-119.log`, `promptTokens` fields).
- Quantization: confirm the checkpoint (gpt-oss ships MXFP4 MoE
  weights). If it's running bf16, the quantized build roughly doubles
  decode throughput on the same card.
- Speculative decoding: JSON-with-short-names output is highly
  predictable; if the box has headroom, an n-gram/eagle draft can help
  decode-bound loads. Lower priority than #1–#3.
- Second replica / bigger card: the client takes a single endpoint;
  a load balancer in front of two vLLM replicas needs no client change.

## How to benchmark (safe, ~2 min, no repo knowledge needed)

Baseline smoke (from repo `/Users/andrewgross/Development/humanify`,
or the pinned worktree `/tmp/humanify-run-main`):

```bash
cd /tmp/humanify-run-main && HUMANIFY_API_KEY=local \
node --import tsx/esm src/index.ts \
  /Users/andrewgross/Development/humanify/test/e2e/fixtures/preact/minified/v10.24.0/terser-default.js \
  -o /tmp/smoke-preact --endpoint http://192.168.1.234:8000/v1 \
  --model openai/gpt-oss-20b
```

Today's baseline for that command: **1m36s, 78 LLM calls,
115.7K tokens (67.6K in / 48.2K out)**. It prints calls/latency/token
totals at the end — use those numbers for A/B. Name-quality eyeball:
`/tmp/smoke-preact/index.js` should have descriptive camelCase names,
not junk.

Raw-throughput probe without our client (isolates server changes):

```bash
vllm bench serve --backend openai-chat --base-url http://192.168.1.234:8000 \
  --model openai/gpt-oss-20b --num-prompts 500 --max-concurrency 70 \
  --random-input-len 1200 --random-output-len 480
```

## Constraints / coordination

- **A production run is likely in flight** (Runs A/B take ~2 h + ~1–2 h;
  they auto-chain). Before restarting vLLM, check activity:
  `curl -s http://192.168.1.234:8000/metrics | grep num_requests_running`.
  Client-side progress: `ps aux | grep 'tsx/esm /tmp/humanify-run-main'`
  on this machine. Restarting mid-run kills hours of work — the client
  contains request failures as "unrenamed" rather than crashing, so a
  bounce mid-run silently degrades output quality instead of failing
  loudly.
- Client-side knobs (batch size 10, lanes 50+20, `max_tokens`,
  `reasoning_effort`, temperature) live in this repo — if server
  findings suggest different client behavior (e.g. bigger batches to
  amortize guided-decoding setup, or explicit reasoning_effort), report
  back and we'll change them here; don't work around them server-side.
- The 30 s client timeout is the hard latency ceiling: any change that
  trades latency for throughput must keep p99 under ~25 s.

---

## Client-side update (2026-07-06, after workstream 2 landed)

The client now supports `--reasoning-effort low|medium|high` (sent as
`reasoning_effort`; omitted when the flag is absent). A/B on the baseline
smoke above, same box, new client (temperature 0, retry batching, retry
context diet, 2-call per-identifier cap):

| run                                  | wall     | calls | tokens (in/out)    | identifiers renamed |
| ------------------------------------ | -------- | ----- | ------------------ | ------------------- |
| old baseline (temp 0.3)              | 1m36s    | 78    | 115.7K (67.6/48.2) | —                   |
| new client, default effort           | 2m09s    | 80    | 142.6K (67.0/75.6) | 182/194             |
| new client, `--reasoning-effort low` | **9.9s** | 78    | 71.6K (62.9/8.8)   | **192/194**         |

Notes: temperature 0 alone LENGTHENED the gpt-oss reasoning channel
(48.2K → 75.6K output tokens) — determinism is kept for reproducibility,
and `low` effort more than pays it back. Name quality at low effort is
equivalent on eyeball (createVirtualNode / hydrateComponent /
commitFiberTree / handleErrorPropagation). run-phase2.sh now defaults to
low via HUMANIFY_REASONING_EFFORT. Server-side levers from the brief
(prefix caching, scheduler) remain open.
