#!/usr/bin/env python3
"""exp167: which discovery guard refuses the `guard.presence` / `guard.tie-fenced` bytes — measured, not inferred.

usage: <exp095 venv python> -I toggles.py <toggle reports dir> <census examples.json> <out.json>

The toggle reports are `humanify discover --unit-prints` runs of six bun-dev bundles, once per config: `base`
(exp165's names-mode config) and one with a single [library.discovery] guard switched off (cfg/<name>.toml). For
every toggle, the units whose claim CHANGED against base are judged by the scorer-14 truth: bytes newly named right,
newly wrong / app / self-other, and bytes that lost a right claim. The census's own causes for the units (from
examples.json, which holds every guard.presence unit and the first 400 tie-fenced) say which cause each toggle
recovers.
"""
import collections
import json
import os
import sys

TOGGLE_DIR, EXAMPLES, OUT = sys.argv[1:4]
sys.argv = [sys.argv[0], "bun-dev"]
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import census  # noqa: E402
from census import lib096, score, truth_audit  # noqa: E402


def main():
    manifest = {e["id"]: e for e in lib096.load_manifest()["entries"]}
    chm = truth_audit.load_chained_manifest()
    om = lib096.load_origins_manifest()
    o7m = truth_audit.load_origins7_manifest()
    cm = truth_audit.load_copies_manifest()
    ex = json.load(open(EXAMPLES))["examples"]
    cause_at = {}
    for cause, rows in ex.items():
        for e in rows:
            cause_at[(e["bundle"], e["s"])] = cause
    files = sorted(os.listdir(TOGGLE_DIR))
    bundles = sorted({f.rsplit(".", 2)[0] for f in files if f.endswith(".base.json")})
    out = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    for b in bundles:
        entry = manifest[b]
        base = json.load(open(os.path.join(TOGGLE_DIR, b + ".base.json")))["libraryDiscovery"]
        units = score.run_units(base)
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        labels = score.label_units(truth, units, origins, truth_audit.Audit(entry, truth, cm))
        every = {(u["s"], u["e"]): u for u in score.scored_units(units)}
        app_pkg = entry["appPackage"]

        def claims(ld):
            return {(c["start"], c["end"]): c["package"] for c in ld["claims"] if (c["start"], c["end"]) in every}
        c0 = claims(base)
        for f in files:
            if not f.startswith(b + ".") or f.endswith(".base.json"):
                continue
            k = f[len(b) + 1:-5]
            ld = json.load(open(os.path.join(TOGGLE_DIR, f)))["libraryDiscovery"]
            c1 = claims(ld)
            for span in set(c0) | set(c1):
                p0, p1 = c0.get(span), c1.get(span)
                if p0 == p1:
                    continue
                u = every[span]
                t = labels[u["i"]]
                v0 = score.judge_origin(p0, t, app_pkg)[0] if p0 else "unclaimed"
                v1 = score.judge_origin(p1, t, app_pkg)[0] if p1 else "unclaimed"
                cause = cause_at.get((b, u["s"]), "-")
                out[k][f"{v0}->{v1}"][cause] += u["b"]
    res = {k: {tr: dict(c) for tr, c in v.items()} for k, v in out.items()}
    json.dump(res, open(OUT, "w"), indent=1, sort_keys=True)
    for k, v in sorted(res.items()):
        print("==", k)
        for tr, c in sorted(v.items(), key=lambda kv: -sum(kv[1].values())):
            print(f"   {tr:28s} {sum(c.values()):>10,} B  {dict(sorted(c.items(), key=lambda kv: -kv[1])[:4])}")


if __name__ == "__main__":
    main()
