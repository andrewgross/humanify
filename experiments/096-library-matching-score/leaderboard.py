"""exp096 leaderboard: labels side by side, totals first, deltas against the FIRST label.

usage: <venv python> -I leaderboard.py <label> <label> [label...] [--force-mixed]
Refuses labels scored on different corpus manifests, origin layers or by different scorer versions (--force-mixed prints them
anyway, loudly). The noise band is read from bands.json (measured: two runs of one configuration, byte-identical
— band 0); a delta of any size is real."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib096 import HERE  # noqa: E402

SETS = ["cc", "heldout", "wrangler", "esbuild-app", "test-bun", "test-esbuild", "test-webpack", "test-rollup",
        "dev-webpack", "dev-rollup"]
SET_NAMES = {"cc": "Claude Code 2.1.216 (sample est.)", "heldout": "held-out apps (67, Bun)",
             "wrangler": "wrangler (esbuild, re-minified)", "esbuild-app": "task-master-ai (esbuild)",
             "test-bun": "TEST split: new apps, Bun (the held-out recipe)",
             "test-esbuild": "TEST split: the same apps, esbuild (the index's esbuild recipe)",
             "test-webpack": "TEST split: the same apps, webpack + terser",
             "test-rollup": "TEST split: the same apps, rollup + terser",
             "dev-webpack": "exp105 dev apps, webpack + terser (the index's webpack recipe)",
             "dev-rollup": "exp105 dev apps, rollup + terser (the index's rollup recipe)"}


def load(label):
    d = os.path.join(HERE, "results", label)
    s = json.load(open(os.path.join(d, "summary.json")))
    t = json.load(open(os.path.join(d, "timing.json"))) if os.path.exists(os.path.join(d, "timing.json")) else {}
    return s, t


def pct(x):
    return f"{100 * x:5.1f}%"


def p_bar(bars):
    passed = bars["precisionBarPassed"]
    return f"{sum(passed.values())}/{len(passed)} sets pass"


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    force = "--force-mixed" in sys.argv
    if not args:
        raise SystemExit(__doc__)
    rows = [(lab, *load(lab)) for lab in args]
    for key in ("corpusSha256", "originsSha256"):
        vals = {s["config"][key] for _, s, _ in rows}
        if len(vals) > 1:
            msg = f"labels were scored on {len(vals)} different corpus manifests"
            if not force:
                raise SystemExit(f"REFUSED: {msg} (--force-mixed to show anyway)")
            print(f"!! MIXED CORPUS: {msg}")
    if len({s["schema"] for _, s, _ in rows}) > 1:
        msg = "labels were scored by different scorer versions"
        if not force:
            raise SystemExit(f"REFUSED: {msg} (--force-mixed to show anyway)")
        print(f"!! MIXED SCORERS: {msg}")
    bands = json.load(open(os.path.join(HERE, "bands.json"))) if os.path.exists(os.path.join(HERE, "bands.json")) \
        else None
    print("band:", "0 (measured: two runs byte-identical, bands.json) — every delta is real" if bands
          else "UNMEASURED (no bands.json) — run determinism.py first")
    print()
    w = max(len(r[0]) for r in rows)
    print(f"{'label':{w}s}  commit        index         index MB  rule        app bar  P bar           wall s  peak MB")
    for lab, s, t in rows:
        c = s["config"]
        print(f"{lab:{w}s}  {c['buildCommit'][:12]}  {c['indexId'][:12]}  {c['indexBytes'] / 1e6:8.0f}  "
              f"{c['rule']:10s}  {'PASS' if s['bars']['appBarPassed'] else 'FAIL':7s}  "
              f"{p_bar(s['bars']):14s}  "
              f"{t.get('totalWallSeconds', 0):6.0f}  {t.get('maxPeakRssMb', 0):7.0f}")
    base = rows[0][1]["sets"]
    print("\nprecision / recall / F1 over bytes, the ORIGIN-AWARE truth (a claim naming the shipping package OR, for a"
          "\nvendored / pre-bundled copy, its origin is right). precision = named / (named + wrong family + app"
          "\nclaimed); bar: precision >= 99.8% and app claimed = 0. [shipper] = the old shipper-only truth; as origin ="
          "\nnamed bytes right only through the origin; deltas against the first label")
    for set_ in SETS:
        if all(set_ not in s["sets"] for _, s, _ in rows):
            continue  # the other corpus's sets (dev labels hold no test set, test labels no dev set)
        print(f"\n{SET_NAMES[set_]}")
        print(f"  {'label':{w}s}  precision (delta)  recall  (delta)      F1  (delta)  [shipper P/R]  as origin  "
              "app claimed  wrong fam KB  not indexed  version  too small  hash differs  guard  other pkg  "
              "no discovery")
        for lab, s, _ in rows:
            r = s["sets"].get(set_)
            if r is None:
                print(f"  {lab:{w}s}  (absent)")
                continue
            b = base.get(set_, r)
            m = r["missShares"]
            dsum = sum(v for k, v in m.items() if k.startswith("d-"))
            esum = sum(v for k, v in m.items() if k.startswith("e-"))
            origin = r["namedAsOriginBytes"] / r["libraryBytes"] if r["libraryBytes"] else 0.0
            shipper = f"{pct(r['precisionShipperOnly'])}/{pct(r['recallShipperOnly'])}"
            print(f"  {lab:{w}s}  {pct(r['precision']):>9s} ({delta(r, b, 'precision')})  "
                  f"{pct(r['recall'])} ({delta(r, b, 'recall')})  {pct(r['f1'])} ({delta(r, b, 'f1')})  "
                  f"{shipper:>13s}  {pct(origin):>9s}  {r['appClaimedBytes']:11d}  "
                  f"{r['wrongFamilyBytes'] / 1e3:12.1f}  "
                  f"{pct(m.get('a', 0)):>11s}  {pct(m.get('b', 0)):>7s}  {pct(m.get('c', 0)):>9s}  "
                  f"{pct(dsum):>12s}  {pct(esum):>5s}  {pct(m.get('f', 0)):>9s}  {pct(m.get('no-discovery', 0)):>12s}")


def delta(r, b, key):
    return f"{100 * (r[key] - b[key]):+5.1f}"


if __name__ == "__main__":
    main()
