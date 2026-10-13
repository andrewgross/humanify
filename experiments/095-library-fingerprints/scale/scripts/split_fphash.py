"""Split data/fphash-jobs.jsonl into 10 chunks sorted by bundle (bounds fphash's bundle-name memory); spans > 1 MB
go to fphash-jobs.big.jsonl (run separately; not merged into the reported results)."""
import json, os
D = "/work/exp095-library-fingerprints/scale/data/"
rows = sorted(open(D + "fphash-jobs.jsonl"), key=lambda l: json.loads(l).get("bundle", ""))
small, big = [], []
for l in rows:
    j = json.loads(l)
    sz = os.path.getsize(j["path"]) if "path" in j else len(j.get("text", ""))
    (big if sz > 1_000_000 else small).append(l)
k = (len(small) + 9) // 10
for i in range(10):
    open(D + f"fphash-jobs.{i}.jsonl", "w").writelines(small[i * k:(i + 1) * k])
open(D + "fphash-jobs.big.jsonl", "w").writelines(big)
print(len(small), len(big))
