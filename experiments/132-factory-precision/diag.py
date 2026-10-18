"""exp132 lab: per-unit table of one DEV label's factory scope — truth, claim, verdict and the index's module-size
evidence per family — dumped as one gz JSON per set for offline sizing (never run on test / test2).

usage: <venv python> -I diag.py <label> --corpus C --sets A[,B] --out DIR

Per factory-scope unit: [entry, unit i, factory i, bytes, factory bytes, label, origin, claimed, rule, verdict,
fam]   where fam = {family: smallest index module bytes holding the unit's whole print, else its largest held
function print} over the unit's whole + function prints at the row floor."""
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

MIN_NODES = 20


def fam_context(ix, u):
    """owner package -> (smallest module bytes holding the whole print, smallest holding any function print)."""
    out = {}
    if "w" not in u:
        return out
    w = u["w"]
    if w[1] >= MIN_NODES:
        for r in ix.units.get(w[0], ()):
            f = r[0]
            a = out.setdefault(f, [None, None])
            a[0] = r[3] if a[0] is None else min(a[0], r[3])
    for fn in u.get("f", ()):
        if fn[3] < MIN_NODES:
            continue
        for r in ix.units.get(fn[2], ()):
            f = r[0]
            a = out.setdefault(f, [None, None])
            a[1] = r[3] if a[1] is None else min(a[1], r[3])
    return out


def main():
    argv = lib096.corpus_flag(sys.argv)
    if lib096.CORPUS in lib096.PRIVATE:
        raise SystemExit("REFUSED: a frozen judge is never diagnosed")
    label = argv[1]
    sets = argv[argv.index("--sets") + 1].split(",")
    out_dir = argv[argv.index("--out") + 1]
    if "--misattr13" in argv:
        truth_audit.RULES13["misattributed"] = argv[argv.index("--misattr13") + 1] == "on"
    os.makedirs(out_dir, exist_ok=True)
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipe = json.load(open(os.path.join(run_dir, "pipeline.json")))
    man = lib096.load_manifest()
    om, o7m = lib096.load_origins_manifest(), truth_audit.load_origins7_manifest()
    chm, cm = truth_audit.load_chained_manifest(), truth_audit.load_copies_manifest()
    ixs = {}
    rows = collections.defaultdict(list)
    for entry in man["entries"]:
        if entry["set"] not in sets or entry["truth"]["method"] != "sourcemap":
            continue
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        rep = doc["libraryDiscovery"]
        if rep is None:
            continue
        sl = rep["indexSlice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe.get("indexDir") or pipe.get("index"), sl, rep["indexId"])
        ix = ixs[sl]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        units = score.run_units(rep)
        fac = score.factory_units(units)
        fbytes = collections.Counter()
        for u in fac:
            if u["k"] == "FactoryBody":
                fbytes[u["m"]] += u["b"]
        labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        claims = {(c["start"], c["end"]): c for c in rep.get("claims", [])}
        for u in fac:
            t = labels[u["i"]]
            c = claims.get((u["s"], u["e"]))
            v = score.judge_origin(c["package"], t, truth["appPackage"])[0] if c else None
            rows[entry["set"]].append([entry["id"], u["i"], u.get("m") if u["k"] == "FactoryBody" else u["i"], u["b"],
                                       fbytes.get(u.get("m"), u["b"]), t["label"], t.get("origin"),
                                       c["package"] if c else None,
                                       (c["rule"] + ":" + str((c.get("evidence") or {}).get("kind", "-"))) if c else None, v,
                                       fam_context(ix, u) if c else {}])
    for s, rs in rows.items():
        with gzip.open(os.path.join(out_dir, f"{label}.{s}.json.gz"), "wt") as fh:
            json.dump(rs, fh)
        print(s, len(rs))


if __name__ == "__main__":
    main()
