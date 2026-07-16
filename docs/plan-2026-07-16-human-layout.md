# Plan 2026-07-16: human-like split layout + vendor unification

Branch: `feat/human-layout`. Goal: the split tree of a real bundle (CC 2.1.89)
should look like a repo a human laid out. Diagnosis lives in the 2026-07-16
conversation; measured on /Users/andrewgross/Development/unpacked-claude-code/
versions/claude-code-2.1.89 (161 top dirs, 79 singletons, 7 drawers of 90-99
files, 1498/1523 vendor files named lib\_<hash>, zod + AWS Bedrock inside src/).

No backwards compat needed anywhere: prior unpacked versions will be
regenerated from scratch after this lands (user said so explicitly).

## Checklist

- [x] 1. Windowed wall-picking — `pickWalls`/`subWallsWithin` (commit 1cb86d9)
     (src/split/cluster-assign.ts:192): wall must land within
     [minPerGroup, maxPerGroup] cuts of the previous wall; take the deepest
     seam inside that window. Kills both 79 singleton top dirs and 90-99-file
     drawers. New config fields (minTop/minSub), keep determinism.
- [x] 2. (commit 8c46a00) Variable depth in `nameSegments` (cluster-assign.ts:409): top group
     with few files (<= ~8) emits files directly at src/<top>/ (no sub);
     only-child sub collapses into parent unconditionally; single-file dir
     hoists the file up a level.
- [x] 3. (commit 89802d6) Folder collisions merge instead of suffixing: same-level folders
     whose polished names are case-insensitively equal become ONE folder
     (folders need no contiguity; only files are contiguous runs). Fuzzy
     stutter collapse: camelCase-tokenize stems, collapse child into parent
     on token-subset (abortError vs abortErrorHandling) with plural
     normalization. Ban minted/ordinal stems (noopFunctionNN, doNothingNN,
     trailing -N) as directory names — mechanical stems too, not just LLM
     proposals.
- [x] 4. (commit 4cf0797) Bottom-up naming with evidence: name files first, then folders from
     member lists (pass `members` — split-namer.ts already renders it);
     reject folder proposal equal to a single member's name. Joint one-call
     naming for the top level (all top groups in one suggestAllNames batch)
     for coherent sibling domains.
- [x] 5. (commit f74a3b8) Stub consolidation: minLines floor (default 25),
     stub runs merge into a neighbor, budget caps win (no extreme-seam
     exception — kept simple); segmentStem falls back to "stubs" instead
     of leaking a banned name. Target: fewer sub-20-line files (was 254).
- [x] 6. (commit afe1489) Unify factory detection + vendor filename floor: ONE module used by
     both bun-module-classification.ts (buildFactoryRecord) and
     cluster-assign.ts (factoryCallee/detectCjsHelper). Vendor filenames
     from fc.binding must pass the naming floor (no 1-2 char names like
     H.js, no minified patterns) — fall back to the classification cascade
     name / lib\_<structuralHash8>. Fix package names ending in .js
     (highlight.js -> highlight.js.js today).
- [x] 7. (commit ff260a0) Fill the stubbed vendor LLM naming step
     (bun-module-classification.ts:209 nameCjsFactories): batch unnamed
     factories through suggestAllNames with a code window (exports, top
     string literals, URLs); floor-validate; lib_hash fallback stays.
- [x] 8. (commit 4690894) Package grouping — SCOPED to name-based (not the
     reference-graph propagation originally sketched): a package that
     IDENTIFIES >=2 modules (banner/url/llm, exact-name) groups into
     vendor/<package>/lib\_<structuralHash8>.js; single-module packages
     stay flat; runtime identifier decoupled from display path (stable
     structural stem) so folders never churn runtime.js. The
     reference-graph propagation of hash-named SATELLITES into their
     parent package is the deferred follow-up (needs the factory→factory
     edge graph; safe increment on this).
- [ ] 9. ESM-inlined library extraction — REFUTED AS DESIGNED by probe
     (scratchpad island-probe.mts on 2.1.89, 2026-07-16): the inlined
     libraries are NOT contiguous (zod markers span statements
     1140..13757, smithy/bedrock 3052..14069 — interleaved by Bun), and
     reference-closure expansion from an SDK seed swallows the whole
     20,308-statement body via shared runtime helpers; URL evidence also
     false-positives on CC's own doc strings. Viable v2 = per-statement
     label propagation from string-fingerprint seeds over the reference
     graph (files need NOT be contiguous for emit/ledger — only fresh
     clustering chose contiguity). Needs an experiments/030-\* with ground
     truth before production (precision over recall). NOT this branch.
- [x] 10. Validated on 2.1.89 (results-2026-07-16-human-layout.md):
      src top dirs 161->24, singletons 79->0, drawers 7->0, sub-20-line
      files 254->6, zero -N/minted dir names, depth 1-2. Vendor: 25->~760
      named, 99 package folders (grpc/protobufjs/prismjs/…),
      is-plain-object hallucination guarded. NOTE: measured via two
      scratchpad harnesses (split-fresh.mts on humanified.js for src/;
      vendor-name-run.mts on the original bundle for vendor/) — a single
      full `humanify --split` end-to-end regen is the remaining manual
      confirmation before the backward/forward version walk. zod/Bedrock
      in-src is item 9 (deferred), not fixed here.

## Resume instructions

- Repo /Users/andrewgross/Development/humanify, branch feat/human-layout.
- Red/green TDD per CLAUDE.md: failing test first, then implement.
- `npm run check` before every commit; pre-commit biome is stricter on
  cognitive complexity than npm run check — `npx biome check <file>` first.
- Unit tests colocated (\*.test.ts). Cluster tests: src/split/cluster-assign.test.ts.
- Local LLM for any live naming runs: http://192.168.1.234:8000/v1,
  model openai/gpt-oss-20b (reference_local_llm memory).
- Tick items here as they complete, one commit per item (or small group).
- Item 10's compare script: experiments-style; put tree-stats helper under
  scripts/ or experiments/, not src/.
- A session-only cron (every 2h) re-enqueues this plan if the session stalls;
  delete it (CronDelete) when all items are done.
