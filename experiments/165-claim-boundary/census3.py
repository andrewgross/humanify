#!/usr/bin/env python3
"""exp165 support census: for every MOVED container (factory on the
whole-module key or a clean claim fold; lazy group on a clean fold), does
the answering family HOLD any print of the moved content?

The gate simulated (R-support): a move whose answering family holds ZERO
units at/over the 20-node row floor is refused; the statements return to
the normal naming flow.

Per moved factory: fold of its interior claims (None / contested / pkg);
answering family = fold if claimed else the family parsed from the
bundleOrder indexPath; units inside the factory's record span classified
held / sub-floor / no-family / cross-claim; bytes and truth (bun-dev).

Outputs the gate's cost table: factories blocked, bytes by class and
truth, at gate levels >=1 held unit, >=25%, >=50% held bytes.

usage: python3 -I census3.py <corpus>   # cc | bun-dev
"""
import collections
import json
import os
import sys

sys.path.insert(
    0, "/work/exp165-boundary/experiments/096-library-matching-score"
)
ROW_FLOOR = 20


def fold(pkgs):
    p = None
    for q in pkgs:
        if q is None:
            continue
        if p is None:
            p = q
        elif p != q:
            return "<contested>"
    return p


def truth_bundev(entry):
    import bisect
    import gzip

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
        if not votes:
            return None
        lab = votes.most_common(1)[0][0]
        return "app" if lab == "app" else ("library:" + lab)

    return at


def truth_cc():
    labels = {}
    with open("/work/cc-library-truth/2.1.216/labels.jsonl") as fh:
        for line in fh:
            l = json.loads(line)
            labels[(l["start"], l["end"])] = l["label"]
    return lambda s, e: labels.get((s, e))


def kind_of(lab):
    if lab is None:
        return "unknown"
    return lab if lab == "app" else "library"


class Census:
    def __init__(self, rep, truth_at):
        ld = rep["libraryDiscovery"]
        self.rows = ld.get("unitRows", [])
        self.claims = ld.get("claims", [])
        self.groups = ld.get("placementGroups", [])
        self.bo = rep.get("bundleOrder", [])
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
        self.fspan = {}
        for f in self.facs:
            bs = self.bodies.get(f, [])
            self.fspan[f] = (
                self.rows[f]["start"],
                max([self.rows[f]["end"]] + [self.rows[b]["end"] for b in bs]),
            )
        self.fspan_list = sorted((s, e, f) for f, (s, e) in self.fspan.items())
        self.fac_ord = {f: n for n, f in enumerate(self.facs)}

    def family_of_path(self, p):
        if not p:
            return None
        parts = p.split("/")
        return parts[1] if len(parts) > 2 and parts[0] == "vendor" else None

    def unit_class(self, i, fam):
        r = self.rows[i]
        mx = max([r["whole"]["nodes"] if r.get("whole") else 0]
                 + [f["nodes"] for f in r.get("functions", [])])
        b = r["bytes"]
        lab, kind = self.truth_at(r["start"], r["end"]), None
        kind = kind_of(lab)
        if mx < ROW_FLOOR:
            return ("sub-floor", b, kind)
        fams = self.ctx.get(i, {})
        if not fams:
            return ("no-family", b, kind)
        if fam in fams:
            return ("held", b, kind)
        return ("cross-claim", b, kind)

    def container_stats(self, spans, fam):
        """units inside the union of (s,e) spans; returns per-class bytes."""
        cs = collections.Counter()
        cb = collections.Counter()
        hb = 0
        held_units = 0
        over_units = 0
        for s, e in spans:
            for i, r in enumerate(self.rows):
                if not (s <= r["start"] and r["end"] <= e):
                    continue
                cls, b, kind = self.unit_class(i, fam)
                cs[(cls, kind)] += 1
                cb[(cls, kind)] += b
                if cls == "held":
                    hb += b
                    held_units += 1
                if cls != "sub-floor":
                    over_units += 1
        return cs, cb, hb, held_units, over_units

    def factories(self):
        """moved factories with fold + answering family."""
        for f in self.facs:
            n = self.fac_ord.get(f)
            if n is None or n >= len(self.bo):
                continue
            row = self.bo[n]
            if not row.get("indexPath"):
                continue
            fs, fe = self.fspan[f]
            inside = [c["package"] for c in self.claims
                      if fs <= c["start"] and c["end"] <= fe]
            fl = fold(inside)
            fam = fl if fl not in (None, "<contested>") else self.family_of_path(row["indexPath"])
            if fam is None:
                continue
            yield f, (fs, fe), fl, fam, row["indexPath"]

    def clean_groups(self):
        for g in self.groups:
            pkgs = [self.claim_at.get((st["start"], st["end"]), [{}])[0].get("package")
                    for st in g["statements"]]
            f = fold(pkgs)
            if f is None or f == "<contested>":
                continue
            if any(p is None for p in pkgs):
                continue  # partial: does not move (lazy)
            yield g, f


