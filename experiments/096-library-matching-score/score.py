"""exp096: score one label's discovery reports — one CARD per (label, corpus entry), one summary per label.

usage: <venv python> -I score.py <label>
reads  <DATA>/results/<label>/reports/<id>.json   (`humanify discover --unit-prints` output)
       <DATA>/results/<label>/pipeline.json        (the runner's config: index dir, rule, binary)
writes experiments/096-library-matching-score/results/<label>/cards/<id>.json and summary.json (DETERMINISTIC:
       no clock, no host — the determinism check compares these bytes), timing.json (wall time, peak memory:
       NOT deterministic, kept out of the cards).

What a card says, totals first (statement bytes = the bytes each statement unit OWNS; units = the run's own unit
model, `unitRows`; a unit is library when most of its mapped bytes come from `node_modules/<pkg>`):
  recall              library bytes claimed as the right package or its family / library bytes
  appClaimedBytes     bytes of APP code (not from node_modules) claimed as any package — the hard bar: 0
  wrongFamilyShare    claimed library bytes named as ANOTHER family / all claimed library bytes
  wrongNameShare      (wrong family + app) / all claimed bytes — read against the ~0.2% bar exp095's scale lane
                      measured for structural AND literal agreement
  misses              every library unit NOT named, one cause each (the exp095 misses lane's classifier,
                      decompose.classify, unchanged): a not indexed / b version missing / c too small /
                      d hash differs (d-file, d-variant, d-absent, d-nofile) / e guard refused (e-veto, e-agree,
                      e-support, e-spec, e-ambig, e-other) / f in the index as another package
  exploratory.names   EXPLORATORY: units claimed through an exact whole-unit hash, the local bindings they
                      declare, and the share of those whose index rows hold the truth's exact version (the index
                      builds that version's npm source, so its names would be the author's, up to a bundler's
                      collision renames). No name is read from the index: it stores none today.
Claude Code is scored on the labelled sample: byte shares are the stratified ratio estimate (strata sizes from the
negatives lane), counts are sample units.
"""
import bisect
import collections
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib096  # noqa: E402
from lib096 import DATA, HERE, dump  # noqa: E402

import mmlib  # noqa: E402  (the misses lane: index table, structural-rule emulation)

MISS_CLASSES = ["no-discovery", "a", "b", "c", "d-file", "d-variant", "d-absent", "d-nofile", "e-veto", "e-agree", "e-support",
                "e-spec", "e-ambig", "e-other", "f", "unit-model"]
WRONG_NAME_BAR = 0.002
# truth labels that are not a library package: app code, the app's own package, no mapping, a chained URL source
NOT_LIBRARY = ("<app>", "<self>", "<unmapped>", "<foreign>")
SCORER_VERSION = "exp096-score/1"


def r6(x):
    return round(x, 6)


def share(n, d):
    return r6(n / d) if d else 0.0


# ---------------------------------------------------------------- the run's units, labelled by the truth

def run_units(report):
    """unitRows -> the misses lane's unit dicts (i, s, e, b, k, rt, mb, w, f) + bindings."""
    out = []
    for i, r in enumerate(report["unitRows"]):
        u = {"i": i, "s": r["start"], "e": r["end"], "b": r["bytes"], "k": r["kind"], "rt": r["runtime"],
             "mb": r["moduleBytes"], "bindings": r["bindings"]}
        if "whole" in r:
            w = r["whole"]
            u["w"] = [w["hash"], w["nodes"], w["content"]]
            u["f"] = [[f["start"], f["end"], f["hash"], f["nodes"], f["content"]] for f in r.get("functions", [])]
        out.append(u)
    return out


def statement_units(units):
    return [u for u in units if u["k"] in ("Statement", "LazyBody") and not u["rt"]]


def label_units(truth, units):
    """truth.py label_units over the frozen segment table: majority mapped bytes; for a library label the
    version/file of the source holding most of its bytes."""
    segs = truth["segments"]
    srcs = truth["sources"]
    offs = [s[0] for s in segs]
    out = {}
    for u in statement_units(units):
        s, e = u["s"], u["e"]
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        votes, files = collections.Counter(), collections.Counter()
        while k < len(segs) and segs[k][0] < e:
            a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                votes[srcs[segs[k][1]][0]] += b - a
                files[segs[k][1]] += b - a
            k += 1
        lab = votes.most_common(1)[0][0] if votes else "<unmapped>"
        row = {"label": lab}
        if lab not in NOT_LIBRARY:
            src = next(si for si, _ in files.most_common() if srcs[si][0] == lab)
            row.update(pkg=lab, version=srcs[src][1], file=srcs[src][2])
        out[u["i"]] = row
    return out


