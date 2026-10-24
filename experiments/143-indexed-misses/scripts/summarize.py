"""exp143: put every missed byte whose truth (package, version) the index holds into ONE mechanism (first match in
the order below), from rows.py rows + annotate.py annotations (the bundle's own slice is the lookup's). Prints the
per-set KB table and writes <out>.json (per set: KB per class; top packages per class by KB, names only; held-out
by aggregate only, no package names).

  W   claimed, but as another family / the app              (a precision-side miss: exp134/136/139's territory)
  S   scorer: the labelled package maps < 50% of the unit's bytes (a straddling unit / glue)
  N   no print: a whole factory kept as one unit
  -- the family holds a print the lookup would accept (whole >= 30 nodes / 3 content, or functions covering
     >= half the whole's nodes, specificity passed, <= 3 packages) -- the lookup or discovery REFUSED:
  R1  too common: > 3 packages hold the whole print
  R2  tie / outscored: another family's prints score >= the family's
  R3  isolation veto (the report's vetoed seed)
  R4  helper table holds the whole print (helper_prints = seed drops it)
  R5  weak and alone: < 100 nodes covered and no family claim within +-10 lookup units (the support rule)
  R6  other discovery guard (conflict / presence / fences / payload)
  -- the family holds a print, but not one the lookup accepts --
  P1  specificity: the family holds the whole print only in an index module > 4x + 4 KB the unit's module
  P2  below the lookup floor: the family holds the whole print but it has < 30 nodes or < 3 content tokens
  P3  partial: family function prints cover < half the whole's nodes
  P4  family function prints exist but are too common (> 3 packages) or fail specificity
  -- the family holds NO print of the unit in this slice --
  T   tiny: no lookup-eligible print at all (whole < 30 nodes, no function >= 30): propagation only
        sub: in a module group the family already claims part of / module group unclaimed / top level
  U1  another family holds the whole print (a copy, a carrier, or the truth's label)
  U2  the truth file is indexed AT THE TRUTH VERSION, the print differs   sub: family holds it in another slice
  U3  the truth file is indexed only at other versions
  U4  the truth file is indexed only in other slices
  U5  the truth file is never indexed (not reached by the recipe's entries, tree-shaken, or dropped)

usage: python3 -I summarize.py <out prefix> <annotations.jsonl> <rows.jsonl> ...  (rows in annotate.py's order)"""
import collections
import json
import sys

HIDDEN = ("heldout",)
ORDER = ["W", "S", "N", "R1", "R2", "R3", "R4", "R5", "R6", "P1", "P2", "P3", "P4", "T", "U1", "U2", "U3", "U4",
         "U5"]
SLICES = ("bun-bun-cjs-min", "bun-node-cjs-min", "esbuild-node-cjs-min", "rollup-node-cjs-min", "webpack-node-cjs-min")


def classify(r, a):
    if r["verdict"] != "unclaimed":
        return "W", "app" if r["verdict"] in ("app", "appClaimed") else "family"
    if r["share"] < 0.5:
        return "S", ""
    if "w" not in r:
        return "N", ""
    x = a[r["slice"]]
    others = [a[s] for s in SLICES if s != r["slice"] and s in a]
    nodes, content = r["w"][1], r["w"][2]
    whole_ok = nodes >= 30 and content >= 3
    whole_hit = whole_ok and x["wFamSpec"] > 0
    fn_hit = x["fFam"] >= 0.5 * max(nodes, 1)
    if whole_hit or fn_hit:
        if whole_hit and x["wOwners"] > 3:
            return "R1", ""
        if (whole_hit and x["wOtherSpec"]) or (not whole_hit and x["fOther"] >= x["fFam"]):
            return "R2", ""
        amb = (not whole_hit) and x["fOther"] >= 0.5 * nodes
        if r["vetoed"]:
            return "R3", ("whole module body" if r["modUnits"] == 1 else "module of several units" if r["m"] is not None
                          else "top level") + (", ambiguous" if amb else "")
        if a.get("helper"):
            return "R4", ""
        cov = nodes if whole_hit else x["fFam"]
        if cov < 100 and r["winFam"] == 0:
            return "R5", ""
        return "R6", "ambiguous (another package reaches the cover)" if amb else "unambiguous"
    if x["wFam"] > 0 and x["wFamSpec"] == 0:
        return "P1", ""
    if x["wFamSpec"] > 0 and not whole_ok:
        return "P2", ("20-29 nodes" if nodes >= 20 else "content < 3") + (
            ", literal-heavy (>= 16 B/node)" if r["bytes"] >= 16 * nodes else "")
    if x["fFam"] > 0:
        return "P3", ""
    if x["fFamAny"] > 0:
        return "P4", ""
    other_slice = any(o["wFam"] > 0 for o in others)
    if not whole_ok and not any(f[3] >= 30 for f in r["f"]):
        ctx = "family-claimed module" if r["modFam"] > 0 else "unclaimed module" if r["m"] is not None else "top level"
        return "T", ctx + (", literal-heavy (>= 16 B/node)" if r["bytes"] >= 16 * nodes else "")
    if x["wOther"]:
        return "U1", ""
    if x["fileV"]:
        return "U2", "held in another slice" if other_slice else "no slice holds it"
    if x["fileAny"]:
        return "U3", ""
    if any(o["fileAny"] for o in others):
        return "U4", "held in another slice" if other_slice else ""
    return "U5", "held in another slice" if other_slice else ""


def main():
    out = sys.argv[1]
    per = collections.defaultdict(collections.Counter)
    sub = collections.defaultdict(collections.Counter)
    top = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    ctx = collections.defaultdict(collections.Counter)  # unclaimed bytes in a family-claimed module, per class
    nunits = collections.defaultdict(collections.Counter)
    sizes = collections.defaultdict(lambda: collections.defaultdict(list))
    ann = open(sys.argv[2])
    for rp in sys.argv[3:]:
        for lr, la in zip(open(rp), ann):
            r, a = json.loads(lr), json.loads(la)
            assert (r["entry"], r["i"]) == (a["entry"], a["i"])
            c, s = classify(r, a)
            st = r["set"]
            per[st][c] += r["bytes"]
            per[st]["total"] += r["bytes"]
            nunits[st][c] += 1
            sizes[st][c].append(r["bytes"])
            if s:
                sub[st][c + ": " + s] += r["bytes"]
            if c != "W" and r["modFam"] > 0:
                ctx[st][c] += r["bytes"]
            if st not in HIDDEN:
                top[st][c][r["pkg"]] += r["bytes"]
    kb = lambda b: round(b / 1024, 1)  # noqa: E731
    doc = {st: {"classes": {c: kb(per[st][c]) for c in ORDER + ["total"] if per[st][c]},
                "sub": {k: kb(v) for k, v in sorted(sub[st].items())},
                "inFamilyClaimedModule": {c: kb(v) for c, v in sorted(ctx[st].items())},
                "units": dict(nunits[st]),
                "top5UnitsShare": {c: round(sum(sorted(v)[-5:]) / sum(v), 3) for c, v in sizes[st].items()},
                **({"top": {c: [[p, kb(b)] for p, b in top[st][c].most_common(8)] for c in ORDER if top[st][c]}}
                   if st not in HIDDEN else {})}
           for st in sorted(per)}
    json.dump(doc, open(out + ".json", "w"), indent=1)
    sets = sorted(per)
    print("| class | " + " | ".join(sets) + " |")
    for c in ORDER + ["total"]:
        print(f"| {c} | " + " | ".join(f"{kb(per[s][c]):,.0f}" for s in sets) + " |")


if __name__ == "__main__":
    main()
