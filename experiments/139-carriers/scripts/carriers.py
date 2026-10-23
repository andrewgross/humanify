"""exp139: per-seed dossier of every INDEX seed one label claims (right and wrong), for simulating carrier rules
before any crates change. Reads a judge13.py file (exp133; scorer 14 since exp137 lives in score.label_units), the
label's reports and its index -- ALL the slices the run read for that bundle (terser_slices = auto reads the
terser-built slices beside the chosen one when its verdict is `union`). DEV corpora only; output stays OUTSIDE the
repo (it names packages).

Per index seed: the structural rule's scores per package (census.scores, over the merged slices), the claimer's and
the truth's family, the same scores over the CHOSEN slice alone (`scores0`), the earliest registry publication time of a version holding the unit's covering prints per
scoring package, and per bundle the family's EXCLUSIVE seed bytes (index seeds whose every scoring package is that
family) and declared/banner seeds. Plus per (entry, package) the non-seed (propagated) claim bytes by verdict.

usage: <venv python> -I carriers.py <corpus> <judge.jsonl> <slot c0|c1> <label> <registry dir> <out.jsonl>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

RIGHT = ("right", "family", "origin", "selfOrigin")


class Merged:
    """The union of several mmlib.Index slices' unit rows (what LibraryIndex::unit_hits reads with union)."""

    def __init__(self, ixs):
        self.ixs = ixs
        self.units = self

    def get(self, h, default=()):
        out = []
        for ix in self.ixs:
            out.extend(ix.units.get(h, ()))
        return out or default


def slices_read(doc):
    ld = doc["libraryDiscovery"]
    sl = [ld.get("indexSlice") or doc["toolchain"]["slice"]]
    why = ld.get("indexSliceReason") or ""
    if "-> union" in why and "also " in why:
        extra = why.split("also ", 1)[1].split(";", 1)[0]
        sl += [s.strip() for s in extra.split(",") if s.strip() and not s.strip().startswith("none")]
    return sl


class Registry:
    def __init__(self, d):
        self.d, self.cache = d, {}

    def time(self, pkg, ver):
        if pkg not in self.cache:
            p = os.path.join(self.d, pkg.replace("/", "__") + ".json")
            self.cache[pkg] = json.load(open(p)).get("time", {}) if os.path.exists(p) else {}
        return self.cache[pkg].get(ver)


def hashes_read(u, mmlib):
    whole, ok_w, ok_f = mmlib.eligible(u)
    return ([whole[2]] if ok_w else []) + [f[2] for f in ok_f]


def main():
    corpus, path, slot, label, regdir, out = sys.argv[1:7]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import mmlib
    import score
    sys.path.insert(0, os.path.join(HERE, "../../134-carrier-ownership/scripts"))
    from census import scores
    reg = Registry(regdir)
    seeds = collections.defaultdict(list)
    prop = collections.defaultdict(collections.Counter)
    declared = collections.defaultdict(set)
    for line in open(path):
        r = json.loads(line)
        c = r.get(slot)
        if not c:
            continue
        if c["rule"] == "seed" and c["evidence"] in ("index", "queueIndex"):
            seeds[r["entry"]].append(r)
        elif c["rule"] == "seed":
            declared[r["entry"]].add(c["package"])
        else:
            v = "right" if c["verdict"] in RIGHT else "wrong" if c["verdict"] == "wrongFamily" else c["verdict"]
            prop[(r["entry"], c["package"])][v] += r["bytes"]
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    loaded = {}
    fh = open(out, "w")
    for entry, rs in sorted(seeds.items()):
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        names = slices_read(doc)
        for n in names:
            if n not in loaded:
                loaded[n] = score.load_index(pipe["indexDir"], n, pipe["indexId"])
        ix = Merged([loaded[n] for n in names])
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        rows = []
        for r in rs:
            u = units.get((r["start"], r["end"]))
            if u is None or "w" not in u:
                continue
            sc = scores(ix, u, mmlib)
            sc0 = scores(loaded[names[0]], u, mmlib) if len(names) > 1 else sc
            first = {}
            for h in hashes_read(u, mmlib):
                for o in ix.units.get(h, ()):
                    if o[0] not in sc:
                        continue
                    for lo, _hi in o[4]:
                        t = reg.time(o[0], lo)
                        if t and (o[0] not in first or t < first[o[0]]):
                            first[o[0]] = t
            c = r[slot]
            rows.append({"entry": entry, "set": r["set"], "scope": r["scope"], "bytes": r["bytes"],
                         "start": r["start"], "end": r["end"], "mb": u["mb"], "app": r["app"],
                         "truth": r["truth"], "origin": r.get("origin"), "package": c["package"],
                         "verdict": c["verdict"], "round": c["round"], "slices": len(names),
                         "scores": {p: list(v) for p, v in sc.items()}, "first": first,
                         "scores0": {p: list(v) for p, v in sc0.items()}})
        excl = collections.Counter()
        for x in rows:
            fams = set()
            for p in x["scores"]:
                if not any(score.same_family(p, q) for q in fams):
                    fams.add(p)
            if len(fams) == 1:
                excl[next(iter(fams))] += x["bytes"]
        for x in rows:
            x["exclusive"] = {p: sum(b for q, b in excl.items() if score.same_family(p, q)) for p in x["scores"]}
            x["exclusive"].setdefault(x["package"], sum(b for q, b in excl.items()
                                                        if score.same_family(x["package"], q)))
            x["declared"] = sorted(p for p in x["scores"] if any(score.same_family(p, d) for d in declared[entry]))
            x["prop"] = {p: dict(prop.get((entry, p), {})) for p in set(x["scores"]) | {x["package"]}}
            fh.write(json.dumps(x) + "\n")


if __name__ == "__main__":
    main()
