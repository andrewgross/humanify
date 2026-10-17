"""exp125: per set P / R / F0.5 (summary.json, scorer 10) of several label FAMILIES, each named by its dev label
(the apps / bundlers / pop / test / test2 labels follow the campaign's naming), with dF0.5 vs the first family.
usage: python3 -I table.py <dev label>...   (e.g. exp124-r5 exp121-k2-v2 exp121-v2 exp125-dlf)"""
import json
import os
import sys

RES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../096-library-matching-score/results")


def labels(dev):
    return [dev, dev + "-apps", dev + "-bundlers", "pop-" + dev, "test-" + dev, "test2-" + dev]


def sets(label):
    p = os.path.join(RES, label, "summary.json")
    if not os.path.exists(p):
        return {}
    d = json.load(open(p))
    return {s: (d["schema"], v) for s, v in d["sets"].items()}


def main():
    fams = sys.argv[1:]
    rows = {}
    for f in fams:
        for lb in labels(f):
            for s, v in sets(lb).items():
                rows.setdefault(s, {})[f] = v
    print("| set | " + " | ".join(fams) + " |")
    print("|---" * (len(fams) + 1) + "|")
    for s, by in rows.items():
        base = by.get(fams[0])
        cells = []
        for f in fams:
            if f not in by:
                cells.append("-")
                continue
            sc, r = by[f]
            c = f"{100 * r['precision']:.2f} / {100 * r['recall']:.2f} / **{100 * r['f05']:.2f}**"
            if base and f != fams[0]:
                c += f" ({100 * (r['f05'] - base[1]['f05']):+.2f})"
            if r["appClaimedBytes"]:
                c += f" APP {r['appClaimedBytes']}"
            if sc != "exp096-score/10":
                c += f" [{sc}]"
            cells.append(c)
        print(f"| {s} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
