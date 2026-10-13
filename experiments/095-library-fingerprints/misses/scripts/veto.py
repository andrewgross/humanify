"""exp095 "misses", Task 2: what the isolation veto throws away, judged against the truth — would accepting vetoed
index seeds (all, or only big ones) add recall without app code or wrong families?

usage: <venv python> -I veto.py   (reads the indexv2 reports, decompose truth and the unit dumps)"""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mmlib import MM, family, load_dump  # noqa: E402

E = "/work/exp095-index-v2-data/eval"


def judge(p, truth):
    if truth in ("<app>",):
        return "app"
    if truth in ("<self>", "<unmapped>"):
        return truth
    if p == truth:
        return "right"
    return "family" if family(p) == family(truth) else "wrongFamily"


def run(name, report, dump, truth):
    units = {u["s"]: u for u in load_dump(dump)}
    t = {r["s"]: r for r in map(json.loads, open(truth))}
    r = json.load(open(report))
    out = collections.defaultdict(collections.Counter)
    for v in r.get("vetoed", []):
        u, tr = units.get(v["start"]), t.get(v["start"])
        if not u or not tr:
            continue
        k = judge(v["package"], tr["label"])
        size = "big(>=100 nodes)" if u.get("w", [0, 0])[1] >= 100 else "small"
        out[size][k] += u["b"]
        out["all"][k] += u["b"]
    return out


def main():
    tot = {rule: collections.defaultdict(collections.Counter) for rule in ("struct", "agree")}
    for slug, js, app in json.load(open(f"{E}/heldout-apps.json")):
        for rule, d in (("struct", "v2s"), ("agree", "v2a")):
            p = f"{E}/{d}/heldout/{slug}.{rule}.json"
            if os.path.exists(p) and "claims" in json.load(open(p)):
                for k, c in run(slug, p, f"{MM}/dump/{slug}.jsonl", f"{MM}/truth/{slug}.jsonl").items():
                    tot[rule][k].update(c)
    for rule in tot:
        print("heldout", rule, {k: dict(v) for k, v in tot[rule].items()})
    for rule in ("struct", "agree"):
        o = run("wrangler", f"{E}/v2/wrangler.{rule}.json", f"{MM}/dump/wrangler.jsonl", f"{MM}/truth/wrangler.jsonl")
        print("wrangler", rule, {k: dict(v) for k, v in o.items()})
        # Claude Code: vetoed seeds on labelled sample units
        r = json.load(open(f"{E}/v2/cc-2.1.216-{rule}.json"))
        lab = {x["s"]: x for x in map(json.loads, open(f"{MM}/truth/cc-2.1.216.jsonl"))}
        c = collections.Counter()
        for v in r.get("vetoed", []):
            for s, x in lab.items():
                if v["start"] <= s < v["end"]:
                    c[judge(v["package"], x.get("pkg") or x["label"])] += x["b"]
        print("cc sample", rule, dict(c), "vetoed rows", len(r.get("vetoed", [])))


if __name__ == "__main__":
    main()
