#!/usr/bin/env python3
"""exp165 census 4: the R-support rule simulated exactly as it would build.

RULE: a factory whose interior claims fold to NONE (the whole-module key
is the only support) moves only if the answering family holds a print of
at least one over-floor unit among the dragged module's statements (the
placement group containing the factory's span). If the group carries
over-floor units and NO family holds any of them (all no-family), the
move is refused.

Measured per corpus: factories blocked, their dragged group bytes by
class and truth, and the answering-family held share of what stays.
"""
import collections
import json
import os
import sys

sys.path.insert(0, "/work/exp165-boundary/experiments/096-library-matching-score")
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
        return "app" if lab == "app" else "library"


    return at


def truth_cc():
    labels = {}
    with open("/work/cc-library-truth/2.1.216/labels.jsonl") as fh:
        for line in fh:
            l = json.loads(line)
            labels[(l["start"], l["end"])] = l["label"]
    return lambda s, e: labels.get((s, e))


class Census:
    def __init__(self, rep, truth_at):
        ld = rep["libraryDiscovery"]
        self.rows = ld.get("unitRows", [])
        self.claims = ld.get("claims", [])
        self.groups = ld.get("placementGroups", [])
        self.bo = rep.get("bundleOrder", [])
        self.truth_at = truth_at
        self.ctx = {}
        for cc in ld.get("callContext", []):
            self.ctx[cc["unit"]] = {f["package"]: f["covered"] for f in cc["families"]}
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
        self.fac_ord = {f: n for n, f in enumerate(self.facs)}

    def family_of_path(self, p):
        if not p:
            return None
        parts = p.split("/")
        return parts[1] if len(parts) > 2 and parts[0] == "vendor" else None

    def group_of(self, s, e):
        """the placement group whose statements contain the factory span."""
        best = None
        for g in self.groups:
            lo = min(st["start"] for st in g["statements"])
            hi = max(st["end"] for st in g["statements"])
            if lo <= s and e <= hi:
                size = hi - lo
                if best is None or size < best[0]:
                    best = (size, g)
        return best[1] if best else None

    def group_units(self, g):
        """units of the group's statements (span-contained)."""
        for st in g["statements"]:
            s, e = st["start"], st["end"]
            for i, r in enumerate(self.rows):
                if s <= r["start"] and r["end"] <= e:
                    yield i, r

    def run(self, out):
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
            if fl not in (None, "<contested>"):
                continue  # claimed-fold move: claim support, rule not applied
            fam = self.family_of_path(row["indexPath"])
            g = self.group_of(fs, fe)
            key = "nogroup" if g is None else "grouped"
            out[(key, "factories")] += 1
            if g is None:
                continue
            hb = 0
            nf_b = 0
            nf_n = 0
            classes = collections.Counter()
            truthc = collections.Counter()
            tot = 0
            for i, r in self.group_units(g):
                mx = max([r["whole"]["nodes"] if r.get("whole") else 0]
                         + [fd["nodes"] for fd in r.get("functions", [])])
                b = r["bytes"]
                tot += b
                lab = self.truth_at(r["start"], r["end"])
                truthc[("app" if lab == "app" else "library" if lab else "unknown")] += b
                if mx < ROW_FLOOR:
                    classes["sub-floor"] += b
                    continue
                fams = self.ctx.get(i, {})
                if not fams:
                    classes["no-family"] += b
                    nf_b += b
                    nf_n += 1
                elif fam in fams:
                    classes["held"] += b
                    hb += b
                else:
                    classes["cross-claim"] += b
            out[(key, "bytes")] += tot
            for k, v in classes.items():
                out[(key, k + "_b")] += v
            for k, v in truthc.items():
                out[(key, "truth_" + k + "_b")] += v
            blocked = hu0_and_no_family(hb, nf_n)
            if blocked:
                out[(key, "BLOCKED")] += 1
                out[(key, "BLOCKED_b")] += tot
                for k, v in classes.items():
                    out[(key, "BLOCKED_" + k + "_b")] += v
                for k, v in truthc.items():
                    out[(key, "BLOCKED_truth_" + k + "_b")] += v


def hu0_and_no_family(hb, nf_n):
    """the rule: answering family holds nothing AND the group carries
    over-floor content no family holds at all."""
    return hb == 0 and nf_n > 0


def main():
    corpus = sys.argv[1]
    out = collections.Counter()
    if corpus == "cc":
        for path in ("/work/exp165-data/discover-cc-216.json",
                     "/work/exp165-data/discover-cc-215.json"):
            rep = json.load(open(path))
            Census(rep, truth_cc()).run(out)
    else:
        entries = {e["id"]: e for e in json.load(open(
            "/work/exp165-boundary/experiments/096-library-matching-score/corpus-bun-dev.json"
        ))["entries"]}
        for line in open("/work/exp165-data/bun-dev-list.tsv"):
            bid = line.rstrip("\n").split("\t")[0]
            path = f"/work/exp165-data/reports/{bid}.json"
            if not os.path.exists(path):
                continue
            Census(json.load(open(path)), truth_bundev(entries[bid])).run(out)
    print(f"corpus {corpus}")
    for k in sorted(out, key=str):
        print(f"  {k}: {out[k]}")


if __name__ == "__main__":
    main()
