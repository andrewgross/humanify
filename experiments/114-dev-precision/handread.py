"""exp114: claims a candidate label REMOVED or RENAMED relative to a base label, with the truth's own source file
and the bundle text, for hand-reading whether each was really wrong. DEV corpora only.
usage: <venv python> -I handread.py [--corpus C] <base> <label> [--n N] [--seed S]"""
import json
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import claims as C  # noqa: E402


def main():
    base, label = ARGV[0], ARGV[1]
    n = int(ARGV[ARGV.index("--n") + 1]) if "--n" in ARGV else 15
    rnd = random.Random(int(ARGV[ARGV.index("--seed") + 1]) if "--seed" in ARGV else 114)
    om, cm = C.manifests()
    moved = []
    for e in C.entries():
        if not os.path.exists(os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")):
            continue
        a = C.entry_claims(base, e, om, cm)
        labels = C._CTX[e["id"]][2]
        b = C.entry_claims(label, e, om, cm)
        for i, ca in a.items():
            cb = b.get(i)
            if cb and cb["package"] == ca["package"]:
                continue
            moved.append((e, i, ca, cb, labels[i]))
    # one draw per (entry, claimed package, new package): the biggest unit of each group, then a random sample
    groups = {}
    for m in moved:
        k = (m[0]["id"], m[2]["package"], m[3]["package"] if m[3] else None)
        if k not in groups or m[2]["bytes"] > groups[k][2]["bytes"]:
            groups[k] = m
    picks = rnd.sample(sorted(groups.values(), key=lambda m: (m[0]["id"], m[1])), min(n, len(groups)))
    print(f"{len(moved)} units moved in {len(groups)} (entry, package, new package) groups; {len(picks)} drawn\n")
    for e, i, ca, cb, lab in picks:
        code = open(lib096.corpus_path(e, "bundle"), "rb").read()
        text = code[ca["start"]:ca["end"]].decode("utf-8", "replace")
        print(f"--- {e['id']}  unit {ca['start']}-{ca['end']} ({ca['bytes']} B)")
        print(f"    was: {ca['package']} ({ca['rule']}/{ca['evidence'] or '-'}) verdict {ca['verdict']}")
        print(f"    now: {(cb['package'] + ' verdict ' + cb['verdict']) if cb else 'unclaimed'}")
        print(f"    truth: {lab.get('label')} {lab.get('version', '')} {lab.get('file', '')}  origin {lab.get('origin')}")
        print(f"    text: {text[:240]!r}\n")


if __name__ == "__main__":
    main()
