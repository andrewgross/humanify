# The pipeline, stage by stage

What actually runs, in order, and which stages can have their strategy swapped.

This exists because the working mental model was **four** stages — detect,
unbundle, name, place — and the code has **twelve**. The four that were named
are real; the eight that were not are where most of the measured noise has come
from. `vendor/` alone went unscored for thirteen experiments at 2.4× the entire
measured `src/` noise (measurement-pitfalls rule 8), and it is a stage nobody
had written down.

The tuned numbers each stage runs with — where each lives, what it was
measured on (almost always Claude Code, often nothing), and whether it depends
on the app, the bundler or the model — are in
[`tuning-values.md`](./tuning-values.md).

## The stages

Ordered as they execute. "Pluggable" means a strategy can be selected without
editing the caller.

| #   | stage                    | entry point (`humanify_core::…`, driven by `humanify_cli::unified`)  | pluggable?                                                                                          |
| --- | ------------------------ | -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| 1   | Detect bundler/minifier  | `detect` → `toolchain::resolve_toolchain` (every plugin piece, once) | **yes** — `--bundler` / `--minifier` override detection                                             |
| 2   | Select unpack adapter    | `unpack::choose_adapter` (called by the toolchain)                   | **yes** — registry of 4 (webcrack, bun, esbuild, passthrough), `supports()`, passthrough last       |
| 3   | Unpack the bundle        | `unpack::run_adapter` (the ONE dispatch site; webcrack via its shim) | via stage 2 — bun and esbuild share the one vendor-extraction flow (`unpack::bun::unpack_bun`)      |
| 4   | Detect libraries         | `libdetect`                                                          | **yes** — registry of 3, `supports()`, default last                                                 |
| 5   | Name vendor files        | `modules` (vendor manifest + namer, prior carry-over)                | injected namer, one implementation — a seam, not a registry                                         |
| 6   | Format                   | `format` (`core::format`, the native formatter)                      | **no** — deliberately; output shape is a fixed point (frozen spec: test/parity/format-goldens.json) |
| 7   | Build the function graph | `graph`                                                              | **no**                                                                                              |
| 8   | Match against the prior  | `matching` + `twins` (the fingerprint cascade)                       | **no** — the cascade is hard-coded order, see below                                                 |
| 9   | Name identifiers         | `naming` (LLM waves + prior transfer, `rename`)                      | **partly** — the name profile registry (bun/esbuild/terser/swc/none); levers toggle passes          |
| 10  | Place statements         | `place::tiers` (`PLACEMENT_TIERS`)                                   | **partly** — a real registry, but not selectable from outside                                       |
| 11  | Split (one path)         | `emit::stable_split`                                                 | **no** — prior present → inherit layout; no prior → clustered fresh grouping                        |
| 12  | Emit + finish on disk    | `emit::cjs`, `finish` (scaffold, relink, ledgers)                    | **no**                                                                                              |

Stage 9 has a step that decides stage 11's FILE names (2026-10-06,
docs/design/module-naming.md): when the bundle's module markers describe
it (the rule the split method reads, `place::method::markers_describe_bundle`),
the **module step** (`naming::module_names`) marks every new, non-barrel
module before the first wave — so no wave asks its lazy-init wrapper — and,
after the waves and the library-prefix pass, before the naming floor and
sweep, asks the model what each module is FOR (8 per call, the file namer's
module kind) and names its wrapper `init<Name>`. The split then reads each
new module's file name back from its wrapper (`initColorUtils` →
`color-utils.js`) and names inferred folders from the same names; the old
warm-hop mint namer is gone. Inherited paths are never renamed.

Since the cutover (docs/rust-port/19-cutover.md) the pipeline is the Rust
binary; the TypeScript names used below (`stableSplitFromCode`,
`PLACEMENT_TIERS`, …) are the historical names of the same stages, kept
because the experiment records use them — `docs/rust-port/01-current-architecture.md`
maps each to its Rust module.

Three stages sit _after_ placement and are easy to forget when reasoning about
output, because they run once the tree looks finished:

- **post-split reconcile** (`finish::reconcile`) — renames inside split
  files, after every prompt. Deterministic; this is why a draw-pinned A/B is
  licensed to measure it.
- **carry into bundle** (`finish::carry`) — writes names back into
  `.humanify/humanified.js`, which becomes the NEXT release's prior. Top-level
  renames must never carry: the export key is a string, and 238/238 drifted.
- **finish on disk** — scaffold, the vendor factory relink (bun and esbuild trees alike: the relink reads the one manifest format both adapters write), ledgers, eval stats. The relink has two answers for a vendor body's out-of-body references (finding #51/#60): recognized Bun interop helpers are bound from `.humanify/__bun-runtime.js`, and app-scope READS recorded by the unpack's scope plan are bridged — a lazy `require(<owner file>).<accessor>` splice through the live getter the emit was forced to export. Writes and non-statement-level bindings keep the factory in the app; nothing else may leave a free name in a vendor file.

