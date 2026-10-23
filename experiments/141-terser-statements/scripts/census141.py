"""exp141: the new wrong bytes (wrong family or app code claimed) one label adds over another on a DEV corpus's
terser set, by error class (data/classes.json; truth <app> = app-copy), scope and seed/propagated. Input: a
judge13.py file of (base, variant). Prints KB; --json writes the table (classes and sizes only).

usage: python3 -I census141.py <judge.jsonl> [--json out.json]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    a = sys.argv[1:]
    out = a[a.index("--json") + 1] if "--json" in a else None
    cls = json.load(open(os.path.join(HERE, "../data/classes.json")))
    of = {tuple(p): k for k, ps in cls.items() if k != "about" for p in ps}
    t = collections.Counter()
    for line in open(a[0]):
        r = json.loads(line)
        if not r["set"].endswith("terser"):
            continue
        c0, c1 = r.get("c0"), r.get("c1")
        if not c1 or c1["verdict"] not in ("wrongFamily", "app"):
            continue
        if c0 and c0["package"] == c1["package"]:
            continue
        k = "app-copy" if r["truth"] == "<app>" else of.get((c1["package"], r["truth"]), "other")
        t[(k, r["scope"], "seed" if c1["rule"] == "seed" else "prop")] += r["bytes"]
    rows = {}
    for (k, sc, how), b in t.items():
        rows.setdefault(k, collections.Counter())[f"{sc}/{how}"] += b
    for k in sorted(rows, key=lambda k: -sum(rows[k].values())):
        print(f"{k:22s} {sum(rows[k].values())/1024:7.1f} KB  " +
              "  ".join(f"{x} {v/1024:.1f}" for x, v in sorted(rows[k].items())))
    print(f"{'total':22s} {sum(t.values())/1024:7.1f} KB")
    if out:
        json.dump({k: dict(v) for k, v in rows.items()}, open(out, "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main()
