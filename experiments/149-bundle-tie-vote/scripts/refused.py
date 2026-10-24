"""exp149: on the CARRIER index, the tied units the run leaves unclaimed that main (recommended index) names right —
the originals the carriers made ambiguous (exp148 §4) — and what each bundle-level vote would name them. Joins
ties.py rows of the carrier label (with spans) to the base label's claims by unit span. Aggregates only.

usage: python3 -I refused.py <base tag> <ties dir of the carrier label> <carrier tag>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402
from sim import STEMS, cls, pick  # noqa: E402

RULES = (("claim", "unique"), ("excl", "unique"), ("excl", "unique+ctx"), ("claim", "plural"), ("excl", "plural"))


def main():
    base_tag, d, tag = sys.argv[1:4]
    agg = collections.defaultdict(collections.Counter)
    for corpus, stem in STEMS.items():
        p = os.path.join(d, f"{corpus}-{tag}.jsonl")
        if not os.path.exists(p):
            continue
        lib096.use(corpus)
        import score
        run = os.path.join(lib096.DATA, "results", stem.format(base_tag), "reports")
        claims, cur = {}, None
        for line in open(p):
            r = json.loads(line)
            if cls(r["verdict"]) != "unclaimed":
                continue
            if r["entry"] != cur:
                cur = r["entry"]
                rep = json.load(open(os.path.join(run, cur + ".json")))["libraryDiscovery"] or {}
                claims = {(c["start"], c["end"]): c["package"] for c in rep.get("claims", [])}
            was = claims.get((r["s"], r["e"]))
            if was is None:
                continue
            v = score.judge_origin(was, {"label": r["truth"], "origin": r.get("origin")}, None)[0]
            if cls(v) != "named":
                continue
            s, b = r["set"], r["bytes"]
            agg[s]["total"] += b
            for ev, rule in RULES:
                f = pick(r["ev"], ev, rule, None)
                out = "unresolved" if f is None else "right" if cls(r["ev"][f]["verdict"]) == "named" else "wrong"
                agg[s][f"{ev}:{rule}:{out}"] += b
    print(f"## {tag}: tied units unclaimed here that {base_tag} names right, KB (right / wrong / unresolved)")
    print("| set | total | " + " | ".join(f"{e}:{r}" for e, r in RULES) + " |")
    print("| --- | ---: | " + " | ".join("---:" for _ in RULES) + " |")
    T = collections.Counter()
    for s in sorted(agg) + ["**all**"]:
        c = agg[s] if s != "**all**" else T
        if s != "**all**":
            T.update(c)
        cells = [f"{c[f'{e}:{r}:right'] / 1024:.1f} / {c[f'{e}:{r}:wrong'] / 1024:.1f} / "
                 f"{c[f'{e}:{r}:unresolved'] / 1024:.1f}" for e, r in RULES]
        print(f"| {s} | {c['total'] / 1024:.1f} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
