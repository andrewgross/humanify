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

SETS = ["cc", "heldout", "wrangler", "esbuild-app"]
SET_NAMES = {"cc": "Claude Code 2.1.216 (sample est.)", "heldout": "held-out apps (67, Bun)",
             "wrangler": "wrangler (esbuild, re-minified)", "esbuild-app": "task-master-ai (esbuild)"}


def load(label):
    d = os.path.join(HERE, "results", label)
    s = json.load(open(os.path.join(d, "summary.json")))
    t = json.load(open(os.path.join(d, "timing.json"))) if os.path.exists(os.path.join(d, "timing.json")) else {}
    return s, t


def pct(x):
    return f"{100 * x:5.1f}%"


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
    print(f"{'label':{w}s}  commit        index         index MB  rule        app bar  wall s  peak MB")
    for lab, s, t in rows:
        c = s["config"]
        print(f"{lab:{w}s}  {c['buildCommit'][:12]}  {c['indexId'][:12]}  {c['indexBytes'] / 1e6:8.0f}  "
              f"{c['rule']:10s}  {'PASS' if s['bars']['appBarPassed'] else 'FAIL':7s}  "
              f"{t.get('totalWallSeconds', 0):6.0f}  {t.get('maxPeakRssMb', 0):7.0f}")
    base = rows[0][1]["sets"]
    print("\nrecall / wrong family: the ORIGIN-AWARE truth (a claim naming the shipping package OR, for a vendored /"
          "\npre-bundled copy, its origin is right); [shipper] = the old shipper-only truth; as origin = named bytes"
          "\nright only through the origin")
    for set_ in SETS:
        print(f"\n{SET_NAMES[set_]}")
        print(f"  {'label':{w}s}  recall   (delta)  [shipper]  as origin  app claimed  wrong family  [shipper]  "
              "wrong name  not indexed  version  too small  hash differs  guard  other pkg  no discovery")
        for lab, s, _ in rows:
            r = s["sets"].get(set_)
            if r is None:
                print(f"  {lab:{w}s}  (absent)")
                continue
            m = r["missShares"]
            d = r["recall"] - base[set_]["recall"] if set_ in base else 0.0
            dsum = sum(v for k, v in m.items() if k.startswith("d-"))
            esum = sum(v for k, v in m.items() if k.startswith("e-"))
            origin = r["namedAsOriginBytes"] / r["libraryBytes"] if r["libraryBytes"] else 0.0
            print(f"  {lab:{w}s}  {pct(r['recall'])}  ({100 * d:+5.1f})  {pct(r['recallShipperOnly']):>9s}  "
                  f"{pct(origin):>9s}  {r['appClaimedBytes']:11d}  "
                  f"{pct(r['wrongFamilyShare']):>12s}  {pct(r['wrongFamilyShareShipperOnly']):>9s}  "
                  f"{pct(r['wrongNameShare']):>10s}  "
                  f"{pct(m.get('a', 0)):>11s}  {pct(m.get('b', 0)):>7s}  {pct(m.get('c', 0)):>9s}  "
                  f"{pct(dsum):>12s}  {pct(esum):>5s}  {pct(m.get('f', 0)):>9s}  {pct(m.get('no-discovery', 0)):>12s}")

if __name__ == "__main__":
    main()
