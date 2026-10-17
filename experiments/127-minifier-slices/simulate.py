"""exp127: `terser_slices = "auto"` simulated EXACTLY from per-bundle cards of a DEV corpus. Auto reads the union
for a bundle iff its terser-only share >= tau (the share is decided before any lookup, so a bundle's auto output IS
its union or its off output). Shares come from a probe label (auto at share 1.0, which records each bundle's
share in recipe.sliceReason). Refuses the frozen test splits.
  python3 -I experiments/127-minifier-slices/simulate.py <corpus> <off label> <union label> <probe label> tau...
"""
import collections
import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RES = os.path.join(HERE, "..", "096-library-matching-score", "results")
DATA = {"dev": "/work/exp096-data", "apps": None, "bundlers": None, "pop": None, "remin": "/work/exp126-data/remin/frozen"}


def data_dir(corpus):
    sys.path.insert(0, os.path.join(HERE, "..", "096-library-matching-score"))
    import lib096
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: the frozen test splits are never diagnosed")
    return lib096.CORPORA[corpus]["data"]


def shares(corpus, probe):
    out = {}
    for f in glob.glob(os.path.join(data_dir(corpus), "results", probe, "reports", "*.json")):
        if f.endswith(".run.json"):
            continue
        r = json.load(open(f))["recipe"]["sliceReason"] or ""
        m = re.search(r"share ([0-9.]+)", r)
        out[os.path.basename(f)[:-5]] = float(m.group(1)) if m else 0.0
    return out


def cards(label):
    return {os.path.basename(f)[:-5]: json.load(open(f)) for f in glob.glob(os.path.join(RES, label, "cards", "*.json"))}


def prf(t):
    p = t["n"] / (t["n"] + t["w"] + t["a"]) if t["n"] + t["w"] + t["a"] else 0
    r = t["n"] / t["l"] if t["l"] else 0
    return p, r, (1.25 * p * r / (0.25 * p + r) if p + r else 0)


def main():
    corpus, off, union, probe, *taus = sys.argv[1:]
    sh, co, cu = shares(corpus, probe), cards(off), cards(union)
    for tau in ["off"] + taus + ["union"]:
        per = collections.defaultdict(lambda: collections.Counter())
        n_on = collections.Counter()
        for k, c in co.items():
            if c["truthMethod"] != "sourcemap":
                continue
            on = tau == "union" or (tau != "off" and sh.get(k, 0.0) >= float(tau))
            t = (cu if on else co)[k]["totals"]
            n_on[c["set"]] += on
            per[c["set"]].update({"n": t["namedBytes"], "w": t["wrongFamilyBytes"], "a": t["appClaimedBytes"],
                                  "l": t["libraryBytes"]})
        for s, t in sorted(per.items()):
            p, r, f = prf(t)
            print(f"tau {tau:6s} {s:16s} union {n_on[s]:3d} P {100 * p:7.3f} R {100 * r:6.2f} F0.5 {100 * f:7.3f}")


if __name__ == "__main__":
    main()
