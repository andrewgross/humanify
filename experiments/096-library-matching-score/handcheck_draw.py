"""exp096: draw the hand-check sample for the origin-aware truth (handcheck.json holds the verdicts read by hand).

usage: <venv python> -I handcheck_draw.py <label> [<label>...]   (default: integ2-structural integ2-agreement)
Every wrong-family claim under the SHIPPER-ONLY truth (scorer 1's judge) on the source-mapped sets, 100 byte-weighted
draws (seed 96), and for each distinct unit: shipper, file, claim, the origin layer's evidence, and the shipped
file's source at the mapped line plus the comment lines above it — printed for reading, never written (the
sources hold third-party and proprietary code)."""
import bisect
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib096  # noqa: E402
import origins as O  # noqa: E402
import score  # noqa: E402


def wrong_claims(label):
    out = []
    om = lib096.load_origins_manifest()
    for entry in lib096.load_manifest()["entries"]:
        if entry["truth"]["method"] != "sourcemap":
            continue
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry["id"] + ".json")))
        rep = doc["libraryDiscovery"]
        if rep is None:
            continue
        truth = lib096.load_truth(entry)
        units = score.run_units(rep)
        st = score.statement_units(units)
        labels = score.label_units(truth, units, lib096.load_origins(entry, om))
        key = {(u["s"], u["e"]): u for u in st}
        for c in rep["claims"]:
            u = key.get((c["start"], c["end"]))
            if u and score.judge(c["package"], labels[u["i"]]["label"], truth["appPackage"]) == "wrongFamily":
                t = labels[u["i"]]
                out.append((label, entry, u["s"], u["e"], u["b"], t, c["package"]))
    return out


def main():
    labels = sys.argv[1:] or ["integ2-structural", "integ2-agreement"]
    rows = [r for lab in labels for r in wrong_claims(lab)]
    random.seed(96)
    picks = sorted(set(random.choices(range(len(rows)), weights=[r[4] for r in rows], k=100)))
    maps = {}
    for n, i in enumerate(picks):
        label, entry, s, e, b, t, claim = rows[i]
        if entry["id"] not in maps:
            js = lib096.corpus_path(entry, "bundle")
            sm = json.load(open(lib096.corpus_path(entry, "innerMap" if "innerMap" in entry["files"] else "map")))
            outer = json.load(open(js + ".map"))
            text = open(js, encoding="utf8").read()
            segs = O.segments(text, outer, (sm, 1) if "innerMap" in entry["files"] else None)
            maps[entry["id"]] = (sm, segs, [x[0] for x in segs], lib096.load_truth(entry))
        sm, segs, offs, truth = maps[entry["id"]]
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        while k < len(segs) - 1 and segs[k][0] < e and truth["sources"][segs[k][1]][0] != t["label"]:
            k += 1
        si, ol = segs[k][1], segs[k][2]
        lines = ((sm.get("sourcesContent") or [None] * len(sm["sources"]))[si] or "").split("\n")
        above = [ln for ln in lines[max(0, ol - 400):ol] if ln.lstrip().startswith(("//", "/*"))][-3:]
        print(f"===== {n} {label} {entry['id']} {b}B shipper={t['label']} file={t.get('file')} claim={claim} "
              f"origin={t.get('origin')} ({t.get('originEvidence')})")
        print("  above:", above)
        print("  source:", "\n          ".join(lines[ol:ol + 5]))


if __name__ == "__main__":
    main()
