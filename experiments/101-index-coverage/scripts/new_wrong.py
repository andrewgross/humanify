"""exp101: the wrong-family claims a candidate label makes that the baseline label does not (held-out set), for
reading by hand. Prints entry, claimed package, the truth's shipper/version/file and origin, bytes, the claim's
rule and evidence kind, and the first characters of the claimed span.

usage: <exp096 venv python> -I new_wrong.py <baseline label> <candidate label> [N]
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402


def wrong(label, entry, om):
    doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")))
    rep = doc["libraryDiscovery"]
    if rep is None:
        return {}
    units = score.run_units(rep)
    st = score.statement_units(units)
    truth = lib096.load_truth(entry)
    audit = score.truth_audit.Audit(entry, truth, score.truth_audit.load_copies_manifest())
    labels = score.label_units(truth, units, lib096.load_origins(entry, om), audit)
    key = {(u["s"], u["e"]): u for u in st}
    out = {}
    for c in rep.get("claims", []):
        u = key.get((c["start"], c["end"]))
        if u is None:
            continue
        t = labels[u["i"]]
        v, _ = score.judge_origin(c["package"], t, None)
        if v == "wrongFamily":
            out[(c["start"], c["end"])] = (c, t, u["b"])
    return out


def main():
    base, cand = sys.argv[1], sys.argv[2]
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 15
    om = lib096.load_origins_manifest()
    rows = []
    for entry in lib096.load_manifest()["entries"]:
        if entry["set"] != "heldout":
            continue
        b = wrong(base, entry, om)
        for k, (c, t, nb) in wrong(cand, entry, om).items():
            if k in b and b[k][0]["package"] == c["package"]:
                continue
            rows.append((nb, entry, c, t))
    rows.sort(key=lambda r: -r[0])
    tot = sum(r[0] for r in rows)
    print(f"new wrong-family claims: {len(rows)} units, {tot} bytes")
    skip = set(os.environ.get("SKIP", "").split(",")) - {""}
    rows = [r for r in rows if r[2]["package"] not in skip]
    for nb, entry, c, t in rows[:n]:
        src = open(lib096.corpus_path(entry, "bundle"), "rb").read()[c["start"]:c["start"] + 160]
        ev = (c.get("evidence") or {}).get("kind")
        print(f"\n{nb:7d} B  {entry['id']}\n  claimed {c['package']}  rule {c['rule']} evidence {ev}\n"
              f"  truth   {t.get('pkg')}@{t.get('version')} {t.get('file')} origin={t.get('origin')}\n"
              f"  code    {src.decode('utf-8', 'replace')!r}")


if __name__ == "__main__":
    main()