After those, the run's REPORTS are written, in this order: `--diagnostics`
(the naming report with the split's placement trail after the strategy
trail), `--stats-json`, `--dump-artifacts`, `--rename-ledger`. They observe
decisions already made — except `--rename-ledger`, which is NOT inert: it
gates the family permute off (plugin.ts `finalizeWithFamilyPermute`), so a
ledger run ships a different tree. (`humanify_cli::unified::RunReports`, the
dump in `humanify_core::artifact_dump`.)

## What was missing from the four-stage model

Stages 4, 5, 7, 8, 12 and all three post-placement passes. In particular:

- **Matching (8) was folded into "naming".** It is a separate question with its
  own failure modes: naming decides what a thing should be called, matching
  decides whether it is the same thing as last release. Most cross-version
  noise is a matching failure, not a naming one. Stage 8 has its own
  inspection verb since exp092: `humanify match` runs the stage cold (no
  LLM path exists in it) and dumps every per-function/per-statement
  decision — the ground-truth harness's instrument.
- **Vendor (5) was absent entirely** — the rule-8 blind spot.
- **Carry (post-placement) was absent**, and it is the only stage whose output
  is consumed by a _future_ run.

## Strategy selection: where the seams actually are

**The toolchain (2026-10-04, finding #76).** Every plugin piece a run uses
is chosen ONCE, at stage 1, by `humanify_core::toolchain::resolve_toolchain`
— from the detection verdict and the `--bundler` / `--minifier` flags — and
handed to the stages that need it as a value (`Toolchain`): the unpack
adapter, the vendor record's stamp, the library detector, the never-rename
lists, the name profile, the module-layout record, the per-bundler tuning,
and four slots that hold today's only implementation (the module wrapper
grammar, the interop helpers, the bundle layout, which unpacked file is
the app). No stage below it compares bundler or minifier names. Each choice
and its reason (flag / detected / fallback / only-implementation) is
written to the `--stats-json` `toolchain` block and logged at `-vv`, so a
run's plugin choice is visible. The registries below are what the
toolchain selects FROM. docs/plugin-spec.md is the per-piece status.

**Two** stages have a real registry — 2 (unpack) and 4 (library detection) —
both the same shape: an array, selection by name or `supports()`, a fallback
last. Stage 10 (`PLACEMENT_TIERS`) is a registry internally but is not
selectable from outside. Stage 9 gained a third, smaller one on
2026-10-03: the minifier **name profile** (`rename::name_profile`,
`NAME_PROFILES` — bun, esbuild, terser, swc, none), the shape knowledge
behind "does this name look minifier-made?". It is chosen once, next to the
unpack adapter (`select_name_profile`, from the `--minifier`/`--bundler`
flags or a definitive bun/esbuild bundler verdict; an unsure input stays on
bun), and passed down; it changes which names count as minted and which
answers are refused, not which passes run. (Stage 11's split-adapter registry was deleted with
the legacy splitter, 2026-08-12.) The unpack registry gained **esbuild** as
its second bundler (exp075's module form, ported 2026-10-02): the same
vendor-extraction implementation as bun — the two differ only in the factory
wrapper shapes (`modules::factory_arg_function`, the one unwrapping owner)
and in what esbuild's unminified builds hand over for free: each module's
original source path, recovered from the factory object's key and recorded
(`FactoryRecord::source_path`, the vendor manifest's `sourcePath`, the
ledger's `fossilModules[].sourcePath`) — recorded metadata ONLY; no name
source, no join key, no behavioral switch reads it.

**The second splitter is GONE (2026-08-12).** Until then a legacy
clustering splitter (`splitFromAst` + a 4-adapter registry + the
`cluster.ts`/`reference-cluster.ts` machinery, ~300 functions) survived as a
silent mid-run fallback when `stableSplitFromCode` declined the input — a
whole bespoke second path the execution census measured at zero runs outside
its own tests. There is now ONE split path: the stable split, which composes
itself upfront from what it knows (prior version → inherit layout; no prior →
seam-clustered fresh grouping via `assignClustered`, with the LLM naming
folders). An input it cannot handle FAILS LOUDLY instead of being re-split a
cruder way. The `--split-strategy` knob, its adapter registry, and the
standalone `split` command are all deleted; future input formats (webpack,
electron) should join as upfront detection + explicit pipeline pieces, not as
fallbacks.

[`plugin-spec.md`](./plugin-spec.md) (2026-10-03) inventories every place
the pipeline still assumes Bun (or another specific bundler/minifier) and
specifies what a new bundler or minifier plugin must supply, piece by piece,
marking which pieces are a real plug point today.

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
