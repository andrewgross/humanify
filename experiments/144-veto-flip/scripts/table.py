"""exp144: P / R / F0.5, wrong KB (wrongFamily + selfOther) and app-claimed bytes per DEV set for several label
stems, from summary.json only (scorer 14, both scopes), and the bar vs the first stem (P >= base, F0.5 >= base -
0.002 pts, app claimed <= base). usage: python3 -I table.py <out.json> <base stem> <stem> ..."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RES = os.path.join(HERE, "../../096-library-matching-score/results")
DEV = ("{}", "{}-apps", "{}-bundlers", "pop-{}", "remin-{}", "remin3-{}")


def sets(stem):
    out = {}
    for pat in DEV:
        p = os.path.join(RES, pat.format(stem), "summary.json")
        if os.path.exists(p):
            out.update(json.load(open(p))["sets"])
    return out


def row(v):
    return {"P": round(100 * v["precision"], 3), "R": round(100 * v["recall"], 2), "F05": round(100 * v["f05"], 3),
            "wrongKB": round((v["wrongFamilyBytes"] + v.get("selfOtherBytes", 0)) / 1024, 1),
            "app": v["appClaimedBytes"]}


def main():
    out, stems = sys.argv[1], sys.argv[2:]
    data = {s: {k: row(v) for k, v in sets(s).items()} for s in stems}
    base = data[stems[0]]
    print("| set | " + " | ".join(stems) + " |")
    print("| --- |" + " ---: |" * len(stems))
    for k in base:
        cells = []
        for s in stems:
            x = data[s].get(k)
            if x is None:
                cells.append("-")
                continue
            b = base[k]
            ok = x["P"] >= b["P"] and x["F05"] >= b["F05"] - 0.002 and x["app"] <= b["app"]
            mark = "" if s == stems[0] or ok else " **x**"
            cells.append(f"{x['P']:.3f} / {x['R']:.2f} / {x['F05']:.3f}, {x['wrongKB']:,} KB, app {x['app']:,}{mark}")
        print(f"| {k} | " + " | ".join(cells) + " |")
    json.dump(data, open(out, "w"), indent=1)


if __name__ == "__main__":
    main()
