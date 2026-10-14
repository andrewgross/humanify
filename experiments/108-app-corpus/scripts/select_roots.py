"""exp108 step 3: new index roots = every package bundled by >= K apps of the ENLARGED training corpus
(exp095's 293 training apps + exp108's apps), not already a root of exp101's c2-l12m4 list.

usage: python3 -I select_roots.py <K> <base roots list> <out new-roots.txt> [--counts]
  --counts  print how many new roots each K in 2..10 would add (sizing), and write nothing.
Held-out apps are excluded from the exp095 side (selection.json "heldout"); exp108's corpus excluded them by identity.
"""
import collections
import json
import os
import sys

SEL095 = "/work/exp095-library-fingerprints/selection"
SEL108 = os.environ.get("SEL", "/work/exp108-data")


def closures():
    held = set(json.load(open(f"{SEL095}/selection.json"))["heldout"])
    pk = collections.Counter()
    n = collections.Counter()
    for sel, src in ((SEL095, "exp095"), (SEL108, "exp108")):
        for slug in sorted(os.listdir(f"{sel}/apps")):
            p = f"{sel}/apps/{slug}/closure.json"
            if not os.path.exists(p):
                continue
            c = json.load(open(p))
            if c["id"] in held or not c.get("packages"):
                continue
            n[src] += 1
            for name in c["packages"]:
                pk[name] += 1
    return pk, n


def main():
    k, base, out = int(sys.argv[1]), sys.argv[2], sys.argv[3]
    pk, n = closures()
    have = {l.strip() for l in open(base) if l.strip()}
    print(f"apps with a closure: {dict(n)}; distinct packages {len(pk)}")
    if "--counts" in sys.argv:
        for kk in range(1, 11):
            print(f"K={kk}: {sum(1 for p, c in pk.items() if c >= kk and p not in have)} new roots")
        return
    new = sorted(p for p, c in pk.items() if c >= k and p not in have)
    open(out, "w").write("\n".join(new) + "\n")
    print(f"K={k}: {len(new)} new roots -> {out}")


if __name__ == "__main__":
    main()
