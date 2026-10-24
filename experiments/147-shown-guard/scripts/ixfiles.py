"""exp147: index file paths for the needed seed hashes + every indexed file of the needed owners (per slice)."""
import json, collections, sys
K = json.load(open('/work/exp147-data/kepthash.json'))
IX = '/work/exp142-data/full/ix/k2-all-r5-origin-prod-asof20261015/'
out = {}
for sl in ('esbuild-node-cjs-min', 'bun-node-cjs-min'):
    hs = {k['hash'] for k in K if k['slice'] == sl and k['hash']}
    owners = {k['fam'] for k in K if k['slice'] == sl}
    rows = collections.defaultdict(list); files = collections.defaultdict(set)
    with open(IX + f'units.{sl}.jsonl') as fh:
        for line in fh:
            h = line[9:25]
            i = line.index('"owner":"') + 9; j = line.index('"', i); ow = line[i:j]
            if h in hs or ow in owners:
                r = json.loads(line)
                if ow in owners: files[ow].add(r['file'])
                if h in hs: rows[h].append([ow, r['file']])
    out[sl] = {"rows": rows, "files": {o: sorted(v) for o, v in files.items()}}
    print(sl, len(hs), len(rows), len(files), flush=True)
json.dump(out, open('/work/exp147-data/ixfiles.json', 'w'))
