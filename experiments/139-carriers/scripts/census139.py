"""exp139: census of CARRIER-wrong bytes, from a judge13.py file (slots c0 = base, c1 = variant) and carriers.py's
dossier of the variant's index seeds. DEV corpora only. Prints per set (aggregates; JSON with --json):

  wrong bytes in c1, and NEW wrong bytes (wrong in c1, not wrong in c0), split into
    seed/carrier-tie       an index seed whose truth family (shipper or origin) scores the unit as well (a tie the
                           name order / previous claim decided)
    seed/carrier-outscored the truth family scores the unit, fewer nodes
    seed/truth-not-scoring the truth family holds no print the lookup reads (coverage, or a copy whose shipper is not
                           indexed: the claimed package's code shipped inside another package)
    seed/other             a declared/banner/helper seed, or no print
    prop/off-carrier       propagated by a package with a carrier-tie/outscored wrong seed in the same bundle
    prop/other             every other propagated wrong byte
  and for the seed classes whether the claimer is SHOWN in the bundle (exclusive index-seed bytes >= 10% of its
  index-seed bytes, or a declared seed) and whether the terser slices made the seed.

usage: <venv python> -I census139.py <judge.jsonl> <dossier.jsonl> [--json out.json] [--names SET]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))


def main():
    import score
    same = score.same_family
    judge, dossier = sys.argv[1:3]
    names = sys.argv[sys.argv.index("--names") + 1] if "--names" in sys.argv else None
    if names == "heldout":
        raise SystemExit("REFUSED: no held-out package names")
    dos = {}
    fam_seed = collections.Counter()
    for line in open(dossier):
        x = json.loads(line)
        dos[(x["entry"], x["start"], x["end"])] = x
        fam_seed[(x["entry"], x["package"])] += x["bytes"]
    out = collections.Counter()
    carrier_pkgs = set()
    rows = []
    for line in open(judge):
        r = json.loads(line)
        c1 = r.get("c1")
        if not c1 or c1["verdict"] != "wrongFamily":
            continue
        new = r.get("c0", {}).get("verdict") != "wrongFamily"
        rows.append((r, c1, new))
        x = dos.get((r["entry"], r["start"], r["end"]))
        if c1["rule"] == "seed" and x:
            right = [t for t in (r["truth"], r.get("origin")) if t and not t.startswith("<")]
            sc = x["scores"]
            wn = sc.get(c1["package"], [0])[0]
            tn = max([v[0] for p, v in sc.items() if any(same(p, t) for t in right)], default=0)
            if tn and tn >= wn:
                x["cls"] = "seed/carrier-tie"
            elif tn:
                x["cls"] = "seed/carrier-outscored"
            else:
                x["cls"] = "seed/truth-not-scoring"
            if x["cls"] != "seed/truth-not-scoring":
                carrier_pkgs.add((r["entry"], c1["package"]))
    by_pkg = collections.Counter()
    for r, c1, new in rows:
        x = dos.get((r["entry"], r["start"], r["end"]))
        if c1["rule"] == "seed":
            cls = x["cls"] if x and "cls" in x else "seed/other"
            if x and "cls" in x:
                w = c1["package"]
                e = x["exclusive"].get(w, 0)
                shown = w in x["declared"] or (e > 0 and e >= 0.1 * fam_seed[(r["entry"], w)])
                s0 = x.get("scores0", x["scores"])
                gained = x.get("slices", 1) > 1 and (w not in s0 or s0[w][0] < x["scores"].get(w, [0])[0])
                sub = ("shown" if shown else "not-shown") + ("/terser-made" if gained else "")
            else:
                sub = "-"
        else:
            cls = "prop/off-carrier" if (r["entry"], c1["package"]) in carrier_pkgs else "prop/other"
            sub = "-"
        for scope in (["all"] + (["new"] if new else [])):
            out[(r["set"], scope, cls, sub)] += r["bytes"]
            out[(r["set"], scope, "TOTAL", "")] += r["bytes"]
        if names and r["set"] == names and new:
            by_pkg[(cls, c1["package"], r["truth"], r.get("origin"))] += r["bytes"]
    res = collections.defaultdict(dict)
    for (s, scope, cls, sub), b in sorted(out.items()):
        res[s].setdefault(scope, {})[cls + ("" if not sub or sub == "-" else " " + sub)] = b
    if "--json" in sys.argv:
        json.dump(res, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1, sort_keys=True)
    for s, d in res.items():
        for scope, cl in d.items():
            print(f"{s} [{scope}] " + "  ".join(f"{k}: {v / 1e3:.1f}" for k, v in cl.items()))
    for k, b in sorted(by_pkg.items(), key=lambda kv: -kv[1])[:40]:
        print(f"{b / 1e3:8.1f} KB  {k}")


if __name__ == "__main__":
    main()
