"""exp141: for every TERSER-MADE index seed of a dossier (exp139's carriers.py output), every package that holds
the unit's prints in the slices the run read, WITHOUT the structural rule's context limit or package cap: per
package (nodes covered by whole-or-functions, the smallest context). A holder the rule cannot see (its context too
big, or one of too many holders) is where a shipper's copy hides. DEV corpora only; output stays OUTSIDE the repo.

usage: <venv python> -I holders.py <corpus> <label> <seeds.jsonl> <out.jsonl>"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../../139-carriers/scripts"))
import lib096  # noqa: E402


def gained(x):
    p = x["package"]
    a, b = x["scores"].get(p), x["scores0"].get(p)
    return a is not None and (b is None or a[0] > b[0])


def holders(ix, u, mmlib):
    whole, ok_w, ok_f = mmlib.eligible(u)
    out = {}
    if ok_w:
        for r in ix.units.get(whole[2], ()):
            n, c = out.get(r[0], (0, 1 << 62))
            out[r[0]] = (whole[3], min(c, r[3]))
        if out:
            return out, True
    taken = []
    for f in sorted(ok_f, key=lambda f: -f[3]):
        rs = ix.units.get(f[2], ())
        if not rs or any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        seen = {}
        for r in rs:
            seen[r[0]] = min(seen.get(r[0], 1 << 62), r[3])
        for p, m in seen.items():
            n, c = out.get(p, (0, 1 << 62))
            out[p] = (n + f[3], min(c, m))
    return out, False


def main():
    corpus, label, path, out = sys.argv[1:5]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a judge corpus")
    lib096.use(corpus)
    import carriers
    import mmlib
    import score
    by = {}
    for line in open(path):
        x = json.loads(line)
        if gained(x):
            by.setdefault(x["entry"], []).append(x)
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    loaded = {}
    fh = open(out, "w")
    for entry, xs in sorted(by.items()):
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        names = carriers.slices_read(doc)
        for n in names:
            if n not in loaded:
                loaded[n] = score.load_index(pipe["indexDir"], n, pipe["indexId"])
        ix = carriers.Merged([loaded[n] for n in names])
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        for x in xs:
            u = units[(x["start"], x["end"])]
            h, w = holders(ix, u, mmlib)
            x = dict(x, holders={p: list(v) for p, v in h.items()}, wholeHit=w,
                     limit=mmlib.RULES["spec"][0] * u["mb"] + mmlib.RULES["spec"][1])
            fh.write(json.dumps(x) + "\n")


if __name__ == "__main__":
    main()
