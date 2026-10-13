"""Experiment A step 3: which packages to index — greedy by bundle-byte coverage per MB vs download rank.

usage: python3 -I greedy.py        -> $SEL/greedy.json (order, curves, watch list), $SEL/split.json (held-out apps)

Inputs: $SEL/corpus.json, $SEL/apps/*/closure.json (closure.py), $SEL/npm-top.json, cached /latest manifests.

- Hold-out: 20% of the apps that bundled (seed 95, per stratum) never inform the selection; their coverage is
  reported separately (and step 4 measures real recognition on them).
- Weight w(app, p): minified bytes of p's files in the app's esbuild bundle (closure.py). Corpus library bytes = the
  sum over apps and packages. "Per-app" coverage = the mean over apps of each app's covered share.
- Cost c(p): the bytes the index has to hold for p ~ p's own minified code (humanify-index bundles p's main entry,
  which is what most apps pull in). Measured as the MEDIAN w(app, p) over the apps that bundle p (the max is
  inflated by apps that carry two copies of p at different versions, or union several subpath entries). A package no app bundles (download-rank candidates
  only) gets an ESTIMATE: its latest tarball's dist.unpackedSize x the median ratio code/unpackedSize of the
  packages that have both (printed). Index MB = k x selected code MB, k calibrated from the real indexes built in
  step 4 (heldout.py); until then the curve's x-axis is code MB.
- Greedy: packages sorted by (training corpus bytes covered) / c(p). Weights are additive per package, so the
  greedy order is the static ratio order (no interaction between packages).
- Download rank: npm-high-impact's order, skipping @types/*, platform binaries and type-only packages (the scale
  lane's skip rule), each package at its own c(p), until the same code MB.
"""
import json, os, re, random, statistics, glob, collections, sys

SEL = os.environ.get("SEL", "/work/exp095-library-fingerprints/selection")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from net import get

SKIP = re.compile(r"^@types/|^@esbuild/|^@rollup/rollup-|^@swc/core-|^@next/swc-|^@img/sharp-|-(darwin|linux|win32|freebsd|android)(-|$)|^@parcel/watcher-|^@oxc-|^@biomejs/cli-|^lightningcss-|^@tailwindcss/oxide-|^fsevents$|^type-fest$|^csstype$|^undici-types$|^devtools-protocol$")
WATCH = ["ink", "@aws-sdk/client-bedrock-runtime", "@azure/identity", "@growthbook/growthbook", "@anthropic-ai/bedrock-sdk",
         "yoga-layout", "yoga-wasm-web", "react", "react-dom", "react-reconciler", "zod", "@anthropic-ai/sdk", "@modelcontextprotocol/sdk"]
BUDGETS_MB = [1, 2, 5, 10, 20, 50, 100, 200, 400, 800]


def load():
    apps = []
    for f in sorted(glob.glob(SEL + "/apps/*/closure.json")):
        r = json.load(open(f))
        if r.get("packages") and r.get("libBytes", 0) > 0:
            apps.append(r)
    return apps


def split(apps):
    rnd = random.Random(95)
    held = set()
    for kind in ("electron", "cli", "agent"):
        ids = sorted(a["id"] for a in apps if a["kind"] == kind)
        rnd.shuffle(ids)
        held |= set(ids[: round(0.2 * len(ids))])
    return held


def unpacked(name):
    st, m = get("https://registry.npmjs.org/" + name.replace("/", "%2F") + "/latest")
    if st == 200 and isinstance(m, dict):
        return (m.get("dist") or {}).get("unpackedSize")
    return None


def curve(order, cost, apps, budgets):
    """coverage (total and per-app mean) at each code-MB budget for a package order."""
    out = []
    tot = sum(sum(p["bytes"] for p in a["packages"].values()) for a in apps)
    per_app_tot = [sum(p["bytes"] for p in a["packages"].values()) for a in apps]
    chosen, spent, i = set(), 0.0, 0
    for b in budgets:
        while i < len(order) and spent + cost[order[i]] <= b * 1e6:
            chosen.add(order[i])
            spent += cost[order[i]]
            i += 1
        cov = sum(p["bytes"] for a in apps for n, p in a["packages"].items() if n in chosen)
        per = [sum(p["bytes"] for n, p in a["packages"].items() if n in chosen) / t for a, t in zip(apps, per_app_tot) if t]
        out.append({"codeMB": b, "packages": len(chosen), "spentMB": round(spent / 1e6, 2), "coverage": round(cov / tot, 4),
                    "perAppMean": round(statistics.mean(per), 4) if per else 0})
    return out


