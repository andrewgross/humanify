"""exp134: the census outputs (census.py, one file per label) as one markdown table: per set and index, wrong-family
KB by mechanism group. Aggregates only.

usage: python3 -I census_table.py <base census.txt>... -- <dl census.txt>..."""
import collections
import re
import sys

GROUPS = [
    ("outscored, truth smaller context", lambda c: c[:4] == ["seed", "index", "outscored", "carrier"]),
    ("outscored, other", lambda c: c[:3] == ["seed", "index", "outscored"]),
    ("tie", lambda c: c[:3] == ["seed", "index", "tie"]),
    ("truth family not read for the unit", lambda c: c[:2] == ["seed", "index"] and c[2] in ("not-held", "filtered")),
    ("other seed", lambda c: c[0] == "seed"),
    ("propagation off a wrong seed", lambda c: c[0] == "propagation" and c[2] == "off-wrong-seed"),
    ("propagation, other", lambda c: c[0] == "propagation"),
]


def read(paths):
    out = collections.defaultdict(collections.Counter)
    for p in paths:
        for line in open(p):
            m = re.match(r"\s*([\d.]+) KB  (\S+)  (\S+)  (.*)$", line)
            if not m:
                continue
            kb, set_, _scope, rest = float(m.group(1)), m.group(2), m.group(3), m.group(4).split("  ")
            for name, pred in GROUPS:
                if pred(rest):
                    out[set_][name] += kb
                    break
    return out


def main():
    i = sys.argv.index("--")
    base, dl = read(sys.argv[1:i]), read(sys.argv[i + 1:])
    sets = sorted(set(base) | set(dl))
    print("| mechanism | " + " | ".join(sets) + " |")
    print("|---|" + "--:|" * len(sets))
    for name, _ in GROUPS:
        cells = [f"{base[s][name]:.0f} / {dl[s][name]:.0f}" for s in sets]
        print(f"| {name} | " + " | ".join(cells) + " |")
    cells = [f"{sum(base[s].values()):.0f} / {sum(dl[s].values()):.0f}" for s in sets]
    print("| **total** | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
