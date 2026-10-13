"""exp095 "misses", Task 2: score exp095-variant-lookup claim files (index SEEDS: no fixpoint, no veto) against the
truth of the three test sets — recall, app bytes claimed, wrong-family bytes — and list what a variant claims that a
baseline does not, for reading by hand.

usage: <venv python> -I veval.py <claims dir> [<baseline claims dir>]
Claims dir: <slug>.claims (held-out apps), cc-2.1.216.claims, wrangler.claims (any subset)."""
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mmlib import MM, family  # noqa: E402

E = "/work/exp095-index-v2-data/eval"
NEG = "/work/exp095-library-fingerprints/negatives"


def verdict(p, label):
    if label == "<app>":
        return "app"
    if label in ("<self>", "<unmapped>"):
        return label[1:-1]
    if label.startswith("<"):  # Claude Code non-library labels: app, compiler-helper, bundler-runtime
        return "app" if label == "<app>" else label[1:-1]
    if p == label:
        return "right"
    return "family" if family(p) == family(label) else "wrongFamily"


def claims(path):
    if not os.path.exists(path):
        return None
    return {r["s"]: r["package"] for r in map(json.loads, open(path))}


def score_bundle(truth, cl):
    c = collections.Counter()
    lib = 0
    for t in truth:
        if t["label"] not in ("<app>", "<self>", "<unmapped>"):
            lib += t["b"]
        p = cl.get(t["s"])
        if p:
            c[verdict(p, t["label"])] += t["b"]
    return lib, c


def summary(lib, c):
    named = c["right"] + c["family"] + c["wrongFamily"]
    return {"recall": round((c["right"] + c["family"]) / max(lib, 1), 4), "app": c["app"], "self": c["self"],
            "wrongFamily": c["wrongFamily"], "wrongShareOfNamed": round(c["wrongFamily"] / max(named, 1), 4),
            "libBytes": lib}


def cc_score(cl):
    meta = json.load(open(f"{NEG}/data/sample-meta.json"))
    labels = [json.loads(l) for l in open(f"{NEG}/labels.jsonl")]
    labels = [r for r in labels if r["stratum"] in meta["strata"]]
    sb = collections.Counter()
    for r in labels:
        sb[r["stratum"]] += r["bytes"]
    starts = sorted(cl)
    import bisect
    spans = {}
    for s in starts:
        spans[s] = cl[s]
    ends = {}
    est, sample = collections.Counter(), collections.Counter()
    lib = 0.0
    for r in labels:
        w = meta["strata"][r["stratum"]]["bytes"] / sb[r["stratum"]]
        lab = r["label"]
        truth = lab.split(":", 1)[1] if lab.startswith("library:") else "<" + lab + ">"
        if lab.startswith("library:"):
            lib += r["bytes"] * w
        p = cl.get(r["start"])
        if p is None:
            continue
        v = verdict(p, truth)
        est[v] += r["bytes"] * w
        sample[v] += r["bytes"]
    named = est["right"] + est["family"] + est["wrongFamily"]
    return {"recall": round((est["right"] + est["family"]) / lib, 4), "appSampleBytes": sample["app"],
            "wrongSampleBytes": sample["wrongFamily"], "wrongShareOfNamed": round(est["wrongFamily"] / max(named, 1), 4)}


def main():
    d = sys.argv[1]
    base = sys.argv[2] if len(sys.argv) > 2 else None
    res = {}
    tot_lib, tot = 0, collections.Counter()
    added = collections.Counter()
    samples = []
    for slug, js, app in json.load(open(f"{E}/heldout-apps.json")):
        cl = claims(f"{d}/{slug}.claims")
        if cl is None:
            continue
        truth = [json.loads(l) for l in open(f"{MM}/truth/{slug}.jsonl")]
        lib, c = score_bundle(truth, cl)
        tot_lib += lib
        tot.update(c)
        if base:
            b = claims(f"{base}/{slug}.claims") or {}
            text = None
            for t in truth:
                p = cl.get(t["s"])
                if p and (b.get(t["s"]) is None or family(b[t["s"]]) != family(p)):
                    v = verdict(p, t["label"])
                    added[v] += t["b"]
                    if v in ("app", "wrongFamily", "self") or len(samples) < 400:
                        if text is None:
                            text = open(js, "rb").read()
                        samples.append({"app": slug, "v": v, "package": p, "truth": t["label"], "b": t["b"],
                                        "text": text[t["s"]:t["s"] + 160].decode("utf8", "replace")})
    if tot_lib:
        res["heldout"] = summary(tot_lib, tot)
    cl = claims(f"{d}/wrangler.claims")
    if cl is not None:
        truth = [json.loads(l) for l in open(f"{MM}/truth/wrangler.jsonl")]
        res["wrangler"] = summary(*score_bundle(truth, cl))
    cl = claims(f"{d}/cc-2.1.216.claims")
    if cl is not None:
        res["cc"] = cc_score(cl)
    if base:
        res["heldoutAddedVsBase"] = dict(added)
        json.dump(samples, open(f"{d}/added-samples.json", "w"), indent=1)
    json.dump(res, open(f"{d}/score.json", "w"), indent=1)
    print(json.dumps(res))


if __name__ == "__main__":
    main()
