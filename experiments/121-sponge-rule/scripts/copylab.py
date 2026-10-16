"""exp121 lab: per INDEX seed of a label, the print-level COPY evidence of the claimed package P: for each print P
covers (the structural rule's emulation, exp114 wrongmech.scores order), every OTHER owner of the same print and the
smallest index module each holds it in, against P's own smallest module for it. A sponge (a package whose dist
inlines a dependency) holds its copy in a module far bigger than the origin's. DEV corpora only.

usage: <venv python> -I copylab.py [--corpus C] <label> <out.jsonl>
Row = seedlab-style claim row (exp106 claims.py verdicts) + for index seeds:
  cov    P's covered nodes;  nodes  the unit's whole nodes;  mb  the bundle's module group bytes
  pmb    P's smallest index module over its covering prints (max over prints of the per-print minimum)
  co     {Q: [[nodes, P's module, Q's module], ...]} per covering print co-held by Q (any owner count, no spec)
  acc    packages the structural rule accepts (cover >= 0.5 x nodes)
  pp     per covering print: [nodes, P's module, [[other owner, its smallest module], ...]]"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../../106-matcher-hygiene"))
sys.path.insert(0, os.path.join(HERE, "../../114-dev-precision"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
if lib096.CORPUS in lib096.PRIVATE:
    raise SystemExit("frozen judge: never diagnosed")
import claims as C  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402


def covering(ix, u, pkg):
    """P's covering prints, in the rule's order (whole first, else greedy non-overlapping functions)."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    spec = mmlib.RULES["spec"]
    sc = {}
    if ok_w:
        o = ix.owners(whole[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            return ([whole] if pkg in o else []), {p: whole[3] for p in o}
    hits = []
    for f in ok_f:
        o = ix.owners(f[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            hits.append((f, o))
    hits.sort(key=lambda x: -x[0][3])
    taken, mine = [], []
    for f, o in hits:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        for p in o:
            sc[p] = sc.get(p, 0) + f[3]
        if pkg in o:
            mine.append(f)
    return mine, sc


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
                row = dict(r)
                if r["rule"] == "seed" and r["evidence"] in ("index", "queueIndex"):
                    u = units[(r["start"], r["end"])]
                    mine, sc = covering(ix, u, r["package"])
                    nodes = u["w"][1] if "w" in u else 0
                    row.update(nodes=nodes, mb=u["mb"], cov=sc.get(r["package"], 0),
                               acc=sorted(q for q, v in sc.items() if v >= mmlib.RULES["cover"] * max(nodes, 1)))
                    co, pmb, pp = {}, 0, []
                    for f in mine:
                        rows = ix.units.get(f[2], ())
                        own = min((x[3] for x in rows if x[0] == r["package"]), default=0)
                        pmb = max(pmb, own)
                        best = {}
                        for x in rows:
                            if x[0] != r["package"]:
                                best[x[0]] = min(best.get(x[0], 1 << 62), x[3])
                        for q, m in best.items():
                            co.setdefault(q, []).append([f[3], own, m])
                        pp.append([f[3], own, sorted(best.items())])
                    row.update(pmb=pmb, co=co, pp=pp)
                fh.write(json.dumps(row) + "\n")


if __name__ == "__main__":
    main()
