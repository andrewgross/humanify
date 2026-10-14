"""exp106 probe: for claims of a label whose package matches, the unit's index evidence: the whole print's owners
and every function print's owners (unfiltered), and the bundle text head. DEV only.
usage: <venv python> -I probe.py <label> <package> [--entry substr] [--max N] [--text]"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, HERE)
import lib096  # noqa: E402
import score  # noqa: E402
import claims as C  # noqa: E402


def main():
    a = sys.argv[1:]
    label, pkg = a[0], a[1]
    ent = a[a.index("--entry") + 1] if "--entry" in a else ""
    mx = int(a[a.index("--max") + 1]) if "--max" in a else 10
    om, cm = C.manifests()
    pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    n = 0
    for e in C.entries():
        if ent not in e["id"]:
            continue
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")))
        if doc["libraryDiscovery"] is None:
            continue
        cl = C.entry_claims(label, e, om, cm)
        hits = [r for r in cl.values() if r["package"] == pkg]
        if not hits:
            continue
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipeline["indexDir"], sl, pipeline["indexId"])
        ix = ixs[sl]
        units = {u["i"]: u for u in score.run_units(doc["libraryDiscovery"])}
        code = open(lib096.corpus_path(e, "bundle"), "rb").read()
        for (i, r) in sorted(cl.items()):
            if r["package"] != pkg:
                continue
            u = units[i]
            print(f"== {e['id']} [{r['start']},{r['end']}) {r['bytes']}B rule={r['rule']} ev={r['evidence']} "
                  f"verdict={r['verdict']} truth={r['truth']} origin={r['origin']}")
            if "w" in u:
                own = sorted({x[0] for x in ix.units.get(u["w"][0], ())})
                print(f"   whole nodes={u['w'][1]} owners({len(own)})={own[:12]}")
                for f in u["f"]:
                    if f[3] >= 20:
                        o = sorted({x[0] for x in ix.units.get(f[2], ())})
                        if o:
                            print(f"   fn [{f[0]},{f[1]}) nodes={f[3]} owners({len(o)})={o[:12]}")
            if "--text" in a:
                print("   ", code[r["start"]:min(r["end"], r["start"] + 300)].decode("utf8", "replace"))
            n += 1
            if n >= mx:
                return


if __name__ == "__main__":
    main()
