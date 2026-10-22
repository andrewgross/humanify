"""exp133: for each unit label c1 claims WRONG and c0 does not (judge13.py rows), the structural rule's view of it
on c1's index (mmlib's emulation, score per package): is the truth's family (shipper, or its origin) accepted for
the unit at all, and if so is it TIED with the winner or OUTSCORED by it? Aggregate bytes per (set, scope, class).
DEV corpora only (held-out read as aggregates, no names).

usage: <venv python> -I tieclass.py <corpus> <judge.jsonl> <label c1>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def main():
    corpus, path, label = sys.argv[1:4]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import mmlib
    import score
    rows = collections.defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        if r.get("c1", {}).get("verdict") == "wrongFamily" and r.get("c0", {}).get("verdict") != "wrongFamily":
            rows[r["entry"]].append(r)
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    out = collections.Counter()
    for entry, rs in rows.items():
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        ix = score.load_index(pipe["indexDir"], doc["toolchain"]["slice"], pipe["indexId"])
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        for r in rs:
            u = units.get((r["start"], r["end"]))
            right = [x for x in (r["truth"], r.get("origin")) if x and not x.startswith("<")]
            if u is None or "w" not in u:
                cls = "no-prints"
            else:
                sc = {}
                whole, ok_w, ok_f = mmlib.eligible(u)
                if ok_w:
                    for p in ix.owners(whole[2], u["mb"], mmlib.RULES["spec"]):
                        sc[p] = whole[3]
                if not sc:
                    for f in ok_f:
                        for p in ix.owners(f[2], u["mb"], mmlib.RULES["spec"]):
                            sc[p] = sc.get(p, 0) + f[3]  # upper bound: no overlap / max_packages check
                win = r["c1"]["package"]
                best_right = max((v for p, v in sc.items() if any(score.same_family(p, t) for t in right)),
                                 default=None)
                if best_right is None:
                    cls = "truth-not-held" if r["c1"]["rule"] == "seed" else "truth-not-held(prop)"
                elif best_right >= sc.get(win, 0):
                    cls = "tie-or-truth-ahead"
                else:
                    cls = "truth-outscored"
            out[(r["set"], r["scope"], r["c1"]["rule"], cls)] += r["bytes"]
    for k, b in sorted(out.items(), key=lambda kv: -kv[1]):
        print(f"{b / 1e3:9.1f} KB  " + "  ".join(k))


if __name__ == "__main__":
    main()
