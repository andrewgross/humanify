#!/usr/bin/env python3
"""exp165 census, full pass: claimed-region composition, stranger classes,
truth-based app share, and the boundary-rule trade table.

Inputs per bundle:
  <report.json>   a `humanify discover` report (--unit-prints --call-context
                  --bundle-order, names-mode config, the names index)
  truth           the frozen truth for the corpus (bun-dev: lib096's
                  segment table; cc: the 620-unit labelled sample)

Populations (where claims land):
  lazy-group   a placement group's statement (the lazy modules; a clean fold
               joins the era's fossil modules and MOVES -> the freeze)
  factory      inside a factory's record span (moved at the split on the
               folded claim or the whole-module key)
  init-other   any other claim (top-level init/registration code)

Stranger classes (the claimed family holds nothing of the unit):
  sub-floor    every print under the 20-node row floor (can't tell)
  no-family    prints exist, no family holds any (app-shaped or too-small)
  cross-claim  other families hold it
  unclaimed    no claim (interior of a moved container)

Trade table rules (per rule: ejected bytes vs family bytes also ejected):
  R-eject      every stranger (claimed-not-held AND unclaimed interior)
               inside a moved container returns to the default naming path
  R-eject-claim  only CLAIMED strangers return (unclaimed interior stays)
  R-composition  a moved container whose stranger bytes exceed the
               threshold does not move wholesale (family bytes go with it)
  R-fold-min   a factory whose fold rests on fewer than N claims / less
               than X% of its body bytes does not move

usage: python3 -I census2.py <corpus>   # corpus = cc | bun-dev
"""
import bisect
import collections
import gzip
import json
import os
import sys

sys.path.insert(
    0, "/work/exp165-boundary/experiments/096-library-matching-score"
)  # for lib096 (python3 -I ignores PYTHONPATH)

ROW_FLOOR = 20  # libindex::units::UNIT_ROW_MIN_NODES
BUNDLE_CC = (
    "/Users/andrewgross/Development/claude-code-versions/inputs/"
    "claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js"
)
TRUTH_CC = "/work/cc-library-truth/2.1.216/labels.jsonl"


def fold(pkgs):
    """library_layout::fold_claims: all-agree passes, disagreement folds to
    the sentinel, silence stays None."""
    p = None
    for q in pkgs:
        if q is None:
            continue
        if p is None:
            p = q
        elif p != q:
            return "<contested>"
    return p


def load_report(path):
    with open(path) as fh:
        return json.load(fh)


# ---------------------------------------------------------------- truth ---
def truth_bundev(entry):
    """The frozen truth's segment table: per offset, the source package."""
    import lib096

    lib096.use("bun-dev")
    p = lib096.truth_path(entry)
    with gzip.open(p, "rt") as fh:
        t = json.load(fh)
    segs = t["segments"]
    srcs = [s[0] for s in t["sources"]]
    offs = [s[0] for s in segs]

    def at(s, e):
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        votes = collections.Counter()
        while k < len(segs) and segs[k][0] < e:
            a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                votes[srcs[segs[k][1]]] += b - a
            k += 1
        return votes.most_common(1)[0][0] if votes else "<unmapped>"

    return at


def truth_cc():
    labels = {}
    with open(TRUTH_CC) as fh:
        for line in fh:
            l = json.loads(line)
            labels[(l["start"], l["end"])] = l["label"]
    return lambda s, e: labels.get((s, e))


