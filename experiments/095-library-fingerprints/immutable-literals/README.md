# exp095 / immutable-literals

Can the things a minifier never renames (strings, property names, numbers, regexes) recognise npm library code in
a minified bundle? The write-up was returned to the orchestrating workflow (writing REPORT.md was blocked); the numbers are in `logs/` and `results/`. This file is how to re-run it.

Inputs (read-only): the shared lab `../lab/` (labels, builds, packages, Claude Code real-world set), the Claude Code
raw bundles under `/Users/andrewgross/Development/claude-code-versions/inputs/`, and the humanified vendor manifests
under `/tmp/eval-work/candidate-e214db7a-scratch/<v>/vendor/_bun-modules.json`. Nothing in the humanify repo is touched.

## Layout

```
scripts/lits.mjs            literal extraction (S: string, P: property name, N: number, R: regex); "source" mode for npm files
scripts/extract-all.mjs     -> data/sources.jsonl (lab packages), data/sources-plus.jsonl (+ pkgs-cc), data/spans.jsonl
                               (every distinct lab span text), data/cc.jsonl (Claude Code sets, layout units, top-level statements)
scripts/extract-stmts.mjs   -> data/stmts.jsonl + data/stmt-lits.jsonl (every top-level statement of every minified lab build)
scripts/common.py           the index: package-level IDF weights, VARIANT switches (plus index, capped vocabulary)
scripts/score.py            scores lab spans + Claude Code queries   -> results/scores-lab<V>.jsonl, scores-cc<V>.jsonl
scripts/score_stmts.py      scores lab statements                    -> results/scores-stmts<V>.jsonl
scripts/recognise.py        claim rules (plain margin / origin tie-break) + threshold grid -> results/recognise-grid*.json
scripts/breakdown.py        recall by bundler, kind, size            -> results/breakdown-*.json
scripts/lpo.py              leave-package-out claims, copy vs look-alike -> results/lpo-*.json
scripts/fill.py, stepb.py   Step B: statement claims, fill, consistent fill; lab + Claude Code negatives -> results/stepb*.json
scripts/cc_scan.py          Claude Code: every factory / lazy group / statement -> results/cc-scan*.json, cc-claims-*.jsonl
scripts/cc_msal.py          recall on Claude Code MSAL modules (banner ground truth) -> results/cc-msal*.json
scripts/stability.py        literal survival vs the npm source       -> results/stability.json
scripts/versions_size.py    version identification, drift, index size -> results/versions.json, results/index-size.json
scripts/extra.py            cross-bundler literal-set identity, exact set hashing, deduped size -> results/extra.json
scripts/anchors.py          rare-literal anchor rule                 -> results/anchors.json
scripts/census.mjs          Part A detectors (declared identity)     -> results/census-hits-{sources,cc}.jsonl
scripts/census_report.py    Part A tables                            -> results/census.json, logs/census.txt
pkgs-cc/                    Claude Code-era package versions (npm install, --ignore-scripts)
tools/venv/                 python venv with numpy + scipy
logs/                       printed tables of every run (the numbers in REPORT.md come from these)
```

Variants (`VARIANT=`): `""` lab index, plain IDF; `-capvocab` identifier-like literals capped at 2 bits;
`-plus` index + `pkgs-cc`; `-plus-capvocab` both. Claim rule (`CLAIM=`): `plain` or `origin` (near-ties broken by the
smallest matching source file). Recommended: `VARIANT=-plus-capvocab CLAIM=origin`.

## Re-run everything (about 10 minutes on 64 cores)

```bash
cd /work/exp095-library-fingerprints/immutable-literals
python3 -m venv tools/venv && tools/venv/bin/pip install -q numpy scipy
(mkdir -p pkgs-cc && cd pkgs-cc && echo '{"private":true}' > package.json && npm install --ignore-scripts --no-audit --no-fund \
  @azure/msal-node@3.8.1 @aws-sdk/client-bedrock-runtime@3.936.0 @anthropic-ai/sdk@0.94.0 chokidar@4.0.3 \
  google-auth-library@10.6.2 @grpc/grpc-js@1.14.4 openid-client@5.7.1)
node --max-old-space-size=16000 scripts/extract-all.mjs --jobs 60     # ~30 s
node --max-old-space-size=16000 scripts/extract-stmts.mjs --jobs 60   # ~40 s
for V in "" -capvocab -plus -plus-capvocab; do VARIANT=$V ./run-scoring.sh & done; wait          # ~2 min
for V in "" -capvocab -plus -plus-capvocab; do for C in plain origin; do VARIANT=$V CLAIM=$C ./run-eval.sh & done; done; wait
tools/venv/bin/python scripts/extra.py
tools/venv/bin/python scripts/anchors.py
node scripts/census.mjs --jobs 60 && python3 scripts/census_report.py > logs/census.txt
```

Each step overwrites its outputs, so any step can be re-run alone once its inputs exist.
