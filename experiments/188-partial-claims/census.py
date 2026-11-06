"""exp188 census: how many library bytes sit in UNCLAIMED units whose exactly-matched function cover is under the
structural rule's 50%? No behaviour change: it reads one label's existing discovery reports (`--unit-prints`) and
the index table the scorer already caches, and emulates the structural route WITHOUT the cover floor
(mmlib.structural's branch: whole print first, else the largest non-overlapping function hits; spec + max_packages
as shipped).

usage: <venv python> -I census.py <label> --corpus bun-dev|bun-held|cc [--json out.json]

Per unclaimed scored unit (statement + factory-body scope, the scorer's own population) with prints:
  cover   = best package's covered nodes / the unit's whole nodes   (0 when no print hit)
  verdict = the scorer's judge_origin() of that best package against the truth label:
            right (right/family/origin), wrong (wrongFamily), app, other (self*, unmapped, foreign)
Bytes are counted two ways: the WHOLE unit (a lower-threshold statement claim) and the matched FUNCTION spans only
(a function-sized claim). Recall points = right bytes / the corpus's library bytes (the label's summary).
A PRIVATE corpus (bun-held) prints totals only: no bundle is named.

What it cannot see: every later guard of the real run (the support window, the isolation veto, growth/glue, the
call-context relaxers and tie-breaks). It is a ceiling on the seed step, validated against the run's own seeds
(the `validation` block: emulated cover >= 0.5 vs the run's index seeds)."""
import bisect
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
S096 = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, S096)
import lib096  # noqa: E402
import score  # noqa: E402  (run_units, label_units, judge_origin, load_index: the scorer's own owners)
import mmlib  # noqa: E402
import truth_audit  # noqa: E402

BUCKETS = [(0.0, 0.0, "0 (no hit)"), (0.0, 0.1, "(0,10)"), (0.1, 0.2, "[10,20)"), (0.2, 0.3, "[20,30)"),
           (0.3, 0.4, "[30,40)"), (0.4, 0.5, "[40,50)"), (0.5, 9.9, ">=50 (guard refused)")]


def bucket(frac):
    if frac <= 0:
        return BUCKETS[0][2]
    for lo, hi, name in BUCKETS[1:]:
        if lo <= frac < hi:
            return name
    return BUCKETS[-1][2]


def best_route(ix, u, rules=mmlib.RULES):
    """(package, covered nodes, matched byte spans) for the structural route with NO cover floor, or None."""
    whole, ok_w, ok_f = mmlib.eligible(u, rules)
    spec = rules["spec"]
    if ok_w:
        o = ix.owners(whole[2], u["mb"], spec)
        if o and len(o) <= rules["max_packages"]:
            return sorted(o)[0], whole[3], [(whole[0], whole[1])]
    hits = []
    for f in ok_f:
        o = ix.owners(f[2], u["mb"], spec)
        if o and len(o) <= rules["max_packages"]:
            hits.append((f, o))
    hits.sort(key=lambda x: -x[0][3])
    taken, score_, spans = [], collections.Counter(), collections.defaultdict(list)
    for f, o in hits:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        for p in o:
            score_[p] += f[3]
            spans[p].append((f[0], f[1]))
    if not score_:
        return None
    best = max(score_.values())
    p = sorted(k for k, v in score_.items() if v == best)[0]
    return p, best, spans[p]


def verdict_of(v):
    if v in ("right", "family", "origin"):
        return "right"
    if v == "wrongFamily":
        return "wrong"
    if v == "app":
        return "app"
    return "other"


class Tally:
    def __init__(self):
        self.unit = collections.defaultdict(collections.Counter)  # bucket -> verdict -> unit bytes
        self.fn = collections.defaultdict(collections.Counter)  # bucket -> verdict -> function-span bytes
        self.lib_unclaimed = collections.Counter()  # bucket -> library unit bytes (truth), any verdict
        self.val = collections.Counter()

    def add(self, b, verdict, unit_bytes, fn_bytes, is_lib):
        self.unit[b][verdict] += unit_bytes
        self.fn[b][verdict] += fn_bytes
        if is_lib:
            self.lib_unclaimed[b] += unit_bytes

    def merge(self, o):
        for k in o.unit:
            self.unit[k].update(o.unit[k])
            self.fn[k].update(o.fn[k])
        self.lib_unclaimed.update(o.lib_unclaimed)
        self.val.update(o.val)


def fn_bytes_of(spans, s, e):
    return sum(min(b, e) - max(a, s) for a, b in spans if min(b, e) > max(a, s))


