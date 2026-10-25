"""exp153 census: for today's lookup decisions, how many would call context confirm or refute, right or wrong
against the truth (exp096's scorer-14 truth, the same label_units the score reads).

usage: <venv python> -I census.py <corpus> <census reports dir> [--baseline <label>] > census-<corpus>.json

Per bundle it reads the census run's reports (census_run.py: `--call-context`, main's defaults) and, with
--baseline, checks that every claim equals that label's report (the census flag must change no claim).

Signals, per (unit, package family): `fit` (every callee of the unit with a print is one the family's index rows
record; at least one callee), `rowed` (the same over callees at or over the 20-node row floor), `tree` (the
unit's call-tree hash equals one of the family's). A signal CONFIRMS, REFUTES (a callee the rows lack; never for
`tree`, which only confirms) or says NOTHING (no callee / no row).

Decisions (the unit's verdict under main's defaults):
  claim       an index seed that was kept (evidence index or queueIndex): the claimed family's signal
  propagated  a unit the fixpoint claimed (down / module / glue) that the index holds a print of
  veto        an index seed the isolation veto dropped: confirm = would be rescued
  support     a structural pick the position window refused (covered < 100 nodes, no claim of its package
              within 10 units), not claimed: confirm = would be claimed
  tie         a pick whose best score more than one family reaches: does the signal single out one of them
  common      not claimed, whole print >= 30 nodes held (specificity aside) by more than max_packages (3) owners:
              does the signal single out one family
  floor       not claimed, whole print 20-29 nodes (under the 30-node floor), held by at most 3 owners:
              does the signal single out one family
Verdicts against the truth: right (the package, its family or the unit's origin's family), wrong (another family),
app (app code: the hard bar), other (self / unmapped / foreign: unjudged, not counted).
"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(HERE, "../../096-library-matching-score")
sys.path.insert(0, SCORE)
import lib096  # noqa: E402

SIGNALS = ("fit", "rowed", "tree")
MAX_PACKAGES = 3
FLOOR = 30
STRONG = 100


def signal(fam, sig, row=None):
    """'confirm' | 'refute' | 'none' for one FamilyFit / PickRow dict (`tree` confirms only a unit with a callee:
    a leaf's tree is its own print)."""
    if fam is None:
        return "none"
    if sig == "tree":
        return "confirm" if fam.get("tree") and (row is None or row["callees"] > 0) else "none"
    f = fam.get("fit" if sig == "fit" else "rowedFit")
    if f is None and sig != "fit":
        f = fam.get("familyRowedFit")
    if not f or f["callees"] == 0:
        return "none"
    return "confirm" if f["held"] == f["callees"] else "refute"


class Tally:
    def __init__(self):
        self.c = collections.defaultdict(lambda: collections.Counter())

    def add(self, cat, key, weight):
        self.c[cat][key + "|n"] += 1
        self.c[cat][key + "|b"] += weight

    def out(self):
        res = {}
        for cat, ctr in sorted(self.c.items()):
            rows = collections.defaultdict(dict)
            for k, v in ctr.items():
                key, unit = k.rsplit("|", 1)
                rows[key]["units" if unit == "n" else "bytes"] = round(v)
            res[cat] = dict(sorted(rows.items()))
        return res


def family_of(row, package, same):
    """The census row's family entry for `package` (its family holds it), else None."""
    for f in row["families"]:
        if same(f["package"], package):
            return f
    return None


def resolve(cands, sig, row):
    """The one candidate family a signal confirms, else None."""
    ok = [f for f in cands if signal(f, sig, row) == "confirm"]
    return ok[0] if len(ok) == 1 else None


def classify(row, claim, vetoed, verdict, weight, tally, same):
    """Every decision `row`'s unit took part in, tallied per signal."""
    pick = row.get("pick")
    claimed = claim is not None
    if claimed and claim["rule"] == "seed" and (claim.get("evidence") or {}).get("kind") in ("index", "queueIndex"):
        fam = family_of(row, claim["package"], same)
        v = verdict(claim["package"])
        for s in SIGNALS:
            tally.add("claim", f"{s}:{signal(fam, s, row)}:{v}", weight)
        if pick and same(pick["package"], claim["package"]) and signal(fam, "fit", row) == "refute":
            for m in pick.get("missing", []):
                why = ("small" if m["nodes"] < 20 else "family-other-row" if m["inFamily"]
                       else "other-family" if m["indexed"] else "not-indexed")
                tally.add("claimRefuteCallees", f"{why}:{v}", weight)
    elif claimed and claim["rule"] != "seed":
        fam = family_of(row, claim["package"], same)
        v = verdict(claim["package"])
        for s in SIGNALS:
            tally.add("propagated", f"{s}:{signal(fam, s, row)}:{v}", weight)
    if vetoed is not None and not claimed:
        fam = family_of(row, vetoed, same)
        v = verdict(vetoed)
        for s in SIGNALS:
            tally.add("veto", f"{s}:{signal(fam, s, row)}:{v}", weight)
    if pick and not pick["supported"] and not claimed:
        v = verdict(pick["package"])
        for s in SIGNALS:
            st = signal({"fit": pick["familyFit"], "rowedFit": pick["familyRowedFit"], "tree": pick["tree"]}, s, row)
            tally.add("support", f"{s}:{st}:{v}", weight)
    if pick:
        tied = [f for f in row["families"] if f["covered"] == pick["covered"] and f["covered"] > 0]
        if len(tied) >= 2:
            today = verdict(pick["package"])
            for s in SIGNALS:
                r = resolve(tied, s, row)
                got = "unresolved" if r is None else verdict(r["package"])
                tally.add("tie", f"{s}:today-{today}:{got}:{'claimed' if claimed else 'unclaimed'}", weight)
    if not claimed and row["route"] == "whole":
        specific = [f for f in row["families"] if f["specific"]]
        if row["wholeNodes"] >= FLOOR and row["lookOwners"] > MAX_PACKAGES:
            for s in SIGNALS:
                r = resolve(specific, s, row)
                tally.add("common", f"{s}:{'unresolved' if r is None else verdict(r['package'])}", weight)
        if 20 <= row["wholeNodes"] < FLOOR and 0 < row["lookOwners"] <= MAX_PACKAGES:
            for s in SIGNALS:
                r = resolve(specific, s, row)
                tally.add("floor", f"{s}:{'unresolved' if r is None else verdict(r['package'])}", weight)


def main():
    argv = lib096.corpus_flag(["x", "--corpus", sys.argv[1]])
    del argv
    census_dir = sys.argv[2]
    baseline = sys.argv[sys.argv.index("--baseline") + 1] if "--baseline" in sys.argv else None
    import score  # noqa: E402  (after lib096.use: score reads the corpus at import)
    import truth_audit  # noqa: E402
    same = score.same_family
    manifest = lib096.load_manifest()
    tally = collections.defaultdict(Tally)
    checked = mismatched = 0
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    for entry in manifest["entries"]:
        doc = json.load(open(os.path.join(census_dir, entry["id"] + ".json")))
        report = doc["libraryDiscovery"]
        if report is None:
            continue
        if baseline:
            base = json.load(open(os.path.join(lib096.DATA, "results", baseline, "reports", entry["id"] + ".json")))
            checked += 1
            if base["libraryDiscovery"].get("claims") != report.get("claims"):
                mismatched += 1
        units = score.run_units(report)
        rows = {r["unit"]: r for r in report.get("callContext", [])}
        claims = {(c["start"], c["end"]): c for c in report.get("claims", [])}
        vetoed = {(v["start"], v["end"]): v["package"] for v in report.get("vetoed", [])}
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        t = tally[entry["set"]]
        if truth["method"] == "sample":
            by_s = {u["s"]: u for u in score.statement_units(units)}
            strata = truth["strata"]
            sb = collections.Counter()
            for tr in truth["rows"]:
                sb[tr["stratum"]] += tr["b"]
            for tr in truth["rows"]:
                u = by_s.get(tr["s"])
                if u is None or u["i"] not in rows:
                    continue
                w = strata[tr["stratum"]]["bytes"] * tr["b"] / sb[tr["stratum"]]

                def verdict(p, tr=tr):
                    if "pkg" in tr:
                        return "right" if same(p, tr["pkg"]) else "wrong"
                    return "app" if tr["label"] == "<app>" else "other"
                classify(rows[u["i"]], claims.get((u["s"], u["e"])), vetoed.get((u["s"], u["e"])), verdict, w, t,
                         same)
            continue
        origins = truth_audit.origin_layer(entry, om, o7m) if truth["method"] == "sourcemap" else None
        truth, origins = truth_audit.redecode(entry, truth, origins)
        labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        app_pkg = truth["appPackage"]
        for u in units:
            if u["i"] not in rows or u["i"] not in labels:
                continue
            lab = labels[u["i"]]

            def verdict(p, lab=lab):
                v, _ = score.judge_origin(p, lab, app_pkg)
                if v in ("right", "family", "origin"):
                    return "right"
                if v == "wrongFamily":
                    return "wrong"
                return "app" if v == "app" else "other"
            classify(rows[u["i"]], claims.get((u["s"], u["e"])), vetoed.get((u["s"], u["e"])), verdict, u["b"], t,
                     same)
    out = {"corpus": lib096.CORPUS, "censusDir": census_dir, "baseline": baseline,
           "claimsChecked": checked, "claimsMismatched": mismatched,
           "sets": {s: t.out() for s, t in sorted(tally.items())}}
    print(json.dumps(out, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