def judge(claimed, truth_label, app_pkg):
    """score_claims.judge, with the app's own package split out: <self> claimed as another package is a copy
    vendored INSIDE the app's package (vite's parse5), reported apart from app code."""
    if truth_label == "<self>":
        return "self" if claimed == app_pkg else "selfOther"
    if truth_label == "<app>":
        return "app"
    if truth_label in ("<unmapped>", "<foreign>"):
        return truth_label.strip("<>")
    if claimed == truth_label:
        return "right"
    return "family" if mmlib.family(claimed) == mmlib.family(truth_label) else "wrongFamily"


# ---------------------------------------------------------------- the exploratory names column

def exact_rows(ix, u, package):
    """The index rows that hold u's WHOLE hash under package's family (an exact unit match), or []."""
    if "w" not in u:
        return []
    fam = mmlib.family(package)
    return [r for r in ix.units.get(u["w"][0], ()) if mmlib.family(r[0]) == fam]


def same_version(ix, rows, pkg, version):
    return version is not None and any(r[0] == pkg and version in ix.expand(r[0], r[4]) for r in rows)


class Names:
    def __init__(self):
        self.c = collections.Counter()

    def add(self, ix, u, package, truth=None):
        rows = exact_rows(ix, u, package)
        if not rows:
            return
        self.c["exactUnits"] += 1
        self.c["exactBytes"] += u["b"]
        self.c["bindings"] += u["bindings"]
        if truth is not None and truth.get("version") is not None:
            self.c["bindingsVersionKnown"] += u["bindings"]
            if same_version(ix, rows, truth["pkg"], truth["version"]):
                self.c["bindingsSameVersion"] += u["bindings"]

    def block(self):
        c = self.c
        return {"exactUnits": c["exactUnits"], "exactBytes": c["exactBytes"], "bindings": c["bindings"],
                "bindingsVersionKnown": c["bindingsVersionKnown"],
                "bindingsSameVersion": c["bindingsSameVersion"],
                "sameVersionShare": share(c["bindingsSameVersion"], c["bindingsVersionKnown"])}


# ---------------------------------------------------------------- one card

def spans(rows):
    return {(r["start"], r["end"]): r["package"] for r in rows}


def rule_name(rule):
    return {"agreement": "agree", "structural": "struct"}[rule]


def score_map(entry, report, truth, ix, rule):
    units = run_units(report)
    by_i = {u["i"]: u for u in units}
    st = statement_units(units)
    labels = label_units(truth, units)
    app_pkg = truth["appPackage"]
    key = {(u["s"], u["e"]): u["i"] for u in st}
    claims_all = report.get("claims", [])
    claims = {key[(c["start"], c["end"])]: c for c in claims_all if (c["start"], c["end"]) in key}
    vetoed = {key[k]: p for k, p in spans(report.get("vetoed", [])).items() if k in key}
    total = collections.Counter()
    for u in st:
        lab = labels[u["i"]]["label"]
        total["library" if lab not in NOT_LIBRARY else lab] += u["b"]
    claimed, seeds = collections.Counter(), collections.Counter()
    names = Names()
    for i, c in claims.items():
        v = judge(c["package"], labels[i]["label"], app_pkg)
        claimed[v] += by_i[i]["b"]
        if c["rule"] == "seed" and (c.get("evidence") or {}).get("kind") in ("index", "queueIndex"):
            seeds[v] += by_i[i]["b"]
        if v in ("right", "family"):
            names.add(ix, by_i[i], c["package"], labels[i])
    # misses: every library unit not named right / same family, one cause (decompose.classify, unchanged)
    struct_claims = mmlib.claim_all(ix, st) if rule == "agreement" else {}
    misses = collections.Counter()
    for u in st:
        t = labels[u["i"]]
        if "pkg" not in t:
            continue
        c = claims.get(u["i"])
        if c and judge(c["package"], t["label"], app_pkg) in ("right", "family"):
            continue
        if "w" not in u:
            misses["unit-model"] += u["b"]
            continue
        import decompose
        misses[decompose.classify(ix, u, t, c["package"] if c else None, vetoed, struct_claims,
                                  rule_name(rule))] += u["b"]
    lib = total["library"]
    named = claimed["right"] + claimed["family"]
    named_any = named + claimed["wrongFamily"]
    all_claimed = sum(claimed.values()) - claimed["unmapped"] - claimed["foreign"]
    seed_named = seeds["right"] + seeds["family"]
    return {
        "totals": {
            "libraryBytes": lib,
            "namedBytes": named,
            "recall": share(named, lib),
            "appClaimedBytes": claimed["app"],
            "wrongFamilyBytes": claimed["wrongFamily"],
            "wrongFamilyShare": share(claimed["wrongFamily"], named_any),
            "wrongNameShare": share(claimed["wrongFamily"] + claimed["app"], all_claimed),
        },
        "statementBytes": {k: total[k] for k in sorted(total)},
        "claimedBytes": {k: claimed[k] for k in sorted(claimed)},
        "indexSeeds": {"namedBytes": seed_named, "recall": share(seed_named, lib),
                       "wrongFamilyShare": share(seeds["wrongFamily"], seed_named + seeds["wrongFamily"]),
                       "appClaimedBytes": seeds["app"]},
        "missBytes": {k: misses[k] for k in MISS_CLASSES if misses[k]},
        "exploratory": {"names": names.block()},
    }


