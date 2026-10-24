"""exp143: annotate rows.py rows with what the INDEX holds for each missed unit, in every slice (the bundle's own
slice is the one the lookup reads; the others say whether another recipe/bundler holds the print). Per slice:
  wFam / wFamSpec      rows of the truth FAMILY (pkg, origin, via) holding the unit's whole print (any version) /
                       those passing the lookup's specificity (index module <= 4 x the unit's module + 4096 B)
  wFamV                a row of the truth PACKAGE holding the whole print at the truth version
  wOwners              packages holding the whole print after specificity (the lookup's max_packages counts these)
  wOtherSpec           another family holds the whole print after specificity
  fFam / fOther        nodes the lookup's greedy function cover gives the family / the best other package (only
                       functions >= 30 nodes whose spec-passing owners are <= 3 packages, as LookupRules)
  fFamAny              family-held function nodes ignoring max_packages and specificity
  fileV / fileAny      the truth (pkg, file) has index rows at the truth version / at any version
plus `helper` (the whole print is in the index's helper table). Index rows below its 20-node unit floor do not
exist, so a unit under it can only be held through a function print.

usage: <venv python> -I annotate.py <index dir> <index id> <out.jsonl> <rows.jsonl> ..."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import score  # noqa: E402

SLICES = ("bun-bun-cjs-min", "bun-node-cjs-min", "esbuild-node-cjs-min", "rollup-node-cjs-min", "webpack-node-cjs-min")
MIN_NODES, MAX_PKG, SPEC = 30, 3, (4, 4096)


def greedy(fns):
    taken, tot = [], 0
    for f in sorted(fns, key=lambda f: -f[3]):
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        tot += f[3]
    return tot


def ask(ix, r, famf):
    mb = r["mb"]
    ok = lambda row: row[3] <= SPEC[0] * mb + SPEC[1]  # noqa: E731
    out = {}
    h, nodes = r["w"][0], r["w"][1]
    rows = ix.units.get(h, ())
    fam_rows = [x for x in rows if famf(x[0])]
    out["wFam"] = len(fam_rows)
    out["wFamSpec"] = sum(1 for x in fam_rows if ok(x))
    out["wFamV"] = any(x[0] == r["pkg"] and r["version"] in ix.expand(x[0], x[4]) for x in fam_rows)
    owners = {x[0] for x in rows if ok(x)}
    out["wOwners"] = len(owners)
    out["wOtherSpec"] = any(not famf(o) for o in owners)
    out["wOther"] = any(not famf(x[0]) for x in rows)
    fam_f, other_f, any_f = [], {}, []
    for f in r["f"]:
        if f[3] < MIN_NODES:
            continue
        frows = ix.units.get(f[2], ())
        if any(famf(x[0]) for x in frows):
            any_f.append(f)
        o = {x[0] for x in frows if ok(x)}
        if not o or len(o) > MAX_PKG:
            continue
        if any(famf(p) for p in o):
            fam_f.append(f)
        for p in o:
            if not famf(p):
                other_f.setdefault(p, []).append(f)
    out["fFam"] = greedy(fam_f)
    out["fOther"] = max((greedy(v) for v in other_f.values()), default=0)
    out["fFamAny"] = greedy(any_f)
    fv = ix.file_versions(r["pkg"], r["file"])
    out["fileV"] = r["version"] in fv
    out["fileAny"] = bool(fv)
    return out


def main():
    ixdir, ixid, out = sys.argv[1:4]
    rows = []
    for p in sys.argv[4:]:
        rows += [json.loads(line) for line in open(p)]
    helpers = {json.loads(line)["hash"] for line in open(os.path.join(ixdir, "helpers.jsonl"))}
    ann = [{"helper": r["w"][0] in helpers} if "w" in r else {} for r in rows]
    for sl in SLICES:
        ix = score.load_index(ixdir, sl, ixid)
        for r, a in zip(rows, ann):
            if "w" not in r:
                continue
            fams = [x for x in (r["pkg"], r.get("origin"), r.get("via")) if x]
            famf = lambda p: any(score.same_family(p, x) for x in fams)  # noqa: E731
            a[sl] = ask(ix, r, famf)
        print("slice", sl, "done", flush=True)
        del ix
    with open(out, "w") as fh:
        for r, a in zip(rows, ann):
            fh.write(json.dumps({"entry": r["entry"], "i": r["i"], **a}) + "\n")


if __name__ == "__main__":
    main()
