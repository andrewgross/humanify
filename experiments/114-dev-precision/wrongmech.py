"""exp114: every wrong-family claimed byte of a label by MECHANISM and by the rule that made the claim (dev corpora
only: dev, apps, bundlers — never the test split).

usage: <venv python> -I wrongmech.py [--corpus dev|apps] <label> <rows-out.jsonl> [--samples N]

Per wrong-family claim (exp106 claims.py's verdicts, the current scorer), re-reads the unit's index evidence with
the in-run structural rule's emulation (exp095 mmlib, specificity on) and records:

  rule        seed:index / seed:queueIndex / seed:declared / seed:banner / down / module / glue
  evidence    for the claimed unit, how the TRUTH package stands in the structural scores:
                tie            truth (or its family) scored the same as the claimed package: the tie-break chose
                outscored      truth holds prints of the unit but fewer covered nodes
                truth-absent   truth's package is indexed but holds none of the unit's prints
                truth-unindexed  truth's package is not in the index
                no-hit         no package holds the unit (a propagated claim over unindexed code)
  relation    claimed vs truth by DECLARED dependencies (any field, devDependencies included, any indexed version;
              the builder's store/declared records): claimed-declares-truth (a shipper of a copy named for its
              origin's code), truth-declares-claimed, shared-dep (both declare a common package), none
  propagation for graph claims: whether the claimed package has RIGHT seeds in the bundle (overreach) or not
Prints bytes per (set, rule, evidence, relation), the top (claimed, truth) groups, and writes every row."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import claims as C  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402

DECLARED = os.environ.get("DECLARED", "/work/exp108-data/idx/store/declared")
_DECL = {}


def declared(pkg):
    """Union of the names every stored version of pkg declares."""
    if pkg in _DECL:
        return _DECL[pkg]
    out = set()
    pre = pkg.replace("/", "__") + "@"
    for f in _FILES.get(pre, ()):
        try:
            out.update(json.load(open(os.path.join(DECLARED, f)))["names"])
        except (OSError, ValueError):
            pass
    _DECL[pkg] = out
    return out


_FILES = collections.defaultdict(list)


def index_declared():
    for f in os.listdir(DECLARED):
        pre = f[: f.rindex("@") + 1]
        _FILES[pre].append(f)


def scores(ix, u):
    """Per package: covered nodes under the structural rule (whole first, else greedy functions)."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    spec = mmlib.RULES["spec"]
    sc = {}
    if ok_w:
        o = ix.owners(whole[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            return {p: whole[3] for p in o}, "whole"
    hits = []
    for f in ok_f:
        o = ix.owners(f[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            hits.append((f, o))
    hits.sort(key=lambda x: -x[0][3])
    taken = []
    for f, o in hits:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        for p in o:
            sc[p] = sc.get(p, 0) + f[3]
    return sc, "functions" if sc else "none"


def whole_owners(ix, u):
    """Every owner of the whole print (no specificity): {owner: smallest module bytes}."""
    if "w" not in u:
        return {}
    out = {}
    for r in ix.units.get(u["w"][0], ()):
        out[r[0]] = min(out.get(r[0], 1 << 62), r[3])
    return out


def relation(claimed, truth):
    if not truth or truth.startswith("<"):
        return "n/a"
    dc, dt = declared(claimed), declared(truth)
    if truth in dc:
        return "claimed-declares-truth"
    if claimed in dt:
        return "truth-declares-claimed"
    if dc and dt and dc & dt:
        return "shared-dep"
    if not dc:
        return "claimed-undeclared"
    return "none"


def main():
    label, out = ARGV[0], ARGV[1]
    samples = int(ARGV[ARGV.index("--samples") + 1]) if "--samples" in ARGV else 40
    index_declared()
    om, cm = C.manifests()
    pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    agg = collections.Counter()
    groups = collections.Counter()
    tot = collections.Counter()
    with open(out, "w") as fh:
        for e in C.entries():
            p = os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")
            if not os.path.exists(p):
                continue
            doc = json.load(open(p))
            rep = doc["libraryDiscovery"]
            if rep is None:
                continue
            cl = C.entry_claims(label, e, om, cm)
            for r in cl.values():
                tot[(r["set"], r["verdict"])] += r["bytes"]
            wrong = [r for r in cl.values() if r["verdict"] == "wrongFamily"]
            if not wrong:
                continue
            sl = doc["toolchain"]["slice"]
            if sl not in ixs:
                ixs[sl] = score.load_index(pipeline["indexDir"], sl, pipeline["indexId"])
            ix = ixs[sl]
            units = {(u["s"], u["e"]): u for u in score.run_units(rep)}
            right_seeds = {r["package"] for r in cl.values() if r["rule"] == "seed" and r["verdict"] in C.GOOD}
            for r in wrong:
                u = units[(r["start"], r["end"])]
                truth = r["origin"] or r["truth"]
                sc, via = scores(ix, u) if "w" in u or u.get("f") else ({}, "none")
                best = max(sc.values()) if sc else 0
                tfam = [p for p in sc if score.same_family(p, truth)]
                if not sc:
                    ev = "no-hit"
                elif tfam and max(sc[p] for p in tfam) == best:
                    ev = "tie"
                elif tfam:
                    ev = "outscored"
                elif ix.has_package(truth):
                    ev = "truth-absent"
                else:
                    ev = "truth-unindexed"
                rule = "seed:" + r["evidence"] if r["rule"] == "seed" else r["rule"]
                rel = relation(r["package"], truth)
                prop = "" if r["rule"] == "seed" else (
                    "overreach" if r["package"] in right_seeds else "from-wrong")
                wo = whole_owners(ix, u)
                row = {**r, "truthPkg": truth, "slice": sl, "ev": ev, "via": via, "rel": rel, "prop": prop,
                       "scores": sc, "wholeOwners": wo, "mb": u["mb"], "nodes": u["w"][1] if "w" in u else 0}
                fh.write(json.dumps(row) + "\n")
                agg[(r["set"], rule, ev, rel)] += r["bytes"]
                groups[(r["set"], r["package"], truth, rule, ev, rel)] += r["bytes"]
    for k, v in sorted(tot.items()):
        if k[1] in ("wrongFamily", "right", "family", "origin"):
            print(f"{v:>11,}  {k}")
    print()
    by_set = collections.Counter()
    for (s, rule, ev, rel), v in agg.items():
        by_set[(s, rule, ev)] += v
    for k, v in sorted(by_set.items(), key=lambda x: (x[0][0], -x[1])):
        print(f"{v:>9,}  {k}")
    print()
    rel_set = collections.Counter()
    for (s, rule, ev, rel), v in agg.items():
        rel_set[(s, rel)] += v
    for k, v in sorted(rel_set.items(), key=lambda x: (x[0][0], -x[1])):
        print(f"{v:>9,}  {k}")
    print()
    for k, v in groups.most_common(samples):
        print(f"{v:>9,}  {k}")


if __name__ == "__main__":
    main()
