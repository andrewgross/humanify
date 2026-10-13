# exact-module-hash / verify — adversarial re-check (commands only)

All scripts run from `/work/exp095-library-fingerprints/exact-module-hash`. They reuse the agent's `fphash` binary and `data/`.

```bash
cd /work/exp095-library-fingerprints/exact-module-hash
# fresh hold-out sample: 400 npm top-download packages (rank 150-3000) NOT in the lab, Bun 1.4.0 + 1.3.14 --minify
(cd verify/fresh && node pick.mjs && ./install.sh && node scripts/build.mjs --bundlers bun-1.4.0,bun-1.3.14 --jobs 48 \
  && node scripts/label.mjs --jobs 48 && cat builds/*/min/*/*/labels.jsonl > labels-all.jsonl)
python3 - <<'PY'   # jobs for fphash
import json
F='/work/exp095-library-fingerprints/exact-module-hash/verify/fresh/'
with open(F+'jobs.jsonl','w') as f:
    for i,l in enumerate(open(F+'labels-all.jsonl')):
        r=json.loads(l)
        if r.get('textPath'): f.write(json.dumps({'id':i,'path':F+r['textPath'],'kind':r['kind'],'bundle':F+'builds/'+r['build']+'/out.js'})+'\n')
PY
fphash/target/release/fphash < verify/fresh/jobs.jsonl > verify/fresh/hash.jsonl
python3 verify/v1_negsizes.py            # sizes of the negative sets
python3 verify/v2_cc_internal.py         # shape collisions inside Claude Code itself
python3 verify/v3_fresh_fp.py            # unseen-package code vs the lab index (wrong-package rate)
python3 verify/v4_pool_collisions.py > verify/v4_pool_collisions.txt   # collisions when the index grows 175 -> 1,221 packages
python3 verify/v5_collision_texts.py     # texts of every >=400 cross-package collision group
python3 verify/v6_cc_pooled.py           # Claude Code against the pooled index
python3 verify/v7_fresh_stability.py     # normaliser hold-out: Bun 1.3.14 vs 1.4.0 on fresh packages
(cd verify/hljs && ./build.sh 11.10.0 11.11.0 11.11.1 11.11.2 11.12.0 && for d in v*/; do v=${d%/}; \
  node ../../../lab/scripts/cc-layout.mjs $PWD/$v/out/entry.js $PWD/$v/layout.json >/dev/null; done; python3 recall.py)
```
