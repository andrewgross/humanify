"""Unit-hash voting (sub-tree hashes) at every scale.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/uv.py
Per query: its non-root units of >= nmin nodes and >= cmin content tokens (distinct strings / property names /
globals / regexes), each distinct hash once; a hash votes for its package when exactly ONE package (or one family,
famMerge) of the scale-R index holds it; stop-lists remove hashes first. Claim: votes >= k and >= 2x the runner-up.
-> data/uv-votes-<fam>-R<R>-n<nmin>-c<cmin>-<famMerge>-<stop>.pkl : {sha: [(pkg, votes, maxnodes), ...top 3]}
"""
import json, pickle, sys, collections
import numpy as np
from lib import *
from evalq import Q

names = json.load(open(D + "pkgs.json"))
U = units()
famid = {}
fam_of_pkg = np.array([famid.setdefault(family(n), len(famid)) for n in names], np.int32)
famname = {v: k for k, v in famid.items()}
stops = json.load(open(D + "stoplists.json")) if os.path.exists(D + "stoplists.json") else {}


def votes(fam, R, nmin, cmin, fm, stop):
    T = np.load(D + f"uidx-{fam}.npz")
    m = (T["minpos"] <= R) & (T["nodes"] >= nmin) & (T["content"] >= cmin)
    th, tp = T["h"][m], T["pkg"][m]
    key = fam_of_pkg[tp] if fm else tp
    # per hash: number of distinct owners (package or family)
    o = np.lexsort((key, th))
    th, key, tp = th[o], key[o], tp[o]
    uh, start, cnt = np.unique(th, return_index=True, return_counts=True)
    # owners distinct within a hash
    newown = np.ones(len(th), bool); newown[1:] = (th[1:] != th[:-1]) | (key[1:] != key[:-1])
    nown = np.add.reduceat(newown.astype(np.int32), start)
    single = nown == 1
    sh, sown = uh[single], key[start[single]]
    # with family merge, the NAME is the family member holding most of the voting hashes: keep one holder per hash
    sname = tp[start[single]]
    smb = np.minimum.reduceat(T["mbytes"][m][o], start)[single]  # smallest index module holding the hash
    stoph = set()
    spec = False
    if stop:
        for name in stop.split("+"):
            if name == "spec":
                spec = True
                continue
            stoph |= set(int(x) for x in stops.get(f"{fam}:{name}", []))
        if stoph:
            keep = ~np.isin(sh, np.array(sorted(stoph), np.uint64))
            sh, sown, smb, sname = sh[keep], sown[keep], smb[keep], sname[keep]
    qs = sorted({q["sha"] for q in Q() if q["fam"] == fam})
    sids = {s: i for i, s in enumerate(json.load(open(D + "shas.json")))}
    qsid = np.array([sids[s] for s in qs])
    um = (U["typ"] != 0) & (U["nodes"] >= nmin) & (U["content"] >= cmin) & np.isin(U["sid"], qsid)
    us, uh2, un = U["sid"][um], U["h"][um], U["nodes"][um]
    k = np.searchsorted(sh, uh2); k[k >= len(sh)] = 0
    hit = sh[k] == uh2
    if spec:  # specificity: the hash must sit in an index module no more than 4x the query module (+4 KB)
        hit &= smb[k] <= 4 * U["bytes"][us] + 4096
    us, uh2, un, own, nm = us[hit], uh2[hit], un[hit], sown[k[hit]], sname[k[hit]]
    # one vote per distinct (sid, hash)
    o = np.lexsort((uh2, us)); us, uh2, un, own, nm = us[o], uh2[o], un[o], own[o], nm[o]
    f = np.ones(len(us), bool); f[1:] = (us[1:] != us[:-1]) | (uh2[1:] != uh2[:-1])
    us, un, own, nm = us[f], un[f], own[f], nm[f]
    res = collections.defaultdict(dict)
    mem = collections.defaultdict(collections.Counter)
    for s, n, w, p in zip(us.tolist(), un.tolist(), own.tolist(), nm.tolist()):
        v = res[s].get(w)
        res[s][w] = (1, n) if v is None else (v[0] + 1, max(v[1], n))
        mem[(s, w)][p] += 1
    shas = json.load(open(D + "shas.json"))
    out = {}
    for s, d in res.items():
        top = sorted(d.items(), key=lambda x: -x[1][0])[:3]
        out[shas[s]] = [((names[mem[(s, w)].most_common(1)[0][0]] if fm else names[w]), v, mx) for w, (v, mx) in top]
    return out


def claim(top, k, margin=2.0, big=None):
    if not top:
        return None
    p, v, mx = top[0]
    v2 = top[1][1] if len(top) > 1 else 0
    if (v >= k or (big and mx >= big)) and v >= margin * v2:
        return p
    return None


if __name__ == "__main__":
    cfgs = sys.argv[1].split(";") if len(sys.argv) > 1 else ["20,0,0,", "30,3,0,", "30,3,1,", "30,3,1,helpers+runtime+stainless+shim", "30,3,1,helpers+runtime+stainless+shim+spec", "30,3,0,helpers+runtime+stainless+shim+spec"]
    for fam in ("bun", "esbuild"):
        for R in SCALES:
            for c in cfgs:
                nmin, cmin, fm, stop = c.split(",")
                v = votes(fam, R, int(nmin), int(cmin), fm == "1", stop)
                pickle.dump(v, open(D + f"uv-votes-{fam}-R{R}-n{nmin}-c{cmin}-{fm}-{stop or 'none'}.pkl", "wb"))
                print(fam, R, c, "queries with votes", len(v), flush=True)
