"""Structural index tables (built once, filtered per scale at query time).
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/structidx.py
-> data/uidx-<fam>.npz   unit table: h (uint64), pkg (int32), minpos (int32), nodes, content, typ   unique (h, pkg)
-> data/widx-<fam>.npz   module table: M (uint64 normM), V (uint64 normV), pkg, minpos, normLen   unique (M, V, pkg)
-> data/veto-<fam>.npz   unit / module hashes seen in the bundler's runtime, unmapped glue or app stubs (never votes)
-> data/pkgs.json        package id table
Index content per family: every ix library span (purity >= 0.9 or unmapped-free), package NOT held out.
minpos = the smallest root position (download rank order among roots) whose builds contain it -> scale membership.
"""
import json, pickle, os
import numpy as np
from lib import *

names, pid = pkg_ids()
json.dump(names, open(D + "pkgs.json", "w"))
U = units()
F = pickle.load(open(D + "fph.pkl", "rb")) if os.path.exists(D + "fph.pkl") else None
import os
shas = json.load(open(D + "shas.json"))


def h16(x):
    return int(x, 16) if x else 0


for fam in ("bun", "esbuild"):
    # (sid, pkg) -> minpos
    sp = {}
    veto_sids = set()
    for s in spans():
        if s["corpus"] != "ix" or s["fam"] != fam:
            continue
        if s["pkg"] in ("<runtime>", "<unmapped>", "<app>", "<bundler-helper>"):
            veto_sids.add(s["sid"])
            continue
    for s in index_rows(fam):
        k = (s["sid"], pid[s["pkg"]])
        sp[k] = min(sp.get(k, 10**9), s["pos"] if s["pos"] is not None else 10**9)
    E = np.array([(a, b, c) for (a, b), c in sp.items()], dtype=np.int64)
    E = E[np.argsort(E[:, 0], kind="stable")]
    esid, epkg, epos = E[:, 0], E[:, 1], E[:, 2]
    # units of index sids (non-root)
    m = (U["typ"] != 0) & np.isin(U["sid"], esid)
    us, uh, un, uc, ut = U["sid"][m], U["h"][m], U["nodes"][m], U["content"][m], U["typ"][m]
    lo = np.searchsorted(esid, us, "left")
    hi = np.searchsorted(esid, us, "right")
    cnt = hi - lo
    rep = np.repeat(np.arange(len(us)), cnt)
    # vectorised expansion of [lo, hi) ranges
    starts = np.repeat(lo, cnt)
    within = np.arange(len(rep)) - np.repeat(np.cumsum(cnt) - cnt, cnt)
    eix = starts + within
    T = np.empty(len(rep), dtype=[("h", "u8"), ("pkg", "i4"), ("pos", "i4"), ("nodes", "i4"), ("content", "i4"), ("typ", "i1"), ("mb", "i8")])
    T["h"], T["pkg"], T["pos"] = uh[rep], epkg[eix], epos[eix]
    T["nodes"], T["content"], T["typ"] = un[rep], uc[rep], ut[rep]
    T["mb"] = U["bytes"][us[rep]]  # bytes of the module span holding this unit occurrence
    T.sort(order=["h", "pkg", "pos", "mb"])
    keep = np.ones(len(T), bool)
    keep[1:] = (T["h"][1:] != T["h"][:-1]) | (T["pkg"][1:] != T["pkg"][:-1])
    mbmin = np.minimum.reduceat(T["mb"], np.nonzero(keep)[0])  # smallest module holding (h, pkg)
    T = T[keep]
    T["mb"] = mbmin
    np.savez(D + f"uidx-{fam}.npz", h=T["h"], pkg=T["pkg"], minpos=T["pos"], nodes=T["nodes"], content=T["content"], typ=T["typ"], mbytes=T["mb"])
    # veto units: runtime / glue / app stubs of the index builds
    vm = (U["typ"] != 0) & np.isin(U["sid"], np.array(sorted(veto_sids)))
    vh = np.unique(U["h"][vm])
    if F is None:
        print(fam, "unit table only (no fph.pkl yet)", len(T), flush=True); continue
    # module table
    rows = []
    for (sid, p), pos in sp.items():
        f = F.get(shas[sid])
        if not f or not f[0]:
            continue
        rows.append((h16(f[0]), h16(f[1]), p, pos, f[2]))
    W = np.array(rows, dtype=[("M", "u8"), ("V", "u8"), ("pkg", "i4"), ("pos", "i4"), ("len", "i4")])
    W.sort(order=["M", "V", "pkg", "pos"])
    keep = np.ones(len(W), bool)
    keep[1:] = (W["M"][1:] != W["M"][:-1]) | (W["V"][1:] != W["V"][:-1]) | (W["pkg"][1:] != W["pkg"][:-1])
    W = W[keep]
    np.savez(D + f"widx-{fam}.npz", M=W["M"], V=W["V"], pkg=W["pkg"], minpos=W["pos"], normLen=W["len"])
    vM = np.unique(np.array([h16(F[shas[s]][0]) for s in veto_sids if shas[s] in F and F[shas[s]][0]], dtype=np.uint64))
    np.savez(D + f"veto-{fam}.npz", units=vh, modules=vM)
    print(fam, "index spans", len(sp), "unit rows", len(T), "distinct unit hashes", len(np.unique(T["h"])), "module rows", len(W), "veto units", len(vh), flush=True)
