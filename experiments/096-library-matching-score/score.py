"""exp096: score one label's discovery reports — one CARD per (label, corpus entry), one summary per label.

usage: <venv python> -I score.py <label>
reads  <DATA>/results/<label>/reports/<id>.json   (`humanify discover --unit-prints` output)
       <DATA>/results/<label>/pipeline.json        (the runner's config: index dir, rule, binary)
writes experiments/096-library-matching-score/results/<label>/cards/<id>.json and summary.json (DETERMINISTIC:
       no clock, no host — the determinism check compares these bytes), timing.json (wall time, peak memory:
       NOT deterministic, kept out of the cards).

What a card says, totals first (statement bytes = the bytes each statement unit OWNS; units = the run's own unit
model, `unitRows`; a unit is library when most of its mapped bytes come from `node_modules/<pkg>`, its SHIPPER;
it also has an ORIGIN when most of those bytes lie in a vendored / pre-bundled copy of another package, see
origins.py). A claim is RIGHT when it names the shipper or the origin (or a member of either's family):
  recall              library bytes claimed right / library bytes
  namedAsShipper/     the two halves of the named bytes, so nothing is hidden: claimed as the shipper's family,
   namedAsOrigin      and claimed as the origin's family (and not the shipper's)
  *ShipperOnly        precision / recall / f1 under the OLD truth (shipper only, scorer 1)
  copyBytes           library bytes whose unit has an origin (a recognisable copy), named or not
  appClaimedBytes     bytes of APP code (not from node_modules) claimed as any package — the hard bar: 0
  precision           named / (named + wrong family + app claimed): of the bytes a claim put a package name on
                      and the truth can judge, the share named right. Claims on the app's own package code (self,
                      selfOther, selfOrigin), unmapped and foreign bytes are neither: reported apart, not judged.
                      Read against the bar: precision >= 99.8% (the ~0.2% wrong-name bar of exp095's scale lane)
  f1                  2PR / (P + R), the harmonic mean of precision and recall
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
PRECISION_BAR = 0.998
# truth labels that are not a library package: app code, the app's own package, no mapping, a chained URL source
NOT_LIBRARY = ("<app>", "<self>", "<unmapped>", "<foreign>")
# 2: origin-aware truth (origins.py): a claim naming a vendored copy's origin is right; the shipper-only shares
#    are kept beside it (*ShipperOnly). Same reports, re-scored: scorer-1 cards are not comparable.
# 3: precision / recall / F1 replace the wrong-family and wrong-name shares (same bytes, standard measures).
SCORER_VERSION = "exp096-score/3"


def r6(x):
    return round(x, 6)


def share(n, d):
    return r6(n / d) if d else 0.0


def prf(named, wrong_family, app, library, suffix=""):
    """Precision, recall and F1 over bytes. True positives are the named bytes (recall's numerator); false
    positives are bytes claimed as the wrong family or app code claimed as any package. Claims the truth cannot
    judge (the app's own package, unmapped, foreign) are neither. Nothing claimed reads precision 0 (the usual
    zero-division convention), so an empty set fails the bar rather than passing it vacuously."""
    p = share(named, named + wrong_family + app)
    r = share(named, library)
    return {"precision" + suffix: p, "recall" + suffix: r, "f1" + suffix: share(2 * p * r, p + r)}


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


def label_units(truth, units, origins=None):
    """truth.py label_units over the frozen segment table: majority mapped bytes; for a library label the
    version/file of the source holding most of its bytes. With the origin layer: `origin` = the package holding
    the MAJORITY of the label's own bytes by origin (no origin counts as a candidate too), when it is not the
    label itself. Also for <self> units (a copy inside the app's own package)."""
    segs = truth["segments"]
    srcs = truth["sources"]
    offs = [s[0] for s in segs]
    run_at = [r[0] for r in origins["runs"]] if origins else []
    out = {}
    for u in statement_units(units):
        s, e = u["s"], u["e"]
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        votes, files = collections.Counter(), collections.Counter()
        by_origin = collections.defaultdict(collections.Counter)
        while k < len(segs) and segs[k][0] < e:
            a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                votes[srcs[segs[k][1]][0]] += b - a
                files[segs[k][1]] += b - a
                if run_at:
                    r = origins["runs"][bisect.bisect_right(run_at, k) - 1]
                    by_origin[srcs[segs[k][1]][0]][(r[1], r[2])] += b - a
            k += 1
        lab = votes.most_common(1)[0][0] if votes else "<unmapped>"
        row = {"label": lab}
        if lab not in NOT_LIBRARY:
            src = next(si for si, _ in files.most_common() if srcs[si][0] == lab)
            row.update(pkg=lab, version=srcs[src][1], file=srcs[src][2])
        if origins and lab in by_origin:
            (oi, ki), _ = sorted(by_origin[lab].items(), key=lambda kv: (-kv[1], kv[0]))[0]
            if oi >= 0:
                row.update(origin=origins["names"][oi], originEvidence=origins["kinds"][ki])
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


def judge_origin(claimed, row, app_pkg):
    """The origin-aware verdict: judge() against the shipper, then a claim naming the unit's ORIGIN's family is
    `origin` (right under the new truth). A <self> unit claimed as its origin is `selfOrigin` (reported apart, as
    selfOther is). Returns (new verdict, shipper-only verdict)."""
    old = judge(claimed, row["label"], app_pkg)
    origin = row.get("origin")
    if origin is not None and old in ("wrongFamily", "selfOther") \
            and mmlib.family(claimed) == mmlib.family(origin):
        return ("origin" if old == "wrongFamily" else "selfOrigin"), old
    return old, old


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


def score_map(entry, report, truth, ix, rule, origins):
    units = run_units(report)
    by_i = {u["i"]: u for u in units}
    st = statement_units(units)
    labels = label_units(truth, units, origins)
    app_pkg = truth["appPackage"]
    key = {(u["s"], u["e"]): u["i"] for u in st}
    claims_all = report.get("claims", [])
    claims = {key[(c["start"], c["end"])]: c for c in claims_all if (c["start"], c["end"]) in key}
    vetoed = {key[k]: p for k, p in spans(report.get("vetoed", [])).items() if k in key}
    total = collections.Counter()
    for u in st:
        lab = labels[u["i"]]["label"]
        total["library" if lab not in NOT_LIBRARY else lab] += u["b"]
        if lab not in NOT_LIBRARY and "origin" in labels[u["i"]]:
            total["copy"] += u["b"]
    claimed, seeds, old = collections.Counter(), collections.Counter(), collections.Counter()
    names = Names()
    for i, c in claims.items():
        v, vo = judge_origin(c["package"], labels[i], app_pkg)
        claimed[v] += by_i[i]["b"]
        old[vo] += by_i[i]["b"]
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
        if c and judge_origin(c["package"], t, app_pkg)[0] in ("right", "family", "origin"):
            continue
        if "w" not in u:
            misses["unit-model"] += u["b"]
            continue
        import decompose
        misses[decompose.classify(ix, u, t, c["package"] if c else None, vetoed, struct_claims,
                                  rule_name(rule))] += u["b"]
    lib = total["library"]
    as_shipper = claimed["right"] + claimed["family"]
    named = as_shipper + claimed["origin"]
    old_named = old["right"] + old["family"]
    seed_named = seeds["right"] + seeds["family"] + seeds["origin"]
    return {
        "totals": {
            "libraryBytes": lib,
            "namedBytes": named,
            **prf(named, claimed["wrongFamily"], claimed["app"], lib),
            "appClaimedBytes": claimed["app"],
            "wrongFamilyBytes": claimed["wrongFamily"],
            "namedAsShipperBytes": as_shipper,
            "namedAsOriginBytes": claimed["origin"],
            "copyBytes": total["copy"],
            "namedBytesShipperOnly": old_named,
            **prf(old_named, old["wrongFamily"], old["app"], lib, "ShipperOnly"),
            "wrongFamilyBytesShipperOnly": old["wrongFamily"],
        },
        "statementBytes": {k: total[k] for k in sorted(total)},
        "claimedBytes": {k: claimed[k] for k in sorted(claimed)},
        "indexSeeds": {"namedBytes": seed_named, **prf(seed_named, seeds["wrongFamily"], seeds["app"], lib),
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
            **prf(named, claimed["wrongFamily"], claimed["app"], lib),
            "appClaimedBytes": round(claimed["app"]),
            "appClaimedSampleUnits": units_claimed["app"],
            "wrongFamilyBytes": round(claimed["wrongFamily"]),
            # the sample truth is hand-labelled per unit (forks = app): no origin layer, both truths agree
            "namedAsShipperBytes": round(named),
            "namedAsOriginBytes": 0,
            "copyBytes": 0,
            "namedBytesShipperOnly": round(named),
            **prf(named, claimed["wrongFamily"], claimed["app"], lib, "ShipperOnly"),
            "wrongFamilyBytesShipperOnly": round(claimed["wrongFamily"]),
        },
        "originTruth": "none: sample truth (hand-labelled units)",
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
        "totals": {"libraryBytes": lib, "namedBytes": 0, **prf(0, 0, 0, lib), "appClaimedBytes": 0,
                   "wrongFamilyBytes": 0,
                   "namedAsShipperBytes": 0, "namedAsOriginBytes": 0, "copyBytes": 0, "namedBytesShipperOnly": 0,
                   **prf(0, 0, 0, lib, "ShipperOnly"), "wrongFamilyBytesShipperOnly": 0},
        "libraryBytesBasis": "mapped library bytes (no unit model: discovery never ran)",
        "statementBytes": {}, "claimedBytes": {},
        "indexSeeds": {"namedBytes": 0, **prf(0, 0, 0, lib), "appClaimedBytes": 0},
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
            for k in ("libraryBytes", "namedBytes", "appClaimedBytes", "wrongFamilyBytes", "namedAsShipperBytes",
                      "namedAsOriginBytes", "copyBytes", "wrongFamilyBytesShipperOnly", "namedBytesShipperOnly"):
                t[k] += c["totals"][k]
            mb.update(c["missBytes"])
            nm.update(c["exploratory"]["names"])
            cb.update(c["claimedBytes"])
        nm.pop("sameVersionShare", None)
        nm.pop("versionScope", None)
        row = {
            "entries": len(cs),
            "discoveryRan": sum(1 for c in cs if c["discoveryRan"]),
            "bundlerDetected": sum(1 for c in cs if c["detected"]["bundler"] == c["toolchain"]["bundler"]),
            "libraryBytes": t["libraryBytes"], "namedBytes": t["namedBytes"],
            **prf(t["namedBytes"], t["wrongFamilyBytes"], t["appClaimedBytes"], t["libraryBytes"]),
            "appClaimedBytes": t["appClaimedBytes"], "wrongFamilyBytes": t["wrongFamilyBytes"],
            "namedAsShipperBytes": t["namedAsShipperBytes"], "namedAsOriginBytes": t["namedAsOriginBytes"],
            "copyBytes": t["copyBytes"],
            **prf(t["namedBytesShipperOnly"], t["wrongFamilyBytesShipperOnly"], t["appClaimedBytes"],
                  t["libraryBytes"], "ShipperOnly"),
            "selfOtherBytes": cb["selfOther"],
            "selfOriginBytes": cb["selfOrigin"],
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
    om = lib096.load_origins_manifest()
    if om["corpusSha256"] != pipeline["corpusSha256"]:
        raise SystemExit("REFUSED: origins.json was frozen for another corpus manifest")
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
            if truth["method"] == "sample":
                body = score_sample(entry, report, truth, indexes[slice_], pipeline["rule"])
            else:
                body = score_map(entry, report, truth, indexes[slice_], pipeline["rule"],
                                 lib096.load_origins(entry, om))
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
        print(f"{entry['id']:60s} P {body['totals']['precision']:.3f} R {body['totals']['recall']:.3f} F1 {body['totals']['f1']:.3f} app {body['totals']['appClaimedBytes']}",
              flush=True)
    sets = aggregate(cards)
    app_total = sum(r["appClaimedBytes"] for r in sets.values())
    summary = {
        "schema": SCORER_VERSION, "label": label,
        "config": {**{k: pipeline[k] for k in ("indexId", "indexBytes", "rule", "buildCommit", "binarySha256",
                                               "corpusSha256")}, "originsSha256": lib096.origins_sha(),
                   **({"settings": pipeline["settings"]} if pipeline.get("settings") else {})},
        "bars": {"appClaimedBytes": app_total, "appBarPassed": app_total == 0,
                 "precisionBar": PRECISION_BAR,
                 "precisionBarPassed": {s: r["precision"] >= PRECISION_BAR for s, r in sets.items()}},
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
