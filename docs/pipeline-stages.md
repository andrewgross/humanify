# The pipeline, stage by stage

What actually runs, in order, and which stages can have their strategy swapped.

This exists because the working mental model was **four** stages — detect,
unbundle, name, place — and the code has **twelve**. The four that were named
are real; the eight that were not are where most of the measured noise has come
from. `vendor/` alone went unscored for thirteen experiments at 2.4× the entire
measured `src/` noise (measurement-pitfalls rule 8), and it is a stage nobody
had written down.

## The stages

Ordered as they execute. "Pluggable" means a strategy can be selected without
editing the caller.

| #   | stage                    | entry point                                    | pluggable?                                                          |
| --- | ------------------------ | ---------------------------------------------- | ------------------------------------------------------------------- |
| 1   | Detect bundler/minifier  | `detectBundle` → `buildPipelineConfig`         | **yes** — `--bundler` / `--minifier` override detection             |
| 2   | Select unpack adapter    | `selectUnpackAdapter` (`src/unpack/index.ts`)  | **yes** — registry of 3, chosen by name, passthrough last           |
| 3   | Unpack the bundle        | `unpackBundle` → `adapter.unpack`              | via stage 2                                                         |
| 4   | Detect libraries         | `selectLibraryDetector` (`library-detection/`) | **yes** — registry of 2, `supports()`, default last                 |
| 5   | Name vendor files        | `vendorNamer`, `priorVendorNames`              | injected function type, one implementation — a seam, not a registry |
| 6   | Format                   | `createBabelPlugin`                            | **no** — deliberately; output shape is a fixed point                |
| 7   | Build the function graph | `buildFunctionGraph` / `buildUnifiedGraph`     | **no**                                                              |
| 8   | Match against the prior  | `matchFunctions` + the fingerprint cascade     | **no** — the cascade is hard-coded order, see below                 |
| 9   | Name identifiers         | `createRenamePlugin` (LLM + prior transfer)    | **no** — levers toggle passes, they do not select a strategy        |
| 10  | Place statements         | `PLACEMENT_TIERS` (`stable-split.ts`)          | **partly** — a real registry, but not selectable from outside       |
| 11  | Select split adapter     | `selectSplitAdapter` (`split/adapters/`)       | registry of 4 — but see the unreachable-override note below         |
| 12  | Emit + finish on disk    | `emitRunnableCjs`, scaffold, relink, ledgers   | **no**                                                              |

Three stages sit _after_ placement and are easy to forget when reasoning about
output, because they run once the tree looks finished:

- **post-split reconcile** (`post-split-reconcile.ts`) — renames inside split
  files, after every prompt. Deterministic; this is why a draw-pinned A/B is
  licensed to measure it.
- **carry into bundle** (`bundle-carry.ts`) — writes names back into
  `.humanify/humanified.js`, which becomes the NEXT release's prior. Top-level
  renames must never carry: the export key is a string, and 238/238 drifted.
- **finish on disk** — scaffold, bun factory relink, ledgers, eval stats.

## What was missing from the four-stage model

Stages 4, 5, 7, 8, 12 and all three post-placement passes. In particular:

- **Matching (8) was folded into "naming".** It is a separate question with its
  own failure modes: naming decides what a thing should be called, matching
  decides whether it is the same thing as last release. Most cross-version
  noise is a matching failure, not a naming one.
- **Vendor (5) was absent entirely** — the rule-8 blind spot.
- **Carry (post-placement) was absent**, and it is the only stage whose output
  is consumed by a _future_ run.

## Strategy selection: where the seams actually are

**Three** stages have a real registry — 2 (unpack), 4 (library detection) and
11 (split) — all three the same shape: an array, selection by name or
`supports()`, a fallback last. Stage 10 (`PLACEMENT_TIERS`) is a registry
internally but is not selectable from outside.

**The split-strategy override is unreachable from the main pipeline.**
`--split-strategy` is registered on the standalone `split` command
(`src/commands/split.ts:118`) and threaded through `splitFromAst`. The unified
pipeline splits via `stableSplitFromCode` and never passes `splitStrategy` at
all, so on the path everything actually runs through, `selectSplitAdapter`'s
override argument is always `undefined`. The registry is real; the knob is
connected to the other door.

Everything else is a fixed call. That is not automatically wrong — a seam with
one implementation is speculative generality — but it is worth knowing which
is which before planning work that assumes a plug point exists.

Both corrections above were found by checking the table against the code after
writing it: stage 4 was credited with less structure than it has, and stage 11
with a CLI flag it does not have on this path.

**Not proposed: runtime-measured strategy selection** (trying several and
keeping the best by score). It needs a per-stage quality metric that is
trustworthy at single-run scale, and the repo's own measurement history is the
argument against believing we have one: rule 11 says the src/ per-hop draw band
is ±2,800 lines, so a selector scoring two strategies on one run would be
choosing noise and reporting a confident winner. Revisit only if a stage gets a
metric whose noise floor is known and smaller than the differences it must
resolve.

## The cascades

Fourteen ordered-fallback cascades run inside stages 8–10 — decide-by-first-hit
ladders that are distinct from the three adapter registries above, which pick a
strategy once per run. Exactly one of the fourteen, `PLACEMENT_TIERS`, is
declared as an array with counters and a trail derived from it; the rest are
hand-written `if` ladders with no per-stage counter and no per-item trail, which
is why explaining a single decision has repeatedly needed offline
reconstruction.
Where a cascade _does_ have counters, they have paid for themselves: the
`singletonUnguarded` counter exists because a guard was structurally dead for
11,094 accepts and reported it as `singletonRejected: 0`.

See [`responsibility.md`](./responsibility.md) for who owns which question
within these stages.
