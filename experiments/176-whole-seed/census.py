#!/usr/bin/env python3
"""exp176: the shape of what the factory whole-seed rule refuses (bun-dev, first-order, read-only).

usage: <exp095 venv python> -I census.py bun-dev <reports dir> <out.json> [--limit N]

Reads exp167's main-binary reports (names mode, index cb64c1f5) and labels every unit with exp096's scorer-14
truth through exp167's census (`entry_rows`, imported from the exp167 branch checkout at /work/exp167-miss). For
every library unit exp167 classes `guard.presence` (the right family holds prints covering >= half the unit, the
structural rule would pick it, no claim: the factory whole-seed rule), it records:

  stmts     body statements of the unit's CommonJS module (FactoryBody units sharing `m`)
  share     the unit's bytes / the module's body bytes
  cover     the right family's cover of the unit (whole print, else the largest non-overlapping function prints
            at the lookup's 30-node floor, any owner count) / the unit's whole-print nodes
  mcover    the same summed over every body statement of the module / the module's nodes (module-level cover)

and, for the module-level view, every module whose units are ALL unclaimed and whose largest statement the
structural rule gives to some family: that family's module cover and the module's truth verdict for it (right /
wrong / app), so the wrong side of a cover threshold is visible, not only the right side. First-order only: no
propagation (rule 11 shape; the measured runs decide).
"""
import collections
import json
import os
import sys

E167 = "/work/exp167-miss/experiments/167-library-miss-census"
sys.path.insert(0, E167)
import census as c167  # noqa: E402  (parses sys.argv[1] as the corpus)

lib096, score, mmlib = c167.lib096, c167.score, c167.mmlib
BINS = (0.5, 0.8, 0.9, 0.95, 0.98, 0.99, 1.0)


def fbin(x):
    for b in BINS:
        if x < b:
            return f"<{b}"
    return ">=1.0"


def module_cover(ix, units, hit):
    cov = sum(mmlib.cover_by(u, hit) for u in units if "w" in u)
    nodes = sum(max(u["w"][1], 1) for u in units if "w" in u)
    return cov, nodes


def main():
    reports, out = sys.argv[2], sys.argv[3]
    limit = int(sys.argv[sys.argv.index("--limit") + 1]) if "--limit" in sys.argv else None
    manifest = lib096.load_manifest()
    chm = c167.truth_audit.load_chained_manifest()
    layers = (chm, lib096.load_origins_manifest(), c167.truth_audit.load_origins7_manifest(),
              c167.truth_audit.load_copies_manifest())
    indexes = {}
    presence = []  # per guard.presence unit
    modules = []  # per unclaimed module with a structural pick
    for n, entry in enumerate(manifest["entries"]):
        if limit is not None and n >= limit:
            break
        rp = os.path.join(reports, entry["id"] + ".json")
        doc = json.load(open(rp))
        sl = doc["toolchain"]["slice"]
        if sl not in indexes:
            indexes[sl] = score.load_index(c167.INDEX_DIR, sl, c167.INDEX_ID)
        ix = indexes[sl]
        code = open(lib096.corpus_path(entry, "bundle"), "rb").read()
        rows = c167.entry_rows(entry, doc, ix, code, layers)
        app_pkg = entry.get("appPackage")
        by_mod = collections.defaultdict(list)
        for r in rows:
            u = r[0]
            if u["k"] == "FactoryBody":
                by_mod[u["m"]].append(r)
        for m, rs in by_mod.items():
            units = [r[0] for r in rs]
            mbytes = sum(u["b"] for u in units)
            stmts = len(units)
            for u, t, c, verdict, cause, det, mates, lab, pv in rs:
                if cause != "guard.presence":
                    continue

                def fam_hit(p, t=t):
                    return any(c167.right_family(o[0], t) for o in ix.units.get(p[2], ()))
                wn = max(u["w"][1], 1)
                mc, mn = module_cover(ix, units, fam_hit)
                presence.append({"bundle": entry["id"], "m": m, "b": u["b"], "pkg": t["pkg"], "stmts": stmts,
                                 "share": u["b"] / max(mbytes, 1), "cover": mmlib.cover_by(u, fam_hit) / wn,
                                 "mcover": mc / max(mn, 1), "s": u["s"], "e": u["e"]})
            if any(r[2] is not None for r in rs):
                continue
            big = max(units, key=lambda u: u["b"])
            if "w" not in big:
                continue
            h = mmlib.structural(ix, big)
            if not h or not isinstance(h, tuple) or h[0] is None:
                continue
            pick = h[0]

            def pick_hit(p, pick=pick):
                return any(c167.same(o[0], pick) for o in ix.units.get(p[2], ()))
            mc, mn = module_cover(ix, units, pick_hit)
            bc = mmlib.cover_by(big, pick_hit) / max(big["w"][1], 1)
            verdicts = collections.Counter()
            for u, t, *_ in rs:
                v = score.judge_origin(pick, t, app_pkg)[0] if "pkg" in t else t["label"]
                verdicts[v] += u["b"]
            modules.append({"bundle": entry["id"], "m": m, "pick": pick, "stmts": stmts, "bytes": mbytes,
                            "bigShare": big["b"] / max(mbytes, 1), "bigCover": bc, "mcover": mc / max(mn, 1),
                            "verdicts": dict(verdicts)})
        print(f"{entry['id']:70s} done", flush=True)
    agg = collections.defaultdict(collections.Counter)
    for p in presence:
        agg["presence:stmts=" + ("1" if p["stmts"] == 1 else "2" if p["stmts"] == 2 else "3+")][fbin(p["cover"])] += p["b"]
        agg["presence:mcover"][fbin(p["mcover"])] += p["b"]
    for mo in modules:
        key = "modules:stmts=" + ("1" if mo["stmts"] == 1 else "2+")
        for v, b in mo["verdicts"].items():
            agg[f"{key}:{v}"][fbin(mo["mcover"])] += b
    json.dump({"presence": presence, "modules": modules, "aggregate": {k: dict(v) for k, v in agg.items()}},
              open(out, "w"), indent=1)
    for k in sorted(agg):
        print(k, dict(sorted(agg[k].items())))


if __name__ == "__main__":
    main()
