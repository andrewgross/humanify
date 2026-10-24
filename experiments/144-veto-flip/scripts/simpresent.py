"""exp144: simulate the PRESENCE condition on top of a relaxed-veto label: a claim the new label makes that the
base does not is dropped when its package had no kept seed in the base run of the same bundle (the base's strict
veto removed every seed the package had: the bundle shows the package nowhere but in self-supported seeds).
Reads newwrong.py rows + base reports. Prints KB per set: gained right kept / dropped, new wrong kept / dropped.
usage: python3 -I simpresent.py <rows.json> <corpus>=<base label> ... [--split] [--out=<sim.json>]"""
import collections
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def present(corpus, base, entry, cache={}):
    k = (corpus, base, entry)
    if k not in cache:
        lib096.use(corpus)
        r = json.load(open(os.path.join(lib096.DATA, "results", base, "reports", entry + ".json")))["libraryDiscovery"]
        cache[k] = {p["package"] for p in r["packages"] if p["seedUnits"] > 0}
    return cache[k]


def main():
    rows = json.load(open(sys.argv[1]))
    split = "--split" in sys.argv
    bases = dict(a.split("=", 1) for a in sys.argv[2:] if "=" in a and not a.startswith("--"))
    out = next((a.split("=", 1)[1] for a in sys.argv[2:] if a.startswith("--out=")), None)
    agg = collections.defaultdict(collections.Counter)
    for r in rows:
        if r["kind"] not in ("gainedRight", "newWrong") or r["new"] is None:
            continue
        keep = r["new"] in present(r["corpus"], bases[r["corpus"]], r["entry"])
        s = r["set"]
        if split:
            s += "/h" + str(int(hashlib.sha256(r["entry"].encode()).hexdigest(), 16) % 2)
        agg[s][r["kind"] + ("Kept" if keep else "Dropped")] += r["bytes"]
    sim = {}
    print("| set | gained right kept / dropped KB | new wrong kept / dropped KB |")
    print("| --- | ---: | ---: |")
    for s, c in sorted(agg.items()):
        f = lambda k: round(c[k] / 1024, 1)  # noqa: E731
        sim[s] = [f("gainedRightDropped"), f("newWrongDropped")]
        print(f"| {s} | {f('gainedRightKept')} / {f('gainedRightDropped')} | {f('newWrongKept')} / {f('newWrongDropped')} |")
    if out:
        old = json.load(open(out)) if os.path.exists(out) else {}
        json.dump({**old, **sim}, open(out, "w"), indent=1)


if __name__ == "__main__":
    main()

