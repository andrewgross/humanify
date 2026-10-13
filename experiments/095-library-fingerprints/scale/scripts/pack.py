"""Pack features.jsonl (+ fphash.jsonl) into numpy arrays for the analysis.
usage: .venv/bin/python scripts/pack.py
-> data/units.npz   sid, typ (0 root 1 stmt 2 fn 3 class), nodes, ntok, content, h (uint64)
-> data/qlits.pkl   {sha: [literals]}   only for spans OUTSIDE the ix corpus (queries)
-> data/fph.pkl     {sha: (normM, normV, normLen, coreLen, factM, nStrings)}
-> data/shas.json   sid -> sha (tasks.jsonl order)
"""
import json, pickle, orjson
import numpy as np
S = "/work/exp095-library-fingerprints/scale/"
shas = [json.loads(l)["k"] for l in open(S + "data/tasks.jsonl")]
sid = {s: i for i, s in enumerate(shas)}
spans = pickle.load(open(S + "data/spans.pkl", "rb"))
qshas = {s["sha"] for s in spans if s["corpus"] != "ix"}
del spans
TY = {"r": 0, "s": 1, "f": 2, "c": 3}
cols = {k: [] for k in ("sid", "typ", "nodes", "ntok", "content", "h")}
qlits, bytes_, perr = {}, np.zeros(len(shas), np.int64), 0
nlit = np.zeros(len(shas), np.int32)
nstr = np.zeros(len(shas), np.int32)
with open(S + "data/features.jsonl", "rb") as f:
    for line in f:
        try:
            r = orjson.loads(line)
        except orjson.JSONDecodeError:
            r = json.loads(line.decode("utf8", "surrogatepass"))
        i = sid[r["k"]]
        bytes_[i] = r.get("bytes", 0)
        if "pe" in r:
            perr += 1
        u = r.get("u") or []
        if u:
            cols["sid"].append(np.full(len(u), i, np.int32))
            cols["typ"].append(np.array([TY[x[0]] for x in u], np.int8))
            cols["nodes"].append(np.array([x[1] for x in u], np.int32))
            cols["ntok"].append(np.array([x[2] for x in u], np.int32))
            cols["content"].append(np.array([x[3] for x in u], np.int32))
            cols["h"].append(np.array([int(x[4], 16) for x in u], np.uint64))
        L = r.get("l") or []
        nlit[i] = len(L)
        nstr[i] = sum(1 for x in L if x.startswith("S"))
        if r["k"] in qshas:
            qlits[r["k"]] = L
out = {k: np.concatenate(v) for k, v in cols.items()}
np.savez(S + "data/units.npz", bytes=bytes_, nlit=nlit, nstr=nstr, **out)
pickle.dump(qlits, open(S + "data/qlits.pkl", "wb"))
json.dump(shas, open(S + "data/shas.json", "w"))
print("units", len(out["h"]), "parse errors", perr, "query lit sets", len(qlits))
