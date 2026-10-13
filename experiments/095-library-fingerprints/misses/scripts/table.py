"""exp095 "misses": the decomposition table per test set — byte shares of library statement bytes per primary cause,
class d split by the diff shape (dsub.py), with bootstrap 95% intervals (held-out: resampling APPS; wrangler:
resampling units; Claude Code: stratified resampling of the labelled sample).

usage: <venv python> -I table.py   -> $MM/decomp/table.json and a printed table
"""
import collections
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dsub import sub  # noqa: E402
from mmlib import MM  # noqa: E402

B = 1000


def load(which):
    rows = [json.loads(l) for l in open(f"{MM}/decomp/{which}.rows.jsonl")]
    dsubs = {}
    for r in map(json.loads, open(f"{MM}/diffd/{which}.jsonl")):
        dsubs[(r.get("app"), r["s"])] = sub(r)
    for r in rows:
        if r["cls"].startswith("d-"):
            s = dsubs.get((r.get("app"), r["s"]), "undiffed")
            # the brief's d sub-classes; file-level outcomes keep the decomposition's own label
            if s == "no-counterpart":
                s = {"d-variant": "other-file", "d-absent": "not-emitted"}.get(r["cls"], "no-counterpart")
            elif r["cls"] == "d-variant" and s != "row-missing":
                s = "other-file"
            elif r["cls"] == "d-absent":
                s = "not-emitted"
            r["cls"] = "d:" + s
    return rows


def shares(rows, weight=None):
    c = collections.Counter()
    for r in rows:
        c[r["cls"]] += r["b"] * (weight(r) if weight else 1)
    n = sum(c.values())
    return {k: v / n for k, v in c.items()}, n


def boot(groups, fn=None, seed=95):
    """groups: list of row lists (the resampling unit) -> {class: (lo, hi)} of byte shares."""
    rnd = random.Random(seed)
    cs = []
    for g in groups:
        c = collections.Counter()
        for r in g:
            c[r["cls"]] += r["b"]
        cs.append(c)
    keys = set().union(*cs) if cs else set()
    draws = collections.defaultdict(list)
    for _ in range(B):
        tot = collections.Counter()
        for _ in cs:
            tot.update(cs[rnd.randrange(len(cs))])
        n = sum(tot.values()) or 1
        for k in keys:
            draws[k].append(tot.get(k, 0) / n)
    return {k: (sorted(v)[int(0.025 * B)], sorted(v)[int(0.975 * B) - 1]) for k, v in draws.items()}


def cc_estimator(meta):
    """The negatives lane's stratified ratio estimator: a stratum's sampled bytes (ALL units, library or not)
    stand for its population bytes."""
    neg = "/work/exp095-library-fingerprints/negatives"
    sb = collections.Counter()
    for u in map(json.loads, open(f"{neg}/labels.jsonl")):
        if u["stratum"] in meta["strata"]:
            sb[u["stratum"]] += u["bytes"]

    def fn(rows):
        c = collections.Counter()
        for r in rows:
            c[r["cls"]] += meta["strata"][r["stratum"]]["bytes"] * r["b"] / sb[r["stratum"]]
        n = sum(c.values())
        return {k: v / n for k, v in c.items()}
    return fn


def cc_boot(rows, fn, seed=95):
    """Stratified bootstrap over ALL sampled units (library and not): resample each stratum's units, keep the
    library ones (the library share of a stratum varies too)."""
    neg = "/work/exp095-library-fingerprints/negatives"
    meta = json.load(open(f"{neg}/data/sample-meta.json"))
    allu = [json.loads(l) for l in open(f"{neg}/labels.jsonl")]
    allu = [u for u in allu if u["stratum"] in meta["strata"]]
    by_start = {r["s"]: r for r in rows}
    strata = collections.defaultdict(list)
    for u in allu:
        strata[u["stratum"]].append(u)
    rnd = random.Random(seed)
    draws = collections.defaultdict(list)
    for _ in range(B):
        pick = []
        sizes = {}
        for h, us in strata.items():
            p = [us[rnd.randrange(len(us))] for _ in us]
            sizes[h] = sum(u["bytes"] for u in p)
            pick += [by_start[u["start"]] for u in p if u["start"] in by_start]
        c = collections.Counter()
        for r in pick:
            c[r["cls"]] += meta["strata"][r["stratum"]]["bytes"] * r["b"] / sizes[r["stratum"]]
        n = sum(c.values()) or 1
        for k in set(c) | set(draws):
            draws[k].append(c.get(k, 0) / n)
    return {k: (sorted(v)[int(0.025 * B)], sorted(v)[int(0.975 * B) - 1]) for k, v in draws.items()}


def main():
    out = {}
    meta = json.load(open("/work/exp095-library-fingerprints/negatives/data/sample-meta.json"))
    for which in ("cc", "heldout", "wrangler"):
        rows = load(which)
        for rule in ("struct", "agree"):
            rs = [r for r in rows if r["rule"] == rule]
            if which == "cc":
                pt = cc_estimator(meta)(rs)
                ci = cc_boot(rs, None)
                n = sum(r["b"] for r in rs)
            else:
                pt, n = shares(rs)
                if which == "heldout":
                    g = collections.defaultdict(list)
                    for r in rs:
                        g[r["app"]].append(r)
                    groups = list(g.values())
                else:
                    groups = [[r] for r in rs]
                ci = boot(groups, lambda x: shares(x)[0])
            out[f"{which}/{rule}"] = {"libraryBytes": n, "shares": pt, "ci": ci}
            if which == "heldout":  # the same, restricted to packages the index holds
                ins = [r for r in rs if r["cls"] != "a"]
                pt2, n2 = shares(ins)
                g = collections.defaultdict(list)
                for r in ins:
                    g[r["app"]].append(r)
                out[f"{which}-inindex/{rule}"] = {"libraryBytes": n2, "shares": pt2,
                                                   "ci": boot(list(g.values()), lambda x: shares(x)[0])}
    json.dump(out, open(f"{MM}/decomp/table.json", "w"), indent=1)
    for k, v in out.items():
        print(f"== {k}  library bytes {v['libraryBytes']:,.0f}")
        for c, s in sorted(v["shares"].items(), key=lambda x: -x[1]):
            lo, hi = v["ci"].get(c, (0, 0))
            print(f"   {c:22s} {s:6.1%}  [{lo:5.1%} – {hi:5.1%}]")


if __name__ == "__main__":
    main()
