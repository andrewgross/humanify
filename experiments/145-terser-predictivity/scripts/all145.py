"""exp145: every historical terser candidate's DEV deltas, 36-app subsample distributions, and candidate dev bars.

usage: python3 all145.py <out.json>
DEV labels only (remin, remin3 and the other dev corpora for the event-size stress). Judge outcomes are the
remin2-terser summaries already quoted in /work/f1-campaign/STATE.md; no judge card is read here.
Writes app-free, package-free numbers (per-app rows are reduced to sorted deltas)."""
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from bootstrap145 import cards, pct, prec  # noqa: E402
from pairs145 import load  # noqa: E402

R145 = "/work/exp145-terser/experiments/096-library-matching-score/results"  # re-scored here (scorer 14)
RX = "/work/exp144-veto/experiments/096-library-matching-score/results"  # exp142 / exp144 labels (scorer 14)

# name: (judge remin2-terser dP in points as quoted, verdict, {corpus: (results, base, cand)}, [other dev label pairs])
CANDS = {
    "exp140 terser_slices alone": (-0.257, "FAIL", {"remin3": (R145, "remin3-exp141-main", "remin3-exp141-terser"),
                                                    "remin": (R145, "remin-exp135-prod", "remin-exp138-terser")}),
    "exp140 terser_slices + exp139 keys": (-0.194, "FAIL", {"remin3": (R145, "remin3-exp141-main", "remin3-exp141-both"),
                                                            "remin": (R145, "remin-exp138-helper", "remin-exp139-both")}),
    "exp141 + terser_anchor=all": (-0.150, "FAIL", {"remin3": (R145, "remin3-exp141-main", "remin3-exp141-x"),
                                                    "remin": (R145, "remin-exp138-helper", "remin-exp141-x")}),
    "exp144 veto_support=shown": (-0.069, "FAIL", {"remin3": (RX, "remin3-exp142-asof", "remin3-exp144-shown2"),
                                                   "remin": (RX, "remin-exp142-asof", "remin-exp144-shown2")}),
    "exp142 as-of index (control)": (None, "PASS", {"remin3": (RX, "remin3-exp142-base", "remin3-exp142-asof"),
                                                    "remin": (RX, "remin-exp142-base", "remin-exp142-asof")}),
}
# every dev label pair of the candidate (for the largest single-bundle wrong event anywhere on dev)
WIDE = {
    "exp144 veto_support=shown": [(RX, f"{p}exp142-asof{s}", f"{p}exp144-shown2{s}") for p, s in
                                  [("", ""), ("", "-apps"), ("", "-bundlers"), ("pop-", "")]],
    "exp142 as-of index (control)": [(RX, f"{p}exp142-base{s}", f"{p}exp142-asof{s}") for p, s in
                                     [("", ""), ("", "-apps"), ("", "-bundlers"), ("pop-", "")]],
}
N = 20000
K = 36


def rows(results, base, cand, set_):
    b, c = cards(results, base, set_), cards(results, cand, set_)
    apps = sorted(set(b) & set(c))
    assert set(b) == set(c), (set_, base, cand)
    return [(b[a], c[a]) for a in apps]


def dp(rs):
    return prec([r[1] for r in rs]) - prec([r[0] for r in rs])


def boot(rs, seed=145):
    rng = random.Random(seed)
    replace = len(rs) <= K
    ds = [dp([rng.choice(rs) for _ in range(K)] if replace else rng.sample(rs, K)) for _ in range(N)]
    return {"p05": pct(ds, 0.05), "p50": pct(ds, 0.5), "p95": pct(ds, 0.95),
            "below0": sum(1 for d in ds if d < 0) / N, "replace": replace}


def stats(rs):
    dw = sorted(((c[1] + c[2]) - (b[1] + b[2]) for b, c in rs), reverse=True)
    added = sum(x for x in dw if x > 0)
    nb = sum(b[0] for b, _ in rs)
    wb = sum(b[1] + b[2] for b, _ in rs)
    dn = sum(c[0] - b[0] for b, c in rs)
    return {"apps": len(rs), "P_base": nb / (nb + wb), "dP": dp(rs), "dNamedKB": dn / 1e3, "dWrongKB": sum(dw) / 1e3,
            "addedWrongKB": added / 1e3, "appsWrongUp": sum(1 for x in dw if x > 0),
            "top1": sum(dw[:1]) / added if added else 0, "top3": sum(x for x in dw[:3] if x > 0) / added if added else 0,
            "top5": sum(x for x in dw[:5] if x > 0) / added if added else 0, "maxAppAddKB": max(dw[0], 0) / 1e3,
            "headroomKB": ((1 - nb / (nb + wb)) * dn - sum(dw)) / 1e3}


def wide_max(name, own):
    m = max(s["maxAppAddKB"] for s in own)
    for results, base, cand in WIDE.get(name, []):
        b, c = load(results, base), load(results, cand)
        for s in b:
            for a in b[s]:
                m = max(m, (c[s][a][1] - b[s][a][1]) / 1e3)
    return m


def main(out):
    res = {}
    for name, (jdp, verdict, corp) in CANDS.items():
        entry = {"judge_dP": jdp, "judge": verdict, "sets": {}}
        pooled = []
        for corpus, (results, base, cand) in corp.items():
            for kind in ("terser", "esbuild"):
                set_ = f"{corpus}-{kind}"
                rs = rows(results, base, cand, set_)
                entry["sets"][set_] = {**stats(rs), "boot": boot(rs)}
                if kind == "terser":
                    pooled += rs
        entry["pooled-terser"] = {**stats(pooled), "boot": boot(pooled)}
        own = [v for v in entry["sets"].values()]
        entry["maxAppAddAnyDevKB"] = wide_max(name, own)
        res[name] = entry
    json.dump(res, open(out, "w"), indent=1, sort_keys=True)
    hdr = f"{'candidate':<36}{'set':<16}{'apps':>5}{'dP pts':>9}{'dNamed KB':>11}{'dWrong KB':>10}{'top1':>6}{'top3':>6}{'top5':>6}{'p05':>9}{'p50':>9}{'<0':>7}"
    print(hdr)
    for name, e in res.items():
        for set_, s in list(e["sets"].items()) + [("pooled-terser", e["pooled-terser"])]:
            b = s["boot"]
            print(f"{name:<36}{set_:<16}{s['apps']:>5}{100*s['dP']:>9.3f}{s['dNamedKB']:>11.0f}{s['dWrongKB']:>10.1f}"
                  f"{s['top1']:>6.2f}{s['top3']:>6.2f}{s['top5']:>6.2f}{100*b['p05']:>9.3f}{100*b['p50']:>9.3f}{b['below0']:>7.3f}")
        print(f"{'':<36}judge remin2-terser {e['judge']} dP {e['judge_dP']}; largest single-app wrong add on any dev set {e['maxAppAddAnyDevKB']:.1f} KB")


if __name__ == "__main__":
    main(sys.argv[1])
