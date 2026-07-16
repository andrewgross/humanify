# Results: human-like split layout + vendor unification (2026-07-16)

Branch `feat/human-layout`. Measured on Claude Code 2.1.89. See the plan
doc (`plan-2026-07-16-human-layout.md`) for the item checklist.

## src/ tree (fresh split of the 2.1.89 humanified single file)

Harness: re-split `.humanify/humanified.js` with no prior ledger (fresh
grouping), mechanical stems and with the local LLM namer.

| Metric                   | Old tree (on disk)                     | New (this branch) |
| ------------------------ | -------------------------------------- | ----------------- |
| src top-level dirs       | 161                                    | 24                |
| single-file dirs         | 79                                     | 0                 |
| junk drawers (>40 files) | 7 (90–99 files)                        | 0                 |
| dir size median / max    | bimodal 1…99                           | 13 / 26           |
| tree depth               | fixed 2                                | 1–2 (variable)    |
| `-N` / minted dir names  | many (errorBuilders-4, noopFunction36) | 0                 |
| files under 20 lines     | 254                                    | 6                 |

Top-level now reads as domains (with the LLM namer): `textInputHandler`,
`hostnameResolver`, `writePermissionEvaluator`, `transcriptModalManager`,
`digitInputHandler` — vs the old `layoutDirection/` (96 files),
`executionEngine/` (96), `wordExtractor/` (99).

## vendor/ (LLM naming + package grouping, original bundle via the adapter)

Harness: `BunUnpackAdapter.unpack` with `createVendorNamer` over the local
LLM (`openai/gpt-oss-20b`).

| Metric                                 | Before          | After                                   |
| -------------------------------------- | --------------- | --------------------------------------- |
| vendor files with a real name          | 25 (banner+url) | ~760 (banner+url+llm)                   |
| lib\_<hash> anonymous                  | 1498            | ~762                                    |
| `H.js` / `DepType.js` (minified stems) | present         | floored to lib\_<hash>                  |
| `highlight.js.js` (double ext)         | present         | `highlight.js`                          |
| package folders                        | 0 (flat)        | grpc-js/opentelemetry/aws-sdk/… grouped |

(Exact grouping counts are in the scratchpad run output; re-run
`vendor-name-run.mts` to refresh.)

## Duplication unified (the arch-review concern, applied here)

`shared/cjs-factory.ts` is now the single definition of the Bun CJS
factory declarator SHAPE and the vendor filename FLOOR. Three call sites
had drifted and each lost a real case:

- classification matched the shape without the param check;
- cluster-assign matched single-declarator statements only → comma-joined
  factories (common in real Bun output) escaped to src/;
- the unpack adapter wrote raw minified factory vars as filenames (H.js).

A fix in the shape or floor now lands in one place.

## Refuted / deferred

- **Item 9 (ESM-inlined library extraction)** — the inlined libraries
  (zod, smithy/bedrock) are NOT contiguous in the wrapper body (probe:
  zod spans statements 1140..13757, interleaved), so a contiguous-span
  island detector can't capture them and reference-closure expansion from
  an SDK seed swallows the whole body via shared runtime helpers. Needs
  per-statement label propagation from string-fingerprint seeds, with an
  experiment + ground truth. Not this branch.
- **Item 8 satellite propagation** — grouping is name-based here; pulling
  hash-named satellite modules into their parent package via the
  factory→factory reference graph is a safe follow-up increment.

## Follow-ups worth a note

- Vendor LLM naming has a precision tail: the model gave 81 distinct
  modules the same name (`is-plain-object`) — hallucination on tiny
  utility modules. Grouping contains them in one folder, but a
  confidence gate (or cross-checking the export set) would help.
- Vendor LLM names are not carried across versions (no structural-hash
  carry-over for the llm source), so a package could be renamed between
  releases. The deterministic cascade (banner/url) and lib\_<hash>
  fallback ARE stable; only the llm tier floats. Worth a carry-over map
  before relying on vendor names for cross-version diffs.
