#!/bin/bash
# Re-run every step of the exact-module-hash study. ~6 min on 64 cores. Needs the lab (../lab) built.
set -e
cd /work/exp095-library-fingerprints/exact-module-hash
export PATH="$HOME/.cargo/bin:$PATH"
(cd fphash && cargo build --release)
# 1. hash every lab span (321,619) with humanify's own hash code + the normaliser
python3 - <<'PY'
import json
with open('data/lab-jobs.jsonl', 'w') as f:
    for i, l in enumerate(open('/work/exp095-library-fingerprints/lab/labels/all.jsonl')):
        r = json.loads(l)
        if r.get('textPath'):
            f.write(json.dumps({'id': i, 'path': '/work/exp095-library-fingerprints/lab/' + r['textPath'], 'kind': r['kind'],
                                'bundle': '/work/exp095-library-fingerprints/lab/builds/' + r['build'] + '/out.js'}) + '\n')
PY
fphash/target/release/fphash < data/lab-jobs.jsonl > data/lab-hash.jsonl
python3 scripts/srcsha.py
# 2. Claude Code modules, negatives, positives
python3 scripts/cc_jobs.py
for v in 2.1.198 2.1.216; do fphash/target/release/fphash < data/cc-$v-jobs.jsonl > data/cc-$v-hash.jsonl; done
python3 - <<'PY'
import json, hashlib
for v in ['2.1.198', '2.1.216']:
    jobs = {json.loads(l)['id']: json.loads(l)['path'] for l in open(f'data/cc-{v}-jobs.jsonl')}
    rows = [json.loads(l) for l in open(f'data/cc-{v}-hash.jsonl')]
    with open(f'data/cc-{v}-hash.jsonl', 'w') as f:
        for r in rows:
            r['sha'] = hashlib.sha256(open(jobs[r['id']], 'rb').read()).hexdigest()[:16]
            f.write(json.dumps(r) + '\n')
PY
# 3. analyses
python3 scripts/stability.py > results/stability.txt
python3 scripts/uniq.py > results/uniq.txt
python3 scripts/drift.py > results/drift.txt
python3 scripts/indexsize.py > results/indexsize.txt
python3 scripts/eager_parts.py > results/eager_parts.txt
python3 scripts/cc_match.py > results/cc_match.txt
python3 scripts/combo_holdout.py > results/combo_holdout.txt
# 4. highlight.js recall at exact versions (builds 11 versions with Bun 1.4.0)
(cd hljs && ./build.sh 10.7.3 11.0.1 11.3.1 11.5.1 11.6.0 11.7.0 11.8.0 11.9.0 11.10.0 11.11.1 11.12.0 \
  && for d in v*/; do v=${d%/}; node ../../lab/scripts/cc-layout.mjs $PWD/$v/out/entry.js $PWD/$v/layout.json >/dev/null; done \
  && python3 hljs_recall.py > ../results/hljs_recall.txt)
# 5. realistic (label-free) segmentation of the combo bundles
for app in combo-1@0.0.0 combo-2@0.0.0 combo-3@0.0.0 combo-4@0.0.0 combo-5@0.0.0 combo-6@0.0.0; do
  for cfg in bun-1.4.0/min esbuild-0.28.2/min; do
    node ../lab/scripts/cc-layout.mjs ../lab/builds/$cfg/$app/main/out.js seg/$(echo $cfg | tr / _)-$app.json >/dev/null 2>&1 || true
  done
done
python3 scripts/realistic_seg.py > results/realistic_seg.txt
# 6. index-size extrapolation (npm registry metadata, network)
(cd npmtop && ./fetch.sh && mkdir -p metalab && cat lab-pkgs.txt | xargs -P 32 -I{} sh -c 'f="metalab/$(echo {} | sed "s#/#__#").json"; [ -s "$f" ] || curl -s -H "Accept: application/vnd.npm.install-v1+json" "https://registry.npmjs.org/$(echo {} | sed "s#/#%2f#")" -o "$f"')
python3 scripts/extrapolate.py > results/extrapolate.txt
