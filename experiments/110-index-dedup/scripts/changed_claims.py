"""exp110: every statement-unit claim that differs between two labels of the DEV corpus (a package changed, a claim
added or removed), with the scorer's verdict on each side, for reading by hand. Never run on the test corpus.

usage: <exp096 venv python> -I changed_claims.py <baseline label> <candidate label> [N] [--sets a,b]
Prints the totals of verdict transitions (bytes), then the N largest changes with the first characters of the span.
"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402


def claims(label, entry, om, cm, chm):
    path = os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")
    rep = json.load(open(path))["libraryDiscovery"]
    if rep is None:
        return {}, {}
    units = score.run_units(rep)
    st = score.statement_units(units)
    truth = score.truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    if truth["method"] == "sample":
        return {}, truth
    audit = score.truth_audit.Audit(entry, truth, cm)
    labels = score.label_units(truth, units, lib096.load_origins(entry, om), audit)
    key = {(u["s"], u["e"]): u for u in st}
    out = {}
    for c in rep.get("claims", []):
        u = key.get((c["start"], c["end"]))
        if u is None:
            continue
        v, _ = score.judge_origin(c["package"], labels[u["i"]], truth["appPackage"])
        out[(c["start"], c["end"])] = (c["package"], v, u["b"], labels[u["i"]])
    return out, truth


def main():
    args = [a for a in sys.argv[1:]]
    sets = None
    if "--sets" in args:
        i = args.index("--sets")
        sets = set(args[i + 1].split(","))
        del args[i:i + 2]
    base, cand = args[0], args[1]
    n = int(args[2]) if len(args) > 2 else 15
    om = lib096.load_origins_manifest()
    cm = score.truth_audit.load_copies_manifest()
    chm = score.truth_audit.load_chained_manifest()
    trans = collections.Counter()
    rows = []
    for entry in lib096.load_manifest()["entries"]:
        if sets and entry["set"] not in sets:
            continue
        try:
            b, _ = claims(base, entry, om, cm, chm)
            c, _ = claims(cand, entry, om, cm, chm)
        except FileNotFoundError:
            continue
        for k in set(b) | set(c):
            pb, pc = b.get(k), c.get(k)
            if pb and pc and pb[0] == pc[0]:
                continue
            nb = (pb or pc)[2]
            t = (pc or pb)[3]
            trans[(entry["set"], pb[1] if pb else "-", pc[1] if pc else "-")] += nb
            rows.append((nb, entry, k, pb[0] if pb else "-", pc[0] if pc else "-",
                         pb[1] if pb else "-", pc[1] if pc else "-", t))
    print("verdict transitions (set, before, after): bytes")
    for k, v in sorted(trans.items(), key=lambda kv: -kv[1]):
        print(f"  {k}: {v}")
    rows.sort(key=lambda r: -r[0])
    print(f"changed claims: {len(rows)}")
    for nb, entry, (s, e), pb, pc, vb, vc, t in rows[:n]:
        src = open(lib096.corpus_path(entry, "bundle"), "rb").read()[s:s + 140]
        print(f"\n{entry['id']} [{entry['set']}] {nb} B  {pb} ({vb}) -> {pc} ({vc})")
        print(f"  truth: {t.get('label')} file={t.get('file')} origin={t.get('origin')}")
        print("  " + src.decode("utf8", "replace").replace("\n", " "))


if __name__ == "__main__":
    main()