# --------------------------------------------------------------- census ---
class Census:
    def __init__(self, report, truth_at):
        ld = report["libraryDiscovery"]
        self.ld = ld
        self.rows = ld.get("unitRows", [])
        self.claims = ld.get("claims", [])
        self.groups = ld.get("placementGroups", [])
        self.bo = report.get("bundleOrder", [])
        self.truth_at = truth_at
        self.idx = {(r["start"], r["end"]): i for i, r in enumerate(self.rows)}
        self.ctx = {}
        for cc in ld.get("callContext", []):
            self.ctx[cc["unit"]] = {f["package"]: f["covered"] for f in cc["families"]}
        self.claim_at = collections.defaultdict(list)
        for c in self.claims:
            self.claim_at[(c["start"], c["end"])].append(c)
        self.facs = [i for i, r in enumerate(self.rows) if r["kind"] == "Factory"]
        self.bodies = collections.defaultdict(list)
        for i, r in enumerate(self.rows):
            if r["kind"] == "FactoryBody" and r.get("module") is not None:
                self.bodies[r["module"]].append(i)
        # a factory's full record span (head row + body rows)
        self.fspan = {}
        for f in self.facs:
            bs = self.bodies.get(f, [])
            self.fspan[f] = (
                self.rows[f]["start"],
                max([self.rows[f]["end"]] + [self.rows[b]["end"] for b in bs]),
            )
        self.fspan_list = sorted((s, e, f) for f, (s, e) in self.fspan.items())
        # bundleOrder rows are in factory order, not unit order
        self.fac_ord = {f: n for n, f in enumerate(self.facs)}

    def factory_placed(self, f):
        n = self.fac_ord.get(f)
        return n is not None and n < len(self.bo) and bool(self.bo[n].get("indexPath"))

    # -- primitives
    def hold(self, i, pkg):
        r = self.rows[i]
        mx = max([r["whole"]["nodes"] if r.get("whole") else 0]
                 + [f["nodes"] for f in r.get("functions", [])])
        if mx < ROW_FLOOR:
            return "sub-floor"
        fams = self.ctx.get(i, {})
        if not fams:
            return "no-family"
        return "held" if pkg in fams else "cross-claim"

    def truth(self, s, e):
        lab = self.truth_at(s, e)
        if lab is None:
            return None, "unlabelled"
        kind = "app" if lab == "app" else ("library" if not lab.startswith("<") else "other")
        return lab, kind

    def population(self, s, e):
        for fs, fe, f in self.fspan_list:
            if fs <= s and e <= fe:
                return "factory", f
        return "lazy-group", None

    # -- lazy groups
    def group_rows(self):
        """per placement group: verdict + member unit info."""
        for g in self.groups:
            rowpkgs = [s.get("package") for s in g["statements"]]
            f = fold(rowpkgs)
            if f is None:
                verdict = "unclaimed"
            elif f == "<contested>":
                verdict = "contested"
            else:
                verdict = "clean" if all(p is not None for p in rowpkgs) else "partial"
            yield g, verdict, f

    # -- factories
    def factory_fold(self, f):
        fs, fe = self.fspan[f]
        inside = [c for c in self.claims if fs <= c["start"] and c["end"] <= fe]
        return fold([c["package"] for c in inside]), inside

    # -- the trade table
    def trade(self):
        """Per rule: stranger bytes ejected vs family bytes also ejected."""
        t = collections.Counter()

        def bump(rule, kind, b):
            t[(rule, kind)] += b

        # lazy groups: only clean groups can move (partial/contested stay;
        # unclaimed moves only on a clean index answer, which lazy regions
        # never get - plan_library_modules passes an empty answers map)
        for g, verdict, f in self.group_rows():
            if verdict != "clean":
                continue
            pkg = f
            for s in g["statements"]:
                i = self.idx.get((s["start"], s["end"]))
                if i is None:
                    continue
                b = self.rows[i]["bytes"]
                lab, kind = self.truth(s["start"], s["end"])
                hs = self.hold(i, pkg)
                if hs == "held":
                    bump("R-eject", "family_" + kind, b)
                    bump("R-eject-claim", "family_" + kind, b)
                    bump("R-composition", "family_" + kind, b)
                else:
                    bump("R-eject", "stranger_" + kind, b)
                    bump("R-eject-claim", "stranger_" + kind, b)
                    bump("R-composition", "stranger_" + kind, b)
        # factories: clean fold moves; unclaimed fold moves only on the
        # whole-module key (the index answer the discover verb already
        # resolved: indexPath present on an unclaimed fold)
        for f in self.facs:
            fl, inside = self.factory_fold(f)
            placed = self.factory_placed(f)
            if not placed:
                continue
            movable = fl not in (None, "<contested>")
            fs, fe = self.fspan[f]
            # every unit row inside the factory's span
            for i, r in enumerate(self.rows):
                if not (fs <= r["start"] and r["end"] <= fe):
                    continue
                b = r["bytes"]
                lab, kind = self.truth(r["start"], r["end"])
                cs = self.claim_at.get((r["start"], r["end"]), [])
                pk = cs[0]["package"] if cs else None
                hs = "unclaimed" if pk is None else self.hold(i, pk)
                # R-eject: strangers inside moved factories (all of them move)
                if hs == "held":
                    bump("R-eject", "family_" + kind, b)
                    bump("R-eject-claim", "family_" + kind, b)
                else:
                    bump("R-eject", "stranger_" + kind, b)
                    bump("R-eject-claim", "stranger_" + kind, b)
                # R-composition / R-fold-min only touch clean-fold factories
                if movable:
                    if hs == "held":
                        bump("R-composition", "family_" + kind, b)
                    else:
                        bump("R-composition", "stranger_" + kind, b)
                    cl = "claim" if pk is not None else "nointerior"
                    if hs == "held":
                        bump("R-fold-min", "family_" + kind, b)
                    else:
                        bump("R-fold-min", "stranger_" + kind, b)
        return t

    # -- the composition tables
    def composition(self):
        out = collections.Counter()
        # lazy groups by verdict
        for g, verdict, f in self.group_rows():
            out[("group", verdict)] += 1
            if verdict == "unclaimed":
                continue
            pkg = f if f != "<contested>" else None
            for s in g["statements"]:
                i = self.idx.get((s["start"], s["end"]))
                if i is None:
                    out[("group", verdict, "nounit")] += 1
                    continue
                b = self.rows[i]["bytes"]
                lab, kind = self.truth(s["start"], s["end"])
                hs = self.hold(i, pkg) if pkg else "no-claim"
                out[("group", verdict, hs)] += 1
                out[("group", verdict, hs + "_b")] += b
                out[("group", verdict, hs + "_" + kind)] += 1
                out[("group", verdict, hs + "_" + kind + "_b")] += b
        # factories
        for f in self.facs:
            fl, _inside = self.factory_fold(f)
            placed = self.factory_placed(f)
            verdict = ("clean" if fl not in (None, "<contested>")
                       else "contested" if fl == "<contested>" else "unclaimed")
            fs, fe = self.fspan[f]
            out[("factory", "moved" if placed else "stay", verdict)] += 1
            for i, r in enumerate(self.rows):
                if not (fs <= r["start"] and r["end"] <= fe):
                    continue
                b = r["bytes"]
                lab, kind = self.truth(r["start"], r["end"])
                cs = self.claim_at.get((r["start"], r["end"]), [])
                pk = cs[0]["package"] if cs else None
                hs = "unclaimed" if pk is None else self.hold(i, pk)
                key = ("factory", "moved" if placed else "stay", verdict if placed else "stay")
                out[key + (hs,)] += 1
                out[key + (hs + "_b",)] += b
                out[key + (hs + "_" + kind,)] += 1
                out[key + (hs + "_" + kind + "_b",)] += b
        return out


