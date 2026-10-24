"""exp149: carrier index + a bundle-level tie vote, projected per set against main on the recommended index.

Start from the carrier label's card; move the tied units the carrier run CLAIMED under the vote (as sim.py), and
let the vote name tied units the carrier run left UNCLAIMED, in two bounds:
  refused   only units the base label (recommended index) names right — the originals the carriers made ambiguous
            (optimistic: assumes the vote reaches exactly those);
  all       every unclaimed tied unit the vote decides (pessimistic: the vote also names what other guards refused).
Propagation off moved units is not re-run. Aggregates only.

usage: <venv python> -I carsim.py <base tag> <base results dir> <carrier ties dir> <carrier tag> <carrier results dir>
       <evidence> <rule>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402
from sim import STEMS, cls, pick  # noqa: E402


def prf(n, w, a, lib):
    p = n / (n + w + a) if n + w + a else 1.0
    r = n / lib if lib else 0.0
    return 100 * p, 100 * r, 100 * (1.25 * p * r / (0.25 * p + r) if p + r else 0.0)


def main():
    base_tag, base_res, d, tag, res, ev_kind, rule = sys.argv[1:8]
    out = {}
    for corpus, stem in STEMS.items():
        p = os.path.join(d, f"{corpus}-{tag}.jsonl")
        if not os.path.exists(p):
            continue
        lib096.use(corpus)
        import score
        run = os.path.join(lib096.DATA, "results", stem.format(base_tag), "reports")
        car = json.load(open(os.path.join(res, stem.format(tag), "summary.json")))["sets"]
        base = json.load(open(os.path.join(base_res, stem.format(base_tag), "summary.json")))["sets"]
        mv = collections.defaultdict(collections.Counter)
        claims, cur = {}, None
        for line in open(p):
            r = json.loads(line)
            v0 = cls(r["verdict"])
            f = pick(r["ev"], ev_kind, rule, r["claimFam"] if v0 != "unclaimed" else None)
            if f is None or f == r["claimFam"]:
                continue
            new = cls(r["ev"][f]["verdict"])
            m = mv[r["set"]]
            if v0 != "unclaimed":
                m[f"{v0}->{new}"] += r["bytes"]
                continue
            m[f"all:{new}"] += r["bytes"]
            if r["entry"] != cur:
                cur = r["entry"]
                rep = json.load(open(os.path.join(run, cur + ".json")))["libraryDiscovery"] or {}
                claims = {(c["start"], c["end"]): c["package"] for c in rep.get("claims", [])}
            was = claims.get((r["s"], r["e"]))
            if was is not None and cls(score.judge_origin(was, {"label": r["truth"], "origin": r.get("origin")},
                                                          None)[0]) == "named":
                m[f"refused:{new}"] += r["bytes"]
        for s, c in car.items():
            m = mv[s]
            b = base[s]
            lib = c["libraryBytes"]
            n0, w0, a0 = c["namedBytes"], c["wrongFamilyBytes"], c["appClaimedBytes"]
            n1 = n0 + m["wrong->named"] - m["named->wrong"] - m["named->other"]
            w1 = w0 - m["wrong->named"] + m["named->wrong"] - m["wrong->other"]
            a1 = a0 + m["named->other"] + m["wrong->other"]
            row = {"base": prf(b["namedBytes"], b["wrongFamilyBytes"], b["appClaimedBytes"], b["libraryBytes"]),
                   "car": prf(n0, w0, a0, lib)}
            for bound in ("refused", "all"):
                row[bound] = prf(n1 + m[f"{bound}:named"], w1 + m[f"{bound}:wrong"], a1 + m[f"{bound}:other"], lib)
                row[bound + "KB"] = [round(m[f"{bound}:named"] / 1024, 1), round(
                    (m[f"{bound}:wrong"] + m[f"{bound}:other"]) / 1024, 1)]
            out[s] = row
    print(f"## carrier index + vote ({ev_kind}:{rule}) vs {base_tag}: P / R / F0.5 (vote's names KB right / wrong)")
    print("| set | main | carrier | carrier+vote, refused bound | carrier+vote, all bound | P >= main (ref / all) |")
    print("| --- | ---: | ---: | ---: | ---: | --- |")
    f = lambda x: f"{x[0]:.3f} / {x[1]:.2f} / {x[2]:.3f}"  # noqa: E731
    for s in sorted(out):
        r = out[s]
        ok = ["yes" if r[k][0] >= r["base"][0] - 1e-9 else "NO" for k in ("refused", "all")]
        print(f"| {s} | {f(r['base'])} | {f(r['car'])} | {f(r['refused'])} ({r['refusedKB'][0]} / "
              f"{r['refusedKB'][1]}) | {f(r['all'])} ({r['allKB'][0]} / {r['allKB'][1]}) | {ok[0]} / {ok[1]} |")


if __name__ == "__main__":
    main()
