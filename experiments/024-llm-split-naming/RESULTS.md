# Exp024 — LLM naming for the split tree: 91%→100% clean, zero churn

Branch `exp024-llm-split-naming` (off `main` after exp023 merged).
Goal: make the stable split's file/folder names read like a human repo,
LLM proposing names within the naming-only law, without breaking the
cross-version stability exp023 established.

## Headline

`--split-llm-names` lifts the split tree from good-mechanical to
repo-quality names in **15 seconds** (235 LLM calls, the box) and the
names **persist across releases with zero churn** — because names live
in `_split-ledger.json`, the next release inherits them and the LLM
never re-runs.

| tree (120 leg, 23 folders / 212 files) | clean folders    | clean files        |
| -------------------------------------- | ---------------- | ------------------ |
| mechanical (exp023)                    | 21/23 (91%)      | 200/212 (94%)      |
| **LLM-named (exp024)**                 | **23/23 (100%)** | **212/212 (100%)** |

("Clean" = not var-decorated / placeholder / minted-ish / generic. The
lone file the classifier flags — `initializeModules` — is a false
positive on its `initializeModule` placeholder regex; it is a real
name.)

Real before→after, one folder:

```
mechanical  IdeConnectionManager/            LLM  ideConnection/
  IdeConnectionManager.js                       ideConnectionManager.js
  decrementOrZeroVal.js                         indexGenerator.js
  loadPluginComponentsVal.js                    pluginStatusChecker.js
  reactLib48.js                                 pluginErrorReporter.js
  isSystemStopHookSummaryWithLabel.js           systemStopHookSummary.js
  renderMcpServerDialog.js                      mcpServerDialogRenderer.js
  usePagination.js                              paginationHook.js
  nextIndex.js                                  indexGenerator.js
```

Folder list reads like a repo: `connectionValidator`, `errorProcessor`,
`issueHandling`, `promptStreamProcessing`, `transcriptEventValidation`,
`userMessage`, `styledTextRenderer`, `timeFormatting`, `proxyConfig`, …

## Stability holds — the load-bearing property

Splitting the next release (`chainA-119`) against the LLM-named
`120-llm` ledger:

- **4.1s, not 15s** — with a prior ledger present the namer is skipped
  entirely (`opts.splitLlmNames && !prior`), so no LLM calls fire.
- **Zero file churn:** `git diff -M` → 177 modified in place, 0
  renamed / added / deleted; 2,568 hunks conserved, 35 byte-identical.
  Identical to the mechanical lineage's churn — the LLM names simply
  ride the ledger. `issueHandling/issueProcessor.js` in one release is
  `issueHandling/issueProcessor.js` in the next.

This is the exp022 contract at the file-NAME axis: prior wins, fresh
decisions only for genuinely-new files/folders.

## What was built (all `npm run check` green, red/green TDD)

- **`src/split/stable-split.ts`** — an optional `SplitNamer` callback,
  invoked ONLY on the fresh-grouping path (never with a prior ledger).
  Files are named first (their stems feed the folder prompts), then
  folders, each concurrently. `acceptProposedName` validates
  (identifier-safe, not generic/placeholder/minted) and normalizes
  kebab/snake → camelCase so the whole tree uses one convention;
  any miss keeps the mechanical stem. The module stays LLM-free — it
  only calls the injected callback.
- **`src/split/split-namer.ts`** — `createSplitNamer(provider)` wraps
  the existing `LLMProvider.suggestAllNames` (reuse, not a new client),
  keyed on the mechanical stem, with a prompt carrying the segment's
  dominant bindings (inbound-ref weighted), sibling names, and member
  files. Best-effort: decline / stem-echo / provider throw all → null.
- **`src/commands/unified.ts`** — `--split-llm-names` (requires
  `--split`; no-op when a prior ledger drives the split). Threads the
  run's provider into `runSplit`/`tryStableSplit`.
- Mechanical wins folded back in: `BAD_STEM` widened (`*Val`,
  `reactLibNN`); generic-name ban-list (`utils`/`helpers`/`core`/…)
  from the smoke-probe failure mode.

## The generic-name failure mode (smoke probe → guard)

The initial probe answered `cli-utils` for a clearly
message-rendering folder. Countermeasures shipped: the generic ban-list
(a specific-but-imperfect mechanical stem beats a vague LLM name), and a
theme-weighted prompt (dominant bindings by inbound refs). In the full
run no folder came back generic — the richer prompt context fixed it.

## Honest caveats

- **Determinism:** the fresh (release-1) tree's names depend on the
  LLM; a re-run may differ. Accepted — it is a once-per-file decision,
  then persisted, exactly like every LLM name in this project. Every
  release AFTER is fully deterministic (inheritance).
- **Naming quality is model-bounded:** a few names are generic-ish at
  the margin (`arrayBuilder`, `completionHandlers`) — accurate but not
  inspired. Good enough to pass as a repo layout; a stronger model or
  a code-window prompt would sharpen them.
- **The classifier over-counts** placeholders (`initializeModules`);
  the 100% is real, spot-checked.
- Runnability (imports/exports) is still the separate exp023 emitter
  track — this experiment is names only.

## Reproduce

```bash
# fresh LLM-named tree (release 1)
HUMANIFY_REASONING_EFFORT=low npx tsx \
  experiments/023-stable-split/run-stable-split.ts \
  /tmp/e022/120F.js /tmp/e024/120-llm --llm-names

# next release inherits (namer skipped — 0 LLM calls, 0 churn)
npx tsx experiments/023-stable-split/run-stable-split.ts \
  /tmp/e022/chainA-119.js /tmp/e024/119-from-llm \
  --prior /tmp/e024/120-llm/_split-ledger.json

python3 experiments/024-llm-split-naming/name-quality.py \
  /tmp/e023-final/120 /tmp/e024/120-llm
bash experiments/023-stable-split/present-split.sh \
  /tmp/e024/120-llm /tmp/e024/119-from-llm

# in-pipeline: add --split-llm-names to the split run
```