def main():
    apps = load()
    held = split(apps)
    train = [a for a in apps if a["id"] not in held]
    test = [a for a in apps if a["id"] in held]
    top = json.load(open(SEL + "/npm-top.json"))
    rank = {n: i + 1 for i, n in enumerate(top)}
    seen_bytes, napps = collections.defaultdict(list), collections.Counter()
    for a in apps:
        for n, p in a["packages"].items():
            seen_bytes[n].append(p["bytes"])
            napps[n] += 1
    cost = {n: statistics.median(v) for n, v in seen_bytes.items()}
    # cost estimate for download-rank packages no app bundles
    dl = [n for n in top if not SKIP.search(n)]
    ratios = []
    for n in list(cost)[:4000]:
        u = unpacked(n)
        if u:
            ratios.append(cost[n] / u)
    ratio = statistics.median(ratios)
    est = 0
    for n in dl[:8000]:
        if n not in cost:
            u = unpacked(n)
            cost[n] = (u or 0) * ratio
            est += 1
    gain = collections.Counter()
    for a in train:
        for n, p in a["packages"].items():
            gain[n] += p["bytes"]
    greedy = sorted((n for n in gain if cost.get(n, 0) > 0), key=lambda n: (-gain[n] / cost[n], n))
    dlorder = [n for n in dl[:8000] if cost.get(n, 0) > 0]
    res = {
        "apps": len(apps), "train": len(train), "heldout": sorted(held),
        "byKind": dict(collections.Counter(a["kind"] for a in apps)),
        "corpusLibMB": round(sum(a["libBytes"] for a in apps) / 1e6, 2),
        "distinctPackages": len(napps), "costRatioMedian(code/unpacked)": round(ratio, 4), "estimatedCosts": est,
        "curves": {
            "greedy/train": curve(greedy, cost, train, BUDGETS_MB),
            "download/train": curve(dlorder, cost, train, BUDGETS_MB),
            "greedy/heldout": curve(greedy, cost, test, BUDGETS_MB),
            "download/heldout": curve(dlorder, cost, test, BUDGETS_MB),
        },
        "watch": {},
        "greedyTop": [{"name": n, "costKB": round(cost[n] / 1e3, 1), "apps": napps[n], "trainKB": round(gain[n] / 1e3, 1),
                       "dlRank": rank.get(n)} for n in greedy[:300]],
    }
    cum, pos = 0.0, {}
    for i, n in enumerate(greedy):
        cum += cost[n]
        pos[n] = (i + 1, cum / 1e6)
    cumd, posd = 0.0, {}
    for i, n in enumerate(dlorder):
        cumd += cost[n]
        posd[n] = (i + 1, cumd / 1e6)
    for w in WATCH:
        res["watch"][w] = {"apps": napps.get(w, 0), "costKB": round(cost.get(w, 0) / 1e3, 1), "dlRank": rank.get(w),
                           "greedyPos": pos.get(w, (None, None))[0], "greedyCodeMB": round(pos[w][1], 2) if w in pos else None,
                           "downloadPos": posd.get(w, (None, None))[0], "downloadCodeMB": round(posd[w][1], 2) if w in posd else None}
    json.dump(res, open(SEL + "/greedy.json", "w"), indent=1)
    json.dump({"heldout": sorted(held), "greedy": greedy, "download": dlorder, "cost": cost}, open(SEL + "/selection.json", "w"))
    for k, c in res["curves"].items():
        print(k, [(x["codeMB"], x["packages"], x["coverage"], x["perAppMean"]) for x in c])
    for w, v in res["watch"].items():
        print(w, v)


if __name__ == "__main__":
    main()
