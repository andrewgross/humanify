"""exp143: mark each vetsim.py row with whether the BASE run claimed the unit anyway (by propagation from another
seed): an exemption changes only units the base left unclaimed. usage: python3 -I vetclaimed.py <vetsim.jsonl>
<out.jsonl> <corpus>=<label> ..."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def main():
    rows = [json.loads(line) for line in open(sys.argv[1])]
    by = collections.defaultdict(list)
    for r in rows:
        by[r["entry"]].append(r)
    for arg in sys.argv[3:]:
        corpus, label = arg.split("=", 1)
        lib096.use(corpus)
        run_dir = os.path.join(lib096.DATA, "results", label, "reports")
        for e in os.listdir(run_dir):
            eid = e[:-5]
            if eid not in by:
                continue
            rep = json.load(open(os.path.join(run_dir, e)))["libraryDiscovery"]
            starts = [(u["start"], u["end"]) for u in rep["unitRows"]]
            claimed = {(c["start"], c["end"]) for c in rep.get("claims", [])}
            for r in by[eid]:
                r["baseClaimed"] = starts[r["i"]] in claimed
    with open(sys.argv[2], "w") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")


if __name__ == "__main__":
    main()