def score_sample(entry, report, truth, ix, rule):
    """Claude Code: the labelled sample, claims judged by span containment (decompose.cc), the stratified ratio
    estimator turning sample bytes into src bytes."""
    units = run_units(report)
    by_s = {u["s"]: u for u in statement_units(units)}
    st = statement_units(units)
    strata = truth["strata"]
    sb = collections.Counter()
    for t in truth["rows"]:
        sb[t["stratum"]] += t["b"]

    def est(t):
        return strata[t["stratum"]]["bytes"] * t["b"] / sb[t["stratum"]]
    cl = sorted((c["start"], c["end"], c["package"]) for c in report.get("claims", []))
    starts = [c[0] for c in cl]

    def claim_at(s):
        k = bisect.bisect_right(starts, s) - 1
        return cl[k][2] if k >= 0 and cl[k][0] <= s < cl[k][1] else None
    vet = {v["start"]: v["package"] for v in report.get("vetoed", [])}
    struct_claims = mmlib.claim_all(ix, st) if rule == "agreement" else {}
    total, claimed, misses = collections.Counter(), collections.Counter(), collections.Counter()
    units_claimed = collections.Counter()
    names = Names()
    for t in truth["rows"]:
        p = claim_at(t["s"])
        lib = "pkg" in t
        cls = "library" if lib else t["label"]
        total[cls] += est(t)
        if p is not None:
            if lib:
                v = "right" if p == t["pkg"] else (
                    "family" if mmlib.family(p) == mmlib.family(t["pkg"]) else "wrongFamily")
            else:
                v = {"<app>": "app"}.get(t["label"], t["label"].strip("<>"))
            claimed[v] += est(t)
            units_claimed[v] += 1
            if v in ("right", "family") and t["s"] in by_s:
                names.add(ix, by_s[t["s"]], p, t)
        if lib and not (p is not None and mmlib.family(p) == mmlib.family(t["pkg"])):
            u = by_s.get(t["s"])
            if u is None or "w" not in u:
                misses["unit-model"] += est(t)
            else:
                import decompose
                vetoed = {u["i"]: vet[u["s"]]} if u["s"] in vet else {}
                misses[decompose.classify(ix, u, t, p, vetoed, struct_claims, rule_name(rule))] += est(t)
    lib = total["library"]
    named = claimed["right"] + claimed["family"]
    named_any = named + claimed["wrongFamily"]
    all_claimed = sum(claimed.values())
    # every claimed exact unit of the whole bundle (names need no truth); the version share stays on the sample
    whole_names = Names()
    key = {(u["s"], u["e"]): u for u in st}
    for c in report.get("claims", []):
        u = key.get((c["start"], c["end"]))
        if u is not None:
            whole_names.add(ix, u, c["package"])
    nb = names.block()
    wb = whole_names.block()
    return {
        "totals": {
            "libraryBytes": round(lib),
            "namedBytes": round(named),
            "recall": share(named, lib),
            "appClaimedBytes": round(claimed["app"]),
            "appClaimedSampleUnits": units_claimed["app"],
            "wrongFamilyBytes": round(claimed["wrongFamily"]),
            "wrongFamilyShare": share(claimed["wrongFamily"], named_any),
            "wrongNameShare": share(claimed["wrongFamily"] + claimed["app"], all_claimed),
        },
        "estimate": "stratified ratio estimate from the 620-unit labelled sample (bytes are src-wide estimates)",
        "statementBytes": {k: round(total[k]) for k in sorted(total)},
        "claimedBytes": {k: round(claimed[k]) for k in sorted(claimed)},
        "claimedSampleUnits": {k: units_claimed[k] for k in sorted(units_claimed)},
        "bundleClaimedBytes": report["seedStatementBytes"] + report["grownStatementBytes"],
        "missBytes": {k: round(misses[k]) for k in MISS_CLASSES if misses[k]},
        "exploratory": {"names": {**wb, "bindingsVersionKnown": nb["bindingsVersionKnown"],
                                  "bindingsSameVersion": nb["bindingsSameVersion"],
                                  "sameVersionShare": nb["sameVersionShare"],
                                  "versionScope": "labelled sample units only"}},
    }


