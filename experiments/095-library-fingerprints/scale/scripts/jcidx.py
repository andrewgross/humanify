"""n-gram (B5) index entries: one per (pkg, version, file) and bundler family = the largest complete instance
(parts of one build merged) among the ix req/dyn builds; minpos = smallest root position containing the triple.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/jcidx.py
-> data/jidx-<fam>.npz : pkg (int32), minpos (int32), ptr (int64), data (uint64 sorted unique per entry), keys (json)
"""
import json, collections
import numpy as np
from lib import *

names, pid = pkg_ids()
json.dump(names, open(D + "pkgs-jidx.json", "w"))  # the id table the jidx/jc files use
z = np.load(D + "b5.npz")
bkeys = json.loads(str(z["keys"]))
bptr, bdata = z["ptr"], z["data"]
shas = json.load(open(D + "shas.json"))
brow = {k: i for i, k in enumerate(bkeys)}
for fam in ("bun", "esbuild"):
    groups = collections.defaultdict(list)
    minpos = {}
    for s in index_rows(fam):
        t = (s["pkg"], s["ver"], s["file"])
        groups[(t, s["build"])].append(brow[s["sha"]])
        minpos[t] = min(minpos.get(t, 10**9), s["pos"] if s["pos"] is not None else 10**9)
    best = {}
    for (t, b), rows in groups.items():
        n = sum(int(bptr[r + 1] - bptr[r]) for r in rows)
        if t not in best or n > best[t][0]:
            best[t] = (n, rows)
    keys = sorted(best)
    arrs = []
    for t in keys:
        rows = best[t][1]
        a = np.unique(np.concatenate([bdata[bptr[r]:bptr[r + 1]] for r in rows])) if len(rows) > 1 else np.asarray(bdata[bptr[rows[0]]:bptr[rows[0] + 1]])
        arrs.append(a)
    ptr = np.zeros(len(arrs) + 1, np.int64)
    np.cumsum([len(a) for a in arrs], out=ptr[1:])
    np.savez(D + f"jidx-{fam}.npz", pkg=np.array([pid[t[0]] for t in keys], np.int32), minpos=np.array([minpos[t] for t in keys], np.int32),
             ptr=ptr, data=np.concatenate(arrs), keys=np.array(json.dumps(keys)))
    print(fam, "entries", len(keys), "shingles", int(ptr[-1]), flush=True)
