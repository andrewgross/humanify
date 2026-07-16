# Plan 2026-07-16 (evening): 4-tier naming/grouping quality on feat/human-layout

Follow-on to the human-layout branch. Goal: the ANCHOR fresh-split tree's
file/folder names read like a human wrote them. Motivated by the
validation-tree comparison vs humanify/src (the real hand-written repo):
generated names are 74% agent-noun (-er/-or/Factory/...), camelCase-only,
avg 2.6 words, folders have And-conjunctions / verb-phrases / Group-Suite
decorations, minted leftovers (appInitializer17). Diagnosis + numbers in
the 2026-07-16 conversation and results-2026-07-16-human-layout.md.

CONSTRAINTS (user, 2026-07-16):

- Kebab-case for src/ app names (NOT vendor package names — those are real
  npm names). Node/TS convention: kebab is the FS-safe neutral default.
- NO backwards compat with prior RUNS (regenerate freely). The ONE contract
  to preserve: --prior-version inheritance (assignWithPrior + ledger format
  UNTOUCHED). All tiers change only fresh-anchor grouping/naming.
- New code in later versions is placed by locality (assignWithPrior), never
  re-named; tree frozen at anchor -> anchor naming is a one-time investment.

## Tiers

- [x] TIER 1 (commit 5103c33) — deterministic surface fixes (cluster-assign.ts, stable-split.ts):
      (a) kebab-case the FINAL src/ file+folder path segments (after all
      camelCase-token merge/collapse logic; dedup on the kebab form).
      Vendor path untouched.
      (b) reject minted stems: extend BAD_STEM so a stem with a 2+-digit
      disambiguator run (appInitializer17, app254Initializer) that is
      NOT a known unit token (8/16/32/64/128/256/512/1024) is rejected;
      segmentStem falls through.
      (c) grammar validation: reject leading conjunction/article
      (and/or/but/nor/the/a/an + Capital) in acceptProposedName (files
      AND folders). FOLDER-only: reject leading verb
      (get/set/build/filter/handle/create/make/render/process/register/
      add/remove/update/fetch/load/parse + Capital) and decoration
      suffix (Group/Suite/Engine/Manager/Hub) — a folder is a noun.
      (d) length: prefer <=3 tokens; prompt asks for it, validation trims/-
      rejects >4-token folder names.
- [x] TIER 2 (commit 66a18fb) — name from behavior not labels (split-namer.ts, cluster-assign.ts):
      pass a code-evidence field on SplitNameRequest (distinctive strings,
      imports/API calls, top function signature — reuse code-window.ts /
      slice from the `code` already threaded into assignClustered). Prompt
      names the CONCEPT from evidence instead of echoing agent-noun labels.
      Good/bad examples in the system prompt (nouns; no and/verb/Group).
- [x] TIER 3 (commit 1ef24a9, dual-window not full community-clustering) — cohesive folders (cluster-assign.ts): keep seam-based FILE
      formation, but replace size-based folder WALLS (pickWalls/groupSegments)
      with reference-community clustering of the FILES (file A->B when A refs
      a binding declared in B; agglomerative/modularity — reuse
      reference-cluster.ts infra). Files need not be contiguous for emit
      (ledger is per-statement); cross-version inheritance unaffected.
      MEASURE with the split-quality metric (experiments/029). Deterministic.
- [x] TIER 4 (commit 99831ad) — holistic revision pass (split-namer.ts): after the tree is
      named, one LLM call per level showing the WHOLE sibling set (folder ->
      its file list) to revise for parallelism / dedupe themes / fix
      outliers. One extra call per level; anchor-only so cost is one-time.
- [x] REGEN + COMPARE (DONE, results doc updated): full `humanify --split` on 2.1.89
      → validation/claude-code-2.1.89-tiers. Files agent-noun 74->38%,
      kebab 6->1537 (100%), minted digit 32->18; folders camelCase
      103->0. Boots (node run.cjs --version -> 2.1.89, exit 0). Residual:
      grab-bag folder NAMES from loudest file (needs deferred community
      clustering). ALL 4 TIERS COMPLETE.
      against humanify/src (target: agent-noun share down, kebab, avg words
      down, 0 And/verb/decoration folders, 0 minted names, folders cohesive).
      Boot test `node run.cjs --version`. Update results doc.

## Resume / how-to

- Repo /Users/andrewgross/Development/humanify, branch feat/human-layout.
- Red/green TDD; `npm run check` before commit; `npx biome check <file>`
  first (pre-commit biome stricter on complexity).
- Split naming code: cluster-assign.ts (grouping+name assembly),
  stable-split.ts (acceptProposedName/BAD_STEM/segmentStem), split-namer.ts
  (LLM prompt). Vendor naming is SEPARATE (unpack/vendor-namer.ts) — do NOT
  kebab vendor names.
- Local LLM for regen: http://192.168.1.234:8000/v1 openai/gpt-oss-20b
  (reference_local_llm). Full run cmd in results doc / validation/ scripts.
- Validation harness: unpacked-claude-code/validation/analyze-tree.mts.
- Tick tiers here as done; one commit per tier (or sub-part).
