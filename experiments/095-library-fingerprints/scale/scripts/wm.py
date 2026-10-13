"""Whole-module exact hashes (normalised, humanify MatchKey via fphash): collision rates and claims per scale.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/wm.py
Collision table (bun slice): share of distinct normM hashes held by 2+ packages, per normLen band, raw / after the
exact-string check (normV must also agree) / after family merge.  -> results/wm-collisions.json
Claims: normLen >= L; the query's normM must be held by exactly ONE package (one family with fam merge) in the
scale-R index; normV must equal one of that package's normV for the hash (exact strings); refuse stringless cores
(no string literal in the span) and stop-listed module hashes (runtime, helpers, Stainless template, shim template).
-> data/wm-claims.pkl {(fam, R, L, fm, stop): {sha: pkg}}
"""
import json, pickle, collections
import numpy as np
from lib import *
from evalq import Q

names = json.load(open(D + "pkgs.json"))
F = fph()
U = units()
sid = {s: i for i, s in enumerate(json.load(open(D + "shas.json")))}
stops = json.load(open(D + "stoplists.json"))
famid = {}
fam_of = np.array([famid.setdefault(family(n), len(famid)) for n in names], np.int32)
famname = {v: k for k, v in famid.items()}
BANDS = [(0, 200), (200, 400), (400, 600), (600, 800), (800, 1600), (1600, 10**9)]
coll = []
claims = {}
for fam in ("bun", "esbuild"):
    W = np.load(D + f"widx-{fam}.npz")
    stopM = {}
    for nm in ("runtime", "helpers", "stainless", "shim"):
        stopM[nm] = {int(x, 16) for x in stops.get(f"{fam}:{nm}:M", [])}
    allstop = set().union(*stopM.values())
    for R in SCALES:
        m = W["minpos"] <= R
        M, V, P, L = W["M"][m], W["V"][m], W["pkg"][m], W["normLen"][m]
        byM = collections.defaultdict(lambda: collections.defaultdict(set))
        lenM = {}
        for h, v, p, l in zip(M.tolist(), V.tolist(), P.tolist(), L.tolist()):
            byM[h][p].add(v)
            lenM[h] = l
        if fam == "bun":
            for lo, hi in BANDS:
                hs = [h for h in byM if lo <= lenM[h] < hi]
                multi = [h for h in hs if len(byM[h]) > 1]
                # exact strings: 2+ packages share the same normV too
                multiV = [h for h in multi if any(len([p for p in byM[h] if v in byM[h][p]]) > 1 for v in set().union(*byM[h].values()))]
                multiF = [h for h in multi if len({fam_of[p] for p in byM[h]}) > 1]
                coll.append(dict(R=R, band=f"{lo}-{hi if hi < 10**9 else 'inf'}", hashes=len(hs), multiPkg=len(multi), multiPkgPct=pct(len(multi), len(hs)),
                                 sameStringsToo=len(multiV), multiFamily=len(multiF), multiFamilyPct=pct(len(multiF), len(hs))))
                print(json.dumps(coll[-1]), flush=True)
        qs = {q["sha"] for q in Q() if q["fam"] == fam}
        for Lmin in (200, 400, 800):
            for fm in (0, 1):
                for stop in ("", "all"):
                    out = {}
                    for s in qs:
                        f = F.get(s)
                        if not f or not f[0] or f[2] < Lmin:
                            continue
                        h, v = int(f[0], 16), int(f[1], 16)
                        e = byM.get(h)
                        if not e:
                            continue
                        if stop and h in allstop:
                            continue
                        if U["nstr"][sid[s]] == 0:
                            continue  # stringless core: "the strings agree" would be automatic
                        owners = {fam_of[p] for p in e} if fm else set(e)
                        if len(owners) != 1:
                            continue
                        hold = [p for p in e if v in e[p]]
                        if not hold:
                            continue
                        out[s] = names[hold[0]]  # a real member package names the claim (families only merge owners)
                    claims[(fam, R, Lmin, fm, stop)] = out
                    print(fam, R, Lmin, fm, stop or "-", "claims", len(out), flush=True)
json.dump(coll, open(S + "results/wm-collisions.json", "w"), indent=0)
pickle.dump(claims, open(D + "wm-claims.pkl", "wb"))
