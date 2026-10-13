"""fphash.jsonl -> data/fph.pkl {sha: (normM, normV, normLen, coreLen, factM, coreM, coreV)}"""
import orjson, pickle
S = "/work/exp095-library-fingerprints/scale/data/"
out, bad = {}, 0
for l in open(S + "fphash.jsonl", "rb"):
    r = orjson.loads(l)
    if not r.get("ok", True) or "normM" not in r:
        bad += 1
        continue
    out[r["id"]] = (r.get("normM"), r.get("normV"), r.get("normLen", 0), r.get("coreLen", 0), r.get("factM"), r.get("coreM"), r.get("coreV"))
pickle.dump(out, open(S + "fph.pkl", "wb"))
print("fph", len(out), "failed", bad)
