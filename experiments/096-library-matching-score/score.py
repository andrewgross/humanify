"""exp096: score one label's discovery reports — one CARD per (label, corpus entry), one summary per label.

usage: <venv python> -I score.py <label> [--corpus dev|test]
       (the corpus must be the one the label ran on: pipeline.json's `corpus`, absent = dev. A TEST label's cards
       stay under the test data dir, beside its reports, and its committed summary holds per-set aggregates only)
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
  f05                 F0.5 = 1.25PR / (0.25P + R): precision weighted over recall (beta 0.5), 0 when P + R = 0. The
                      HEADLINE from scorer 5 on (held-out and test F0.5, under the 99.8% precision bar, app claimed 0)
  misses              every library unit NOT named, one cause each (the exp095 misses lane's classifier,
                      decompose.classify, unchanged): a not indexed / b version missing / c too small /
                      d hash differs (d-file, d-variant, d-absent, d-nofile) / e guard refused (e-veto, e-agree,
                      e-support, e-spec, e-ambig, e-other) / f in the index as another package
  exploratory.names   EXPLORATORY: units claimed through an exact whole-unit hash, the local bindings they
                      declare, and the share of those whose index rows hold the truth's exact version (the index
                      builds that version's npm source, so its names would be the author's, up to a bundler's
                      collision renames). No name is read from the index: it stores none today.
Scorer 4 (exp104 truth audit): families also joined by npm metadata (families_lib), units the map never mapped
<unmapped> (truth_audit `carried`), dependency-gated named vendored copies as origins (truth_audit `copies`).
Scorer 5 (exp106): rollup `?commonjs-*` virtual ids are <unmapped>; a source reached through an installed package's
own source map keeps that package, never <app> (truth_audit.patch_sources).
Scorer 6 (exp107): the source map decoded as the spec writes it (decoder.py: UTF-16 generated columns, one-field
segments = unmapped), at score time from the frozen bundle and map (truth_audit.redecode); a carried unit with
nothing to check is <unmapped> (truth_audit carried_verdict).
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
from lib096 import HERE, INDEX_CACHE, dump  # noqa: E402

import mmlib  # noqa: E402  (the misses lane: index table, structural-rule emulation)
import families_lib  # noqa: E402
import truth_audit  # noqa: E402

FAMILIES = families_lib.Families()
same_family = FAMILIES.same

MISS_CLASSES = ["no-discovery", "a", "b", "c", "d-file", "d-variant", "d-absent", "d-nofile", "e-veto", "e-agree", "e-support",
                "e-spec", "e-ambig", "e-other", "f", "unit-model"]
PRECISION_BAR = 0.998
# truth labels that are not a library package: app code, the app's own package, no mapping, a chained URL source
NOT_LIBRARY = ("<app>", "<self>", "<unmapped>", "<foreign>")
# 2: origin-aware truth (origins.py): a claim naming a vendored copy's origin is right; the shipper-only shares
#    are kept beside it (*ShipperOnly). Same reports, re-scored: scorer-1 cards are not comparable.
# 3: precision / recall / F1 replace the wrong-family and wrong-name shares (same bytes, standard measures).
# 4: the exp104 truth audit, three mechanical corrections (experiments/104-truth-audit/README.md): families joined
#    by npm metadata (families_lib: one repository, renamed-to, GitHub fork under a scope); a unit the source map
#    never mapped and the previous mapping's source does not contain is <unmapped> (truth_audit: carried); a
#    dependency-gated, named vendored copy gets an origin (truth_audit: copies). Same reports, re-scored.
# 5: the exp106 source-table corrections (truth_audit.patch_sources): a rollup `?commonjs-*` virtual id is
#    <unmapped> (bundler glue), and a source reached through an installed package's OWN source map (esbuild chained
#    @angular/compiler's map out of node_modules) keeps that package instead of <app> (frozen layer chained.json).
#    Same reports, re-scored; both rules off reproduces scorer 4's cards (verify_s4.py). Also F0.5 (`f05`, beta 0.5:
#    precision over recall) beside precision / recall / F1 in every prf() block — the campaign's headline metric.
# 6: the exp107 decoder (decoder.py, truth_audit.redecode): generated columns read as UTF-16 code units (scorer 4
#    read code points: an emoji on a minified line moved every later mapping on it), and one-field segments kept as
#    unmapped (the bundler's own glue is no longer credited to the mapping before it); a unit with no mapping of its
#    own and too few tokens to check against the source it was carried from is <unmapped> (truth_audit
#    carried_verdict, `unverified`). Same reports, re-scored; the decoder rules off reproduce scorer 4's segments
#    exactly (experiments/107-zero-app-claims/verify_decoder.py); all three off reproduces scorer 5's cards
#    (verify_s5.py). Built on 5: patch_sources (5) first, then redecode (6).
# 7: the exp112 origin layer (origins.py RULES7, frozen by experiments/112-test-precision/scorer7/origins7.py):
#    a rolldown lazy module body emitted after its region's `//#endregion` is that region's package (lazybody),
#    and a file named after a package its owner declares as a build input only (npm devDependencies / bundled, or
#    the tarball's own license collection) is a copy of it (chunk). Same reports, re-scored; every rule off
#    reproduces scorer 6's cards (verify_s6.py; the layer is byte-identical: origins7.py verify).
# 8: exp119 re-derived families-derived.json (exp104's derive_families.py, rules unchanged) over every package of
#    the default index k2-all plus the earlier universe (17,316 names): same repository, renamed-to, GitHub-recorded
#    fork, pairwise. family() reads more links, nothing else changed; the frozen copies layers keep the families
#    they were built with. Same reports, re-scored (experiments/119-families-full/README.md, ledger.py).
# 9: exp120, two truth-side fixes (experiments/120-truth-families/README.md). (a) families-derived.json re-derived
#    (exp104's rules unchanged) over the exp119 universe plus every package NAME any corpus's truth labels with (the
#    truths' source tables, chained, origin and copies layers): a truth package outside the universe (a fork such
#    as @snyk/graphlib) was in no derived family. (b) rule `npmchain` (truth_audit RULES9, frozen layer chained9):
#    a chained source whose path names a package npm does not have (`@primevue/src`, from @primevue/core's own map)
#    keeps the real package whose map listed it. Same reports, re-scored; RULES9 off + scorer 8's links file
#    reproduces scorer 8's cards.
SCORER_VERSION = "exp096-score/9"


def r6(x):
    return round(x, 6)


def share(n, d):
    return r6(n / d) if d else 0.0


def prf(named, wrong_family, app, library, suffix=""):
    """Precision, recall, F1 and F0.5 over bytes (F0.5: beta = 0.5, precision weighted over recall — the
    campaign's headline from scorer 5 on). True positives are the named bytes (recall's numerator); false
    positives are bytes claimed as the wrong family or app code claimed as any package. Claims the truth cannot
    judge (the app's own package, unmapped, foreign) are neither. Nothing claimed reads precision 0 (the usual
    zero-division convention), so an empty set fails the bar rather than passing it vacuously."""
    p = share(named, named + wrong_family + app)
    r = share(named, library)
    return {"precision" + suffix: p, "recall" + suffix: r, "f1" + suffix: share(2 * p * r, p + r),
            "f05" + suffix: share(1.25 * p * r, 0.25 * p + r)}


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


def label_units(truth, units, origins=None, audit=None):
    """truth.py label_units over the frozen segment table: majority mapped bytes; for a library label the
    version/file of the source holding most of its bytes. With the origin layer: `origin` = the package holding
    the MAJORITY of the label's own bytes by origin (no origin counts as a candidate too), when it is not the
    label itself. Also for <self> units (a copy inside the app's own package). With the scorer-4 `audit`
    (truth_audit.Audit): a carried unit is <unmapped> (`carriedFrom` keeps the old label), and a library unit with
    no origin takes a frozen copy range's origin (`originEvidence` "copy")."""
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
        if audit is not None and lab != "<unmapped>" and audit.carried(s, e):
            out[u["i"]] = {"label": "<unmapped>", "carriedFrom": lab}
            continue
        row = {"label": lab}
        if lab not in NOT_LIBRARY:
            src = next(si for si, _ in files.most_common() if srcs[si][0] == lab)
            row.update(pkg=lab, version=srcs[src][1], file=srcs[src][2])
        if origins and lab in by_origin:
            (oi, ki), _ = sorted(by_origin[lab].items(), key=lambda kv: (-kv[1], kv[0]))[0]
            if oi >= 0:
                row.update(origin=origins["names"][oi], originEvidence=origins["kinds"][ki])
        if audit is not None and lab not in NOT_LIBRARY and "origin" not in row:
            co = audit.copy_origin(s, e, lab)
            if co is not None:
                row.update(origin=co[0], originEvidence="copy")
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
    return "family" if same_family(claimed, truth_label) else "wrongFamily"


def judge_origin(claimed, row, app_pkg):
    """The origin-aware verdict: judge() against the shipper, then a claim naming the unit's ORIGIN's family is
    `origin` (right under the new truth). A <self> unit claimed as its origin is `selfOrigin` (reported apart, as
    selfOther is). Returns (new verdict, shipper-only verdict)."""
    old = judge(claimed, row["label"], app_pkg)
    origin = row.get("origin")
    if origin is not None and old in ("wrongFamily", "selfOther") \
            and same_family(claimed, origin):
        return ("origin" if old == "wrongFamily" else "selfOrigin"), old
    return old, old


# ---------------------------------------------------------------- the exploratory names column

def exact_rows(ix, u, package):
    """The index rows that hold u's WHOLE hash under package's family (an exact unit match), or []."""
    if "w" not in u:
        return []
    return [r for r in ix.units.get(u["w"][0], ()) if same_family(r[0], package)]


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


def score_map(entry, report, truth, ix, rule, origins, audit):
    units = run_units(report)
    by_i = {u["i"]: u for u in units}
    st = statement_units(units)
    labels = label_units(truth, units, origins, audit)
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
                    "family" if same_family(p, t["pkg"]) else "wrongFamily")
            else:
                v = {"<app>": "app"}.get(t["label"], t["label"].strip("<>"))
            claimed[v] += est(t)
            units_claimed[v] += 1
            if v in ("right", "family") and t["s"] in by_s:
                names.add(ix, by_s[t["s"]], p, t)
        if lib and not (p is not None and same_family(p, t["pkg"])):
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
    mmlib.MM = os.path.join(INDEX_CACHE, "ix-" + index_id[:16])
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
    argv = lib096.corpus_flag(sys.argv)
    label = argv[1]
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    if pipeline.get("corpus", "dev") != lib096.CORPUS:
        raise SystemExit(f"REFUSED: the label ran on the {pipeline.get('corpus', 'dev')} corpus — pass --corpus")
    test = lib096.CORPUS in lib096.PRIVATE
    manifest = lib096.load_manifest()
    if pipeline["corpusSha256"] != lib096.manifest_sha():
        raise SystemExit("REFUSED: the label ran on another corpus manifest")
    om = lib096.load_origins_manifest()
    if om["corpusSha256"] != pipeline["corpusSha256"]:
        raise SystemExit("REFUSED: origins.json was frozen for another corpus manifest")
    cm = truth_audit.load_copies_manifest()
    if cm["corpusSha256"] != pipeline["corpusSha256"]:
        raise SystemExit("REFUSED: the copies layer was frozen for another corpus manifest")
    chm = truth_audit.load_chained_manifest()
    if chm["corpusSha256"] != pipeline["corpusSha256"]:
        raise SystemExit("REFUSED: the chained-sources layer was frozen for another corpus manifest")
    c9m = truth_audit.load_chained9_manifest()
    if c9m is not None and c9m["corpusSha256"] != pipeline["corpusSha256"]:
        raise SystemExit("REFUSED: the scorer 9 chained layer was frozen for another corpus manifest")
    o7m = truth_audit.load_origins7_manifest()
    if o7m is not None and o7m["corpusSha256"] != pipeline["corpusSha256"]:
        raise SystemExit("REFUSED: the scorer 7 origin layer was frozen for another corpus manifest")
    out_dir = os.path.join(HERE, "results", label)
    # a test label's per-bundle cards are NOT committed (tuning lanes must not read per-app test misses)
    cards_dir = os.path.join(run_dir if test else out_dir, "cards")
    os.makedirs(out_dir, exist_ok=True)
    os.makedirs(cards_dir, exist_ok=True)
    indexes = {}
    cards = collections.defaultdict(list)
    timing = {}
    for entry in manifest["entries"]:
        if pipeline.get("sets") and entry["set"] not in pipeline["sets"]:
            continue  # the label ran a subset of the corpus (run.py --sets)
        rp = os.path.join(run_dir, "reports", entry["id"] + ".json")
        doc = json.load(open(rp))
        report = doc["libraryDiscovery"]
        slice_ = doc["toolchain"]["slice"]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m) if entry["truth"]["method"] == "sourcemap" else None
        truth, origins = truth_audit.redecode(entry, truth, origins)
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
                body = score_map(entry, report, truth, indexes[slice_], pipeline["rule"], origins,
                                 truth_audit.Audit(entry, truth, cm))
        card = {"schema": SCORER_VERSION, "label": label, "entry": entry["id"], "set": entry["set"],
                "truthMethod": truth["method"], "bundleSha256": entry["files"]["bundle"]["sha256"],
                "indexId": pipeline["indexId"], "rule": pipeline["rule"], "toolchain": doc["toolchain"],
                "detected": doc["detected"], "discoveryRan": report is not None,
                "noDiscovery": doc.get("noDiscovery"), "reportSha256": lib096.sha256_file(rp), **body}
        open(os.path.join(cards_dir, entry["id"] + ".json"), "w").write(dump(card))
        cards[entry["set"]].append(card)
        tp = os.path.join(run_dir, "reports", entry["id"] + ".run.json")
        if os.path.exists(tp):
            timing[entry["id"]] = json.load(open(tp))
        if not test:
            print(f"{entry['id']:60s} P {body['totals']['precision']:.3f} R {body['totals']['recall']:.3f} F0.5 {body['totals']['f05']:.3f} F1 {body['totals']['f1']:.3f} app {body['totals']['appClaimedBytes']}",
              flush=True)
    sets = aggregate(cards)
    app_total = sum(r["appClaimedBytes"] for r in sets.values())
    summary = {
        "schema": SCORER_VERSION, "label": label,
        "config": {**{k: pipeline[k] for k in ("indexId", "indexBytes", "rule", "buildCommit", "binarySha256",
                                               "corpusSha256")}, "originsSha256": lib096.origins_sha(),
                   "copiesSha256": truth_audit.copies_sha(), "familiesSha256": families_lib.derived_sha(),
                   "chainedSha256": truth_audit.chained_sha(),
                   **({"origins7Sha256": truth_audit.origins7_sha()} if o7m else {}),
                   **({"chained9Sha256": truth_audit.chained9_sha()} if c9m else {}),
                   **({"settings": pipeline["settings"]} if pipeline.get("settings") else {}),
                   **({"corpus": lib096.CORPUS} if lib096.CORPUS != "dev" else {}),
                   **({"sets": pipeline["sets"]} if pipeline.get("sets") else {})},
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
        **({} if test else {"perEntry": timing})}))
    print(dump(summary["bars"]))


if __name__ == "__main__":
    main()
