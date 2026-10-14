"""exp114 lab: every INDEX seed of a label with its structural evidence (mmlib emulation: per-package covered nodes,
the families reaching the cover, every family holding any print of the unit) and its verdict, for simulating
bundle-level seed rules before writing them in Rust. DEV corpora only.
usage: <venv python> -I seedlab.py [--corpus C] <label> <out.jsonl>"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import claims as C  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402
from wrongmech import scores  # noqa: E402


def holders(ix, u):
    """Every package holding any print (whole or function, >= 30 nodes) of u, with specificity: {pkg: nodes}."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    out = {}
    for p in ([whole] if ok_w else []) + list(ok_f):
        for o in ix.owners(p[2], u["mb"], mmlib.RULES["spec"]):
            out[o] = max(out.get(o, 0), p[3])
    return out


def context(ix, u, pkgs):
    """Per package: the smallest index module holding any of u's prints (whole or function), unfiltered."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    out = {}
    for p in ([whole] if ok_w else []) + list(ok_f):
        for r in ix.units.get(p[2], ()):
            if r[0] in pkgs:
                out[r[0]] = min(out.get(r[0], 1 << 62), r[3])
    return out


def main():
    label, out = ARGV[0], ARGV[1]
    om, cm = C.manifests()
    pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    with open(out, "w") as fh:
        for e in C.entries():
            p = os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")
            if not os.path.exists(p):
                continue
            doc = json.load(open(p))
            rep = doc["libraryDiscovery"]
            if rep is None:
                continue
            sl = doc["toolchain"]["slice"]
            if sl not in ixs:
                ixs[sl] = score.load_index(pipeline["indexDir"], sl, pipeline["indexId"])
            ix = ixs[sl]
            units = {(u["s"], u["e"]): u for u in score.run_units(rep)}
            for r in C.entry_claims(label, e, om, cm).values():
                u = units[(r["start"], r["end"])]
                row = dict(r)
                if r["rule"] == "seed" and r["evidence"] in ("index", "queueIndex"):
                    sc, via = scores(ix, u)
                    floor = mmlib.RULES["cover"] * max(u["w"][1] if "w" in u else 1, 1)
                    row["accept"] = sorted(p for p, v in sc.items() if v >= floor)
                    best = max(sc.values()) if sc else 0
                    row["tied"] = sorted(p for p, v in sc.items() if v == best)
                    row["holders"] = holders(ix, u)
                    row["ctx"] = context(ix, u, set(sc))
                    row["mb"] = u["mb"]
                    row["nodes"] = u["w"][1] if "w" in u else 0
                fh.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()
