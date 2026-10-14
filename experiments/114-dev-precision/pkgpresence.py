"""exp114: per (entry, claimed package): bytes by verdict — is a wrong-family package ABSENT from the bundle (every
claim of it wrong) or present with some claims overreaching? usage: <venv python> -I pkgpresence.py [--corpus C] <label>"""
import collections
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import claims as C  # noqa: E402


def main():
    label = ARGV[0]
    om, cm = C.manifests()
    per = collections.defaultdict(collections.Counter)
    for e in C.entries():
        if not os.path.exists(os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")):
            continue
        for r in C.entry_claims(label, e, om, cm).values():
            v = "good" if r["verdict"] in C.GOOD else r["verdict"]
            per[(e["set"], e["id"], r["package"])][v] += r["bytes"]
    cls = collections.Counter()
    rows = []
    for k, c in per.items():
        if not c["wrongFamily"]:
            continue
        kind = "absent(all wrong)" if not c["good"] else "mixed"
        cls[(k[0], kind)] += c["wrongFamily"]
        rows.append((c["wrongFamily"], k, c["good"]))
    for k, v in sorted(cls.items()):
        print(f"{v:>9,}  {k}")
    rows.sort(reverse=True)
    for w, k, g in rows[:40]:
        print(f"{w:>8,} wrong {g:>9,} good  {k}")


if __name__ == "__main__":
    main()
