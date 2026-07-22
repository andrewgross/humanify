# Plan: wave-deterministic prompt context (Lever 3, part 2)

Written 2026-07-22 after the LLM response cache landed (3bef249). Nothing
here is built yet; the ceiling measurements are done and say GO.

## Why (measured)

- Two byte-identical same-session runs diverge by **65,706 output lines**
  (1,138 statements) on 118→119 while their vs-prior scores differ by 115 —
  the LLM floor churns wholesale under a stable aggregate (034 README).
- The response cache pins repeated prompts (73% rerun hit rate) but cannot
  reach byte-determinism: completion order feeds live `usedNames` /
  generated code into later prompts, and cache hits (instant) reshuffle the
  interleaving further (measured: cached rerun still diverges 46.8k lines).
- Prompt content must therefore depend only on the dependency graph, not on
  completion timing.

## Wave structure (measured on 2.1.119, `wave-profile` in the -vv log)

5,017 pending nodes drain in **10 waves** [2572, 475, 319, 185, 125, 80,
56, 14, 27, 5]; 1,159 nodes (23%) sit on dependency cycles (the existing
deadlock-break tiers handle these today). Wide-and-shallow ⇒ barrier
stall cost ≈ 10–15 straggler tails ≈ minutes per pair. **GO.**

## Design sketch

Processor dispatch (processor.ts runProcessUnifiedLoop) becomes wave-based:

1. Wave N = all pending nodes whose deps settled in waves <N (existing
   `checkNodeReady`), plus deadlock-break promotions when a wave comes up
   empty (same tier order as today: ignore scopeParent edges, then force).
2. BUILD all wave-N prompts against the frozen AST state at the wave
   barrier (buildContext / selectRequestCode / windowed usedNames all read
   a consistent snapshot — no changes to their internals, only to WHEN they
   run relative to renames).
3. Dispatch the whole wave concurrently (same limiter); responses collect.
4. APPLY all renames at the barrier in deterministic node order (graph
   iteration order), through the existing validated path; collisions
   resolve deterministically. Retry rounds happen INSIDE the wave against
   the same snapshot (retry prompts already only add prev/failure context);
   the cross-function retry batcher keeps working — batches now form
   within a wave, deterministically given deterministic membership.
5. Module-binding lane joins the same wave structure (same readiness rule).

Determinism claim: prompt bytes = f(input, prior, waves<N applications) —
all deterministic; with `--llm-cache`, reruns become byte-identical, and
the eval can finally see real effects without same-session A/B choreography.

## Validation

- Unit: wave scheduler on fake graphs (order-permutation invariance).
- E2E: cached double-run on 119 → **byte-identical outputs** (the KPI).
- Throughput: wall-time within ~1.3× of the free-running loop on a probe.
- Quality gate: same-session A/B (wave vs main) on 2 pairs — noise KPIs
  must not regress; novel/realLn frozen as always.

## Risks

- Throughput at barrier tails (bounded by the 10-wave profile; measure).
- Deadlock-tier parity: promotions must match today's semantics or names
  drift for the cycle population (1,159 nodes on 119).
- Retry-batcher timing windows inside waves (already deterministic given
  membership; verify with the double-run KPI).