def score_floor(entry, truth):
    """The adapter floored the bundle (no module unpacked): the pipeline runs no discovery, so nothing is named.
    There is no unit model to count statements with: the denominator is the MAPPED library bytes."""
    if truth["method"] == "sample":
        raise SystemExit(f"{entry['id']}: no discovery ran on a sample-truth bundle")
    segs, srcs = truth["segments"], truth["sources"]
    lib = 0
    for k, (off, src) in enumerate(segs):
        end = segs[k + 1][0] if k + 1 < len(segs) else off
        if srcs[src][0] not in NOT_LIBRARY:
            lib += end - off
    return {
        "totals": {"libraryBytes": lib, "namedBytes": 0, "recall": 0.0, "appClaimedBytes": 0,
                   "wrongFamilyBytes": 0, "wrongFamilyShare": 0.0, "wrongNameShare": 0.0},
        "libraryBytesBasis": "mapped library bytes (no unit model: discovery never ran)",
        "statementBytes": {}, "claimedBytes": {},
        "indexSeeds": {"namedBytes": 0, "recall": 0.0, "wrongFamilyShare": 0.0, "appClaimedBytes": 0},
        "missBytes": {"no-discovery": lib},
        "exploratory": {"names": Names().block()},
    }


# ---------------------------------------------------------------- the label

def load_index(index_dir, slice_, index_id):
    mmlib.IX = index_dir
    mmlib.MM = os.path.join(DATA, "cache", "ix-" + index_id[:16])
    os.makedirs(mmlib.MM, exist_ok=True)
    return mmlib.Index(slice_)


def aggregate(cards):
    """Per set: summed bytes (held-out: every app's bytes together), shares recomputed from the sums."""
    out = {}
    for set_, cs in sorted(cards.items()):
        t, mb, nm, cb = collections.Counter(), collections.Counter(), collections.Counter(), collections.Counter()
        for c in cs:
            for k in ("libraryBytes", "namedBytes", "appClaimedBytes", "wrongFamilyBytes"):
                t[k] += c["totals"][k]
            t["claimedBytes"] += sum(v for k, v in c["claimedBytes"].items() if k not in ("unmapped", "foreign"))
            mb.update(c["missBytes"])
            nm.update(c["exploratory"]["names"])
            cb.update(c["claimedBytes"])
        named_any = t["namedBytes"] + t["wrongFamilyBytes"]
        nm.pop("sameVersionShare", None)
        nm.pop("versionScope", None)
        row = {
            "entries": len(cs),
            "discoveryRan": sum(1 for c in cs if c["discoveryRan"]),
            "bundlerDetected": sum(1 for c in cs if c["detected"]["bundler"] == c["toolchain"]["bundler"]),
            "libraryBytes": t["libraryBytes"], "namedBytes": t["namedBytes"],
            "recall": share(t["namedBytes"], t["libraryBytes"]),
            "appClaimedBytes": t["appClaimedBytes"],
            "wrongFamilyShare": share(t["wrongFamilyBytes"], named_any),
            "wrongNameShare": share(t["wrongFamilyBytes"] + t["appClaimedBytes"], t["claimedBytes"]),
            "selfOtherBytes": cb["selfOther"],
            "missShares": {k: share(mb[k], t["libraryBytes"]) for k in MISS_CLASSES if mb[k]},
            "exploratoryNames": {**{k: nm[k] for k in sorted(nm)},
                                 "sameVersionShare": share(nm["bindingsSameVersion"], nm["bindingsVersionKnown"])},
        }
        if set_ == "cc":
            row["appClaimedSampleUnits"] = cs[0]["totals"]["appClaimedSampleUnits"]
        out[set_] = row
    return out


