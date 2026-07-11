# Experiment 024 — LLM naming for the split tree

**Goal:** make the stable split's file and folder names read like a
human repo. The mechanical stems (exp023: "most externally-referenced
binding") are right ~70% of the time (`IdeConnectionManager/`,
`checkConnection.js`, `buildMarkdownTable.js`) but var-flavored or
misleading otherwise (`handleMessageVal/`, `rgbString/` as a FOLDER
name, `loadPluginComponentsVal.js`, `reactLib48.js`, `floor.js`,
`decrementOrZeroVal.js`). The LLM proposes names; deterministic code
does everything else — the LLM-for-naming-only law applies verbatim.

## The stability contract (non-negotiable, from exp023)

Names live in `_split-ledger.json` and inherit across releases — a
renamed file IS churn. Therefore:

- LLM naming runs ONLY when a file/folder is NEW: the fresh-grouping
  release (release 1), genuinely-new segments in later releases, or an
  explicit one-time `--rename-split-tree` upgrade pass that rewrites
  the ledger (a deliberate, flagged churn event).
- Inherited files/folders keep their ledger names forever, even if the
  LLM would name them better today. Same law as exp022: prior wins;
  fresh decisions only for the residue.

## Design

1. **Mechanical improvements first (free, deterministic):** widen
   BAD_STEM (`*Val` stems, `reactLibNN`, `lib`-ish, single short
   words like `floor` when the segment has richer candidates); prefer
   exported/high-inbound function and class names; consider the top-3
   candidates instead of argmax. Measure how much this alone fixes.
2. **LLM naming for files:** one request per file — prompt carries the
   folder context, the file's declared bindings (top ~15 by inbound
   refs, with kinds), and sibling file names to avoid collisions;
   response = a single kebab-or-camel basename. Validate: unique in
   folder, identifier-safe, not minted-shaped (`isBunToken`), not a
   BAD_STEM; on decline/invalid → mechanical stem (skip, never force).
3. **LLM naming for folders:** after files are named, one request per
   folder with its file names + top bindings; same validation.
4. **Ledger integration:** names are decided before the ledger is
   written, so persistence is automatic. The pipeline gains
   `--split-llm-names` (opt-in; requires the provider) wired into the
   fresh-grouping path of `stableSplitFromCode` via an injected naming
   callback — the module stays LLM-free; `runSplit` passes the
   callback when the flag is set.

## Smoke probe (2026-07-10, gpt-oss-20b, effort low)

A folder prompt (file list + top bindings, "reply with only a name")
returns a clean single-token answer instantly — the mechanics work.
But for a clearly message/transcript-rendering folder it answered
`cli-utils`: the generic-name failure mode. Design consequences:
ban a generic-name list (`utils`, `helpers`, `misc`, `core`, `common`,
`lib`, `main`) in validation; weight the prompt toward the DOMINANT
theme (inbound-ref-weighted bindings, maybe 2–3 signature lines of
code); and prefer the mechanical stem whenever the LLM's answer is
generic — a specific-but-imperfect name beats a generic one.

## Metrics + success criteria

- **Name quality:** share of files/folders with placeholder/var-shaped
  names (`*Val`, `noop*`, `reactLibNN`, minted-ish) — measured on the
  212-file tree; target near-zero. Human eyeball of the full tree
  listing (the real bar: "would this pass review as a repo layout?").
- **Stability unchanged:** re-run the exp023 presentation — file-list
  churn must stay ZERO on the lineage pair (names ride the ledger).
- **Determinism bound:** LLM naming is once-per-file, persisted;
  re-running release 1 may differ (accepted, like all LLM naming).

## Runbook

- Box: `http://192.168.1.234:8000/v1`, `openai/gpt-oss-20b`,
  `HUMANIFY_API_KEY=local`, `HUMANIFY_REASONING_EFFORT=low`. Owned
  hardware — wall-clock is the only budget.
- Artifacts: `/tmp/e023-final/120` (fresh tree), `/tmp/e022/120F.js`
  (leg to re-split), `/tmp/exp023-chain/cc-119-lineage/` (pipeline
  tree). Regenerate via `experiments/023-stable-split/run-stable-split.ts`.
- Offline iterate: drive the naming callback in a harness over the 120
  tree before touching the pipeline; present with
  `experiments/023-stable-split/present-split.sh`.
- House rules: red/green TDD for src changes, `npm run check` green
  per commit, complexity ≤ 15, branch `exp024-llm-split-naming`, no
  merge — Andrew reviews.

## Code anchors (verified 2026-07-10 on main d00f408)

- Naming today: `src/split/stable-split.ts` — `segmentStem`, `BAD_STEM`,
  `betterStem`, `uniqueName`, `assignFresh` (folder+file naming);
  budgets/threshold notes in the module docblock.
- Pipeline: `src/commands/unified.ts` — `runSplit`, `tryStableSplit`,
  `loadPriorSplitLedger`, `--split-ledger` flag.
- Provider: `src/llm/openai-compatible.ts` (`suggestAllNames`); sweep
  prompt shape worth mirroring: `src/rename/coverage-sweep.ts`
  `nameGroup`.
- Presentation: `experiments/023-stable-split/present-split.sh`.

## Out of scope

- Renaming BINDINGS (the rename campaign owns that).
- Runnable emission (exp023 task #7 — separate track).
- Re-clustering / moving statements between files.