def census_map(report, truth, ix, origins, audit, entry, t):
    units = score.run_units(report)
    every = score.scored_units(units)
    labels = score.label_units(truth, units, origins, audit)
    app_pkg = truth["appPackage"]
    key = {(u["s"], u["e"]): u["i"] for u in every}
    claims = {key[(c["start"], c["end"])]: c for c in report.get("claims", []) if (c["start"], c["end"]) in key}
    for u in every:
        if "w" not in u:
            continue
        r = best_route(ix, u)
        c = claims.get(u["i"])
        frac = r[1] / max(u["w"][1], 1) if r else 0.0
        if c is not None:
            if c["rule"] == "seed" and (c.get("evidence") or {}).get("kind") in ("index", "queueIndex"):
                t.val["seedBytes"] += u["b"]
                if r and frac >= 0.5 and score.same_family(r[0], c["package"]):
                    t.val["seedBytesEmulatedSame"] += u["b"]
            continue
        row = labels[u["i"]]
        if r is None:
            v = "nohit"
            fb = 0
        else:
            v = verdict_of(score.judge_origin(r[0], row, app_pkg)[0])
            fb = fn_bytes_of(r[2], u["s"], u["e"])
        t.add(bucket(frac), v, u["b"], fb, row["label"] not in score.NOT_LIBRARY)


def census_sample(report, truth, ix, t):
    units = score.run_units(report)
    st = score.statement_units(units)
    by_s = {u["s"]: u for u in st}
    strata = truth["strata"]
    sb = collections.Counter()
    for row in truth["rows"]:
        sb[row["stratum"]] += row["b"]

    def est(row, nbytes):
        return strata[row["stratum"]]["bytes"] * nbytes / sb[row["stratum"]]
    cl = sorted((c["start"], c["end"], c["package"]) for c in report.get("claims", []))
    starts = [c[0] for c in cl]

    def claim_at(s):
        k = bisect.bisect_right(starts, s) - 1
        return cl[k][2] if k >= 0 and cl[k][0] <= s < cl[k][1] else None
    for row in truth["rows"]:
        if claim_at(row["s"]) is not None:
            continue
        u = by_s.get(row["s"])
        if u is None or "w" not in u:
            continue
        r = best_route(ix, u)
        frac = r[1] / max(u["w"][1], 1) if r else 0.0
        lib = "pkg" in row
        if r is None:
            v, fb = "nohit", 0
        elif lib:
            v = "right" if score.same_family(r[0], row["pkg"]) else "wrong"
            fb = fn_bytes_of(r[2], u["s"], u["e"])
        else:
            v = "app" if row["label"] == "<app>" else "other"
            fb = fn_bytes_of(r[2], u["s"], u["e"])
        # the sample row's bytes, function bytes scaled to it (row b == unit bytes for a matched statement)
        t.add(bucket(frac), v, est(row, row["b"]), est(row, min(fb, row["b"])), lib)


def main():
    argv = lib096.corpus_flag(sys.argv)
    label = argv[1]
    out_json = argv[argv.index("--json") + 1] if "--json" in argv else None
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    private = lib096.CORPUS in lib096.PRIVATE
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    indexes = {}
    sets = collections.defaultdict(Tally)
    for entry in lib096.load_manifest()["entries"]:
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        report = doc["libraryDiscovery"]
        if report is None:
            continue
        slice_ = doc["toolchain"]["slice"]
        if slice_ not in indexes:
            indexes[slice_] = score.load_index(pipeline["indexDir"], slice_, pipeline["indexId"])
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m) if entry["truth"]["method"] == "sourcemap" else None
        truth, origins = truth_audit.redecode(entry, truth, origins)
        t = Tally()
        if truth["method"] == "sample":
            census_sample(report, truth, indexes[slice_], t)
        else:
            census_map(report, truth, indexes[slice_], origins, truth_audit.Audit(entry, truth, cm), entry, t)
        sets[entry["set"]].merge(t)
        if not private:
            print(f"  {entry['id']}", file=sys.stderr, flush=True)
    summary = json.load(open(os.path.join(S096, "results", label, "summary.json")))
    out = {}
    for set_, t in sorted(sets.items()):
        lib = summary["sets"][set_]["libraryBytes"]
        rows = {}
        for _, _, b in BUCKETS:
            uc, fc = t.unit[b], t.fn[b]
            rows[b] = {"libraryUnclaimedBytes": round(t.lib_unclaimed[b]),
                       "unit": {k: round(uc[k]) for k in ("right", "wrong", "app", "other", "nohit")},
                       "function": {k: round(fc[k]) for k in ("right", "wrong", "app", "other")},
                       "recallPointsUnit": round(100 * uc["right"] / lib, 3) if lib else 0,
                       "recallPointsFunction": round(100 * fc["right"] / lib, 3) if lib else 0}
        out[set_] = {"libraryBytes": lib, "recall": summary["sets"][set_]["recall"],
                     "precision": summary["sets"][set_]["precision"], "buckets": rows,
                     "validation": {k: round(v) for k, v in t.val.items()}}
    text = json.dumps(out, indent=1, sort_keys=True)
    print(text)
    if out_json:
        open(out_json, "w").write(text + "\n")


if __name__ == "__main__":
    main()
