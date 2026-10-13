# subtree-hashes/verify: adversarial re-check of the subtree-hash claims

Everything the verifier made. The findings are in `../VERIFY.md`. Uses `../scripts/canon.mjs`,
`../scripts/extract.mjs`, `../lib/*.mjs` unchanged (read-only), so the index is exactly the one the claims used
(lab + rwlab, Bun 1.4.0, minified).

```
flab/            a third lab: 82 FOREIGN npm packages (in neither the lab catalog nor rwlab), the 18 rwlab packages
                 at their latest version (for unseen Bun / esbuild versions), and unsampled releases around the
                 claimed CC versions (zod 3.25.71-76, @anthropic-ai/sdk 0.92-0.94, msal-node 3.8.0/1/3, ...)
tools/           bun 1.2.23, 1.3.5, 1.4.1 and esbuild 0.19.12 (never used to derive the normaliser)
data/            unit TSVs for flab (units-loose/builds/...), CC 2.1.86 / 2.1.119 layouts + units (cc/)
scripts/         foreign.mjs, foreign-fp.mjs, foreign-classify.py, unseen-bun.mjs, cc-app-hits-any.mjs,
                 cc-named.mjs, cc-suspects.mjs, version-check.mjs
results/         every number in VERIFY.md
```

## Re-run

```bash
cd /work/exp095-library-fingerprints/subtree-hashes/verify/flab
node scripts/pick.mjs > versions.json     # NOTE: live npm 'latest'; the versioncheck block was appended by hand (see versions.json)
scripts/install.sh && node scripts/build.mjs && node scripts/build-esbuild.mjs
node scripts/label.mjs --jobs 48 && find builds -name labels.jsonl | sort | xargs cat > labels/all.jsonl
cd .. && find flab/builds -name out.js | sed 's|/out.js||; s|^flab/||' > dirs.txt && cd ..
cat verify/dirs.txt | xargs -P 48 -n 4 env LABROOT=$PWD/verify/flab/ node --max-old-space-size=8000 scripts/extract.mjs loose verify/data/units-loose
cd verify
node --max-old-space-size=30000 scripts/foreign.mjs > results/foreign.txt
for m in 20 30 50; do node --max-old-space-size=30000 scripts/foreign-fp.mjs $m > results/foreign-fp-$m.jsonl; python3 scripts/foreign-classify.py $m > results/foreign-classes-$m.txt; done
node --max-old-space-size=30000 scripts/unseen-bun.mjs > results/unseen-bun.txt
for v in 2.1.86 2.1.119; do B=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-$v/binary-decompiled/src/entrypoints/index.js
  node /work/exp095-library-fingerprints/lab/scripts/cc-layout.mjs $B data/cc/layout-$v.json
  (cd .. && node scripts/extract-file.mjs loose verify/data/cc/bundle-$v.tsv $B verify/data/cc/layout-$v.json); done
for v in 2.1.86 2.1.119 2.1.198 2.1.216; do for m in 15 20 30; do node --max-old-space-size=20000 scripts/cc-app-hits-any.mjs $v $m > results/cc-app-hits-$v-min$m.txt; done; done
node --max-old-space-size=20000 scripts/cc-suspects.mjs 2.1.216 20 > results/cc-suspects-2.1.216.txt
node --max-old-space-size=20000 scripts/cc-named.mjs 2.1.216 20 tslib,@opentelemetry/api,asynckit,diff,marked,js-yaml > results/cc-named-helpers-2.1.216.txt
node --max-old-space-size=20000 scripts/version-check.mjs 2.1.216 > results/version-check-2.1.216.txt
```