def report_table(title, counts):
    print(f"--- {title} ---")
    for k in sorted(counts, key=str):
        print(f"  {k}: {counts[k]}")


def main():
    corpus = sys.argv[1] if len(sys.argv) > 1 else "cc"
    totals = collections.Counter()
    trade = collections.Counter()
    nbundles = 0
    if corpus == "cc":
        rep = load_report("/work/exp165-data/discover-cc-216.json")
        c = Census(rep, truth_cc())
        report_table("cc composition", c.composition())
        report_table("cc trade", c.trade())
        return
    entries = {e["id"]: e for e in json.load(
        open("/work/exp165-boundary/experiments/096-library-matching-score/corpus-bun-dev.json")
    )["entries"]}
    import lib096

    lib096.use("bun-dev")
    list_path = "/work/exp165-data/bun-dev-list.tsv"
    for line in open(list_path):
        bid, bundle, _bundler = line.rstrip("\n").split("\t")
        path = f"/work/exp165-data/reports/{bid}.json"
        if not os.path.exists(path):
            continue
        rep = load_report(path)
        c = Census(rep, truth_bundev(entries[bid]))
        nbundles += 1
        totals.update(c.composition())
        trade.update(c.trade())
    print(f"bundles {nbundles}")
    report_table("bun-dev composition (aggregated)", totals)
    report_table("bun-dev trade (aggregated)", trade)


if __name__ == "__main__":
    main()