def main():
    label = sys.argv[1]
    run_dir = os.path.join(DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    manifest = lib096.load_manifest()
    if pipeline["corpusSha256"] != lib096.manifest_sha():
        raise SystemExit("REFUSED: the label ran on another corpus manifest")
    out_dir = os.path.join(HERE, "results", label)
    os.makedirs(os.path.join(out_dir, "cards"), exist_ok=True)
    indexes = {}
    cards = collections.defaultdict(list)
    timing = {}
    for entry in manifest["entries"]:
        rp = os.path.join(run_dir, "reports", entry["id"] + ".json")
        doc = json.load(open(rp))
        report = doc["libraryDiscovery"]
        slice_ = doc["toolchain"]["slice"]
        truth = lib096.load_truth(entry)
        if report is None:
            body = score_floor(entry, truth)
        else:
            if report.get("indexId") != pipeline["indexId"]:
                raise SystemExit(f"REFUSED: {entry['id']} ran on index {report.get('indexId')}, label says "
                                 f"{pipeline['indexId']}")
            if slice_ not in indexes:
                indexes[slice_] = load_index(pipeline["indexDir"], slice_, pipeline["indexId"])
            fn = score_sample if truth["method"] == "sample" else score_map
            body = fn(entry, report, truth, indexes[slice_], pipeline["rule"])
        card = {"schema": SCORER_VERSION, "label": label, "entry": entry["id"], "set": entry["set"],
                "truthMethod": truth["method"], "bundleSha256": entry["files"]["bundle"]["sha256"],
                "indexId": pipeline["indexId"], "rule": pipeline["rule"], "toolchain": doc["toolchain"],
                "detected": doc["detected"], "discoveryRan": report is not None,
                "noDiscovery": doc.get("noDiscovery"), "reportSha256": lib096.sha256_file(rp), **body}
        open(os.path.join(out_dir, "cards", entry["id"] + ".json"), "w").write(dump(card))
        cards[entry["set"]].append(card)
        tp = os.path.join(run_dir, "reports", entry["id"] + ".run.json")
        if os.path.exists(tp):
            timing[entry["id"]] = json.load(open(tp))
        print(f"{entry['id']:60s} recall {body['totals']['recall']:.3f} app {body['totals']['appClaimedBytes']}",
              flush=True)
    sets = aggregate(cards)
    app_total = sum(r["appClaimedBytes"] for r in sets.values())
    summary = {
        "schema": SCORER_VERSION, "label": label,
        "config": {k: pipeline[k] for k in ("indexId", "indexBytes", "rule", "buildCommit", "binarySha256",
                                            "corpusSha256")},
        "bars": {"appClaimedBytes": app_total, "appBarPassed": app_total == 0,
                 "wrongNameBar": WRONG_NAME_BAR,
                 "wrongNameBarPassed": {s: r["wrongNameShare"] <= WRONG_NAME_BAR for s, r in sets.items()}},
        "sets": sets,
    }
    open(os.path.join(out_dir, "summary.json"), "w").write(dump(summary))
    walls = [t["wallSeconds"] for t in timing.values()]
    open(os.path.join(out_dir, "timing.json"), "w").write(dump({
        "note": "NOT deterministic: wall clock and peak memory of each `humanify discover` run (index load "
                "included); the cards and summary.json are the deterministic part",
        "totalWallSeconds": round(sum(walls), 1), "maxPeakRssMb": max((t["peakRssMb"] for t in timing.values()),
                                                                       default=0),
        "perEntry": timing}))
    print(dump(summary["bars"]))


if __name__ == "__main__":
    main()