def summarize(title, per_gate):
    print(f"--- {title} ---")
    for gate, c in per_gate.items():
        tot = sum(v for k, v in c.items() if k[1] == "b" or k[0].endswith("_b"))
        print(f"  gate {gate}:")
        for k in sorted(c, key=str):
            print(f"    {k}: {c[k]}")


def main():
    corpus = sys.argv[1] if len(sys.argv) > 1 else "cc"
    # gates: >=N held units; G2 = verifiable (has over-floor units) and zero
    # held; fracs = held-bytes share
    gates = (1, 2, "G2", 0.25, 0.5)
    out = collections.Counter()
    nb = 0
    if corpus == "cc":
        reps = ["/work/exp165-data/discover-cc-216.json", "/work/exp165-data/discover-cc-215.json"]
        truths = [truth_cc(), None]  # 215 has no labels; skip truth there
    else:
        entries = {e["id"]: e for e in json.load(open(
            "/work/exp165-boundary/experiments/096-library-matching-score/corpus-bun-dev.json"
        ))["entries"]}
        import lib096
        lib096.use("bun-dev")
        reps = [f"/work/exp165-data/reports/{line.split(chr(9))[0]}.json"
                for line in open("/work/exp165-data/bun-dev-list.tsv")]
        truths = None
    for ri, path in enumerate(reps):
        if not os.path.exists(path):
            continue
        if corpus == "cc":
            rep = json.load(open(path))
            c = Census(rep, truth_cc())
            pool = [(rep, c)]
        else:
            rep = json.load(open(path))
            bid = os.path.basename(path)[:-5]
            c = Census(rep, truth_bundev(entries[bid]))
            pool = [(rep, c)]
        for rep, c in pool:
            nb += 1
            for f, (fs, fe), fl, fam, ipath in c.factories():
                cs, cb, hb, hu, ou = c.container_stats([(fs, fe)], fam)
                tot = sum(cb.values())
                gate_block = {}
                for g in gates:
                    if g == "G2":
                        gate_block[g] = hu == 0 and ou > 0
                    elif isinstance(g, int):
                        gate_block[g] = hu < g
                    else:
                        gate_block[g] = tot > 0 and hb / tot < g
                moved_kind = "claimed-fold" if fl not in (None, "<contested>") else "whole-module"
                for g, blocked in gate_block.items():
                    if not blocked:
                        continue
                    out[(g, moved_kind, "factories")] += 1
                    for k, v in cb.items():
                        out[(g, moved_kind, k[0] + "_b")] += v
                        out[(g, moved_kind, k[0] + "_" + k[1] + "_b")] += v
                    out[(g, moved_kind, "bytes_b")] += tot
            for g, fam in c.clean_groups():
                spans = [(st["start"], st["end"]) for st in g["statements"]]
                cs, cb, hb, hu, ou = c.container_stats(spans, fam)
                tot = sum(cb.values())
                for gt in gates:
                    blocked = ((hu == 0 and ou > 0) if gt == "G2"
                               else (hu < gt) if isinstance(gt, int)
                               else (tot > 0 and hb / tot < gt))
                    if not blocked:
                        continue
                    out[(gt, "lazy-clean", "groups")] += 1
                    for k, v in cb.items():
                        out[(gt, "lazy-clean", k[0] + "_b")] += v
                        out[(gt, "lazy-clean", k[0] + "_" + k[1] + "_b")] += v
                    out[(gt, "lazy-clean", "bytes_b")] += tot
            # baseline: ALL moved containers (no gate) for comparison
            for f, (fs, fe), fl, fam, ipath in c.factories():
                cs, cb, hb, hu, ou = c.container_stats([(fs, fe)], fam)
                moved_kind = "claimed-fold" if fl not in (None, "<contested>") else "whole-module"
                out[("none", moved_kind, "factories")] += 1
                for k, v in cb.items():
                    out[("none", moved_kind, k[0] + "_b")] += v
                    out[("none", moved_kind, k[0] + "_" + k[1] + "_b")] += v
            for g, fam in c.clean_groups():
                spans = [(st["start"], st["end"]) for st in g["statements"]]
                cs, cb, hb, hu, ou = c.container_stats(spans, fam)
                out[("none", "lazy-clean", "groups")] += 1
                for k, v in cb.items():
                    out[("none", "lazy-clean", k[0] + "_b")] += v
                    out[("none", "lazy-clean", k[0] + "_" + k[1] + "_b")] += v
    print(f"bundles {nb}")
    bygate = collections.defaultdict(collections.Counter)
    for k, v in out.items():
        bygate[k[0]][k[1:]] = v
    for gate in ("none",) + tuple(gates):
        gk = gate
        print(f"--- gate {gk} ---")
        for k in sorted(bygate[gk], key=str):
            print(f"  {k}: {bygate[gk][k]}")


if __name__ == "__main__":
    main()
