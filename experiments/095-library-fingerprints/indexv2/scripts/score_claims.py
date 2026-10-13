"""Score library-discovery reports (exp095-core-discover with INDEX=...) exactly against a bundle's source map
(lane "indexv2"): every top-level statement unit is labelled by the package most of its mapped bytes come from
(`node_modules/<pkg>`; anything else, and the CLI's own package, is APP code), then each claim is judged.

usage: <venv python> -I score_claims.py <bundle.js> <bundle.js.map> <units.tsv> <out.json> <app package or -> \
           <report.json>... [--inner <inner.map> <line offset>]
Per report, by statement bytes: library bytes in the bundle; claimed right package / same family (another member of
the family, libix.family) / wrong family / app / unmapped; the same for index seeds alone (evidence kind index or
queueIndex); samples of every wrong-family and app claim (package, truth, text) for reading by hand.
"""
import collections, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../discovery/other"))
sys.path.insert(0, os.path.join(HERE, "../../selection/scripts"))
import score_sourcemap as ssm  # noqa: E402
from libix import family  # noqa: E402


def labeller(text, smap, app_pkg):
    segs = ssm.segments(text, smap)
    offs = [s[0] for s in segs]
    sources = smap["sources"]
    import bisect

    def label(s, e):
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        votes = collections.Counter()
        while k < len(segs) and segs[k][0] < e:
            a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                p = ssm.package_of(sources[segs[k][1]])
                votes["<app>" if p is None else ("<self>" if p == app_pkg else p)] += b - a
            k += 1
        return votes.most_common(1)[0][0] if votes else "<unmapped>"
    return label


def judge(claimed, truth, app_pkg):
    if truth == "<self>":  # the CLI's own package: a library too when the index names it so (vite, svgo)
        return "self" if claimed == app_pkg else "app"
    if truth == "<app>":
        return "app"
    if truth == "<unmapped>":
        return "unmapped"
    if claimed == truth:
        return "right"
    return "family" if family(claimed) == family(truth) else "wrongFamily"


def main():
    args = sys.argv[1:]
    inner = None
    if "--inner" in args:
        i = args.index("--inner")
        inner = (args[i + 1], int(args[i + 2]))
        args = args[:i] + args[i + 3:]
    bundle, mapf, units_tsv, out, app_pkg, reports = args[0], args[1], args[2], args[3], args[4], args[5:]
    text = open(bundle, encoding="utf8").read()
    smap = json.load(open(mapf))
    if inner:
        smap = ssm.compose(smap, json.load(open(inner[0])), inner[1])
    label = labeller(text, smap, None if app_pkg == "-" else app_pkg)
    units = {}
    for l in open(units_tsv):
        s, e, owned, kind, runtime = l.rstrip("\n").split("\t")
        if (kind == "Statement" or kind.startswith("LazyBody")) and runtime != "true":
            units[(int(s), int(e))] = int(owned)
    lab = {k: label(*k) for k in units}
    total = collections.Counter()
    for k, o in units.items():
        total["lib" if lab[k] not in ("<app>", "<self>", "<unmapped>") else lab[k]] += o
    res = {"bundle": bundle, "statementBytes": dict(total), "reports": {}}
    for rp in reports:
        r = json.load(open(rp))
        row = {}
        for scope, keep in (("all", lambda c: True),
                            ("indexSeeds", lambda c: c["rule"] == "seed" and (c.get("evidence") or {}).get("kind") in ("index", "queueIndex"))):
            c, samples = collections.Counter(), []
            for cl in r.get("claims", []):
                k = (cl["start"], cl["end"])
                if k not in units or not keep(cl):
                    continue
                v = judge(cl["package"], lab[k], app_pkg)
                c[v] += units[k]
                if v in ("app", "wrongFamily", "family") and len(samples) < 60:
                    samples.append({"verdict": v, "package": cl["package"], "truth": lab[k], "bytes": units[k],
                                    "rule": cl["rule"], "text": text[cl["start"]:cl["start"] + 120]})
            named = c["right"] + c["family"] + c["wrongFamily"]
            row[scope] = {"bytes": dict(c), "recall": round((c["right"] + c["family"]) / max(total["lib"], 1), 4),
                          "wrongFamilyShareOfNamed": round(c["wrongFamily"] / max(named, 1), 4),
                          "samples": samples}
        res["reports"][rp] = row
        print(os.path.basename(rp), {s: {k: v for k, v in row[s].items() if k != "samples"} for s in row}, flush=True)
    json.dump(res, open(out, "w"), indent=1)


if __name__ == "__main__":
    main()
