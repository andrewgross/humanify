#!/usr/bin/env python3
"""exp167: every library byte discovery did NOT name right, split into ONE exclusive cause.

usage: <exp095 venv python> -I census.py bun-dev <reports dir> <out dir> [--selftest]
       <exp095 venv python> -I census.py cc <report.json> <out dir>

Reads `humanify discover --unit-prints --call-context --bundle-order` reports (names-mode config, index cb64c1f5),
labels each unit with exp096's scorer-14 truth (score.label_units with every frozen layer, exactly as score.py
does), and re-derives the scorer's verdicts (named / wrong family / app claimed) so the totals can be checked
against a scorer card. Every library unit not named right gets ONE cause, first match wins:

  wrong-package       claimed, but as another family (precision; listed apart)
  unit-model          the unit carries no print (an uncut Factory / no whole print)
  not-indexed         neither the shipper nor its origin is a package of the index
                        .refused  every pinned version refused at build (pinyin) / .never  never pinned
  guard.*             the right family HOLDS prints covering >= half the unit at the lookup's 30-node floor, yet
                      no claim: .veto (the isolation veto dropped the right family), .tie (another family wins the
                      structural score), .window (structural says right, under 100 nodes, no same-package claim
                      within 10 units), .specificity, .multi-package (> 3 owners), .discovery (structural says
                      right and it would survive the window: refused by a discovery step), .other
  held-by-other       only OTHER families hold prints covering >= half the unit (shared code / copies)
  sub-floor           every print of the unit is under the index's 20-node row floor: no row can exist
  version-absent      the (package, version) is not indexed and no indexed version of the family holds the print
                        .skew  another indexed version holds the same file (its prints differ)
                        .nofile  no indexed version holds that file
  lookup-floor        family holds the unit's prints only at 20-29 nodes (rows exist, the lookup reads >= 30)
  print-differs       version indexed, prints >= 20 nodes, no family row holds them
                        .file  the file is indexed at that version  .absent  the file is not  .nofile  no file

Attributes beside the cause (not exclusive): `mates` = whether OTHER units of the same CommonJS module (FactoryBody
units share `m`) are claimed as the right family (a propagation gap), `queued` = the unit sits in a queue region.
Labels: the scorer's vocabulary is printed (the app spelling is "<app>", not "app" — exp165's miscount).
"""
import bisect
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
S096 = os.path.normpath(os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, S096)
import lib096  # noqa: E402

CORPUS = sys.argv[1] if len(sys.argv) > 1 else "bun-dev"
lib096.use(CORPUS)
import score  # noqa: E402
import truth_audit  # noqa: E402
import mmlib  # noqa: E402
import decompose  # noqa: E402

INDEX_DIR = "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28"
INDEX_ID = os.path.basename(INDEX_DIR)
ROW_FLOOR = 20
R20 = dict(mmlib.RULES, min_nodes=ROW_FLOOR)
SNIP = 160
PLANT = "--plant" in sys.argv
planted = set()


def refused_packages():
    m = json.load(open(os.path.join(INDEX_DIR, "manifest.json")))
    by = collections.defaultdict(collections.Counter)
    for r in m["refused"]:
        by[r["name"]][r["reason"][:40]] += 1
    return by, {(r["name"], r["version"]): r["reason"][:40] for r in m["refused"]}


REFUSED_PKG, REFUSED_VER = refused_packages()
_STOP = json.load(open(os.path.join(INDEX_DIR, "stoplists.json")))


def stoplisted(p):
    return p in _STOP["helperPackages"] or any(p.startswith(x) for x in _STOP["helperPackagePrefixes"])


def best_families(ix, u, rules=None):
    """The structural rule's per-package score (mmlib.structural's scoring, guards as `rules`): the packages tied at
    the best score, or []."""
    rules = rules or mmlib.RULES
    whole, ok_w, ok_f = mmlib.eligible(u, rules)
    spec = rules["spec"]
    score = {}
    if ok_w:
        o = ix.owners(whole[2], u["mb"], spec)
        if o and len(o) <= rules["max_packages"]:
            for p in o:
                score[p] = whole[3]
    if not score:
        hits = []
        for f in ok_f:
            o = ix.owners(f[2], u["mb"], spec)
            if o and len(o) <= rules["max_packages"]:
                hits.append((f, o))
        hits.sort(key=lambda x: -x[0][3])
        taken = []
        for f, o in hits:
            if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
                continue
            taken.append((f[0], f[1]))
            for p in o:
                score[p] = score.get(p, 0) + f[3]
    if not score:
        return []
    best = max(score.values())
    return sorted(p for p, v in score.items() if v == best)


def multi_family(tied):
    """True when the tied packages span more than one family."""
    return any(not same(a, tied[0]) for a in tied[1:])


def same(a, b):
    return a is not None and b is not None and score.same_family(a, b)


def right_family(p, t):
    return same(p, t["pkg"]) or same(p, t.get("origin")) or same(p, t.get("via"))


def raw_cover(u, hit):
    """Nodes covered by prints `hit` accepts with NO lookup floor (the index's own 20-node row floor only): the
    whole print, else the largest non-overlapping functions."""
    whole, fns = mmlib.prints(u)
    if whole[3] >= ROW_FLOOR and hit(whole):
        return whole[3]
    return mmlib.greedy_cover([f for f in fns if f[3] >= ROW_FLOOR and hit(f)])


def prints_max(u):
    whole, fns = mmlib.prints(u)
    return max([whole[3]] + [f[3] for f in fns])


def classify(ix, u, t, claim, vetoed, claims_by_i, q_index, q_list, queued):
    """-> (cause, detail dict)."""
    P, V, F = t["pkg"], t.get("version"), t.get("file")
    if claim is not None:
        return "wrong-package", {"claimed": claim["package"], "rule": claim["rule"]}
    if u["k"] == "Factory" or "w" not in u:
        return "unit-model", {}
    if not ix.has_package(P) and not (t.get("origin") and ix.has_package(t["origin"])):
        if stoplisted(P):
            return "not-indexed.stoplisted", {}
        if P in REFUSED_PKG:
            return "not-indexed.refused", {"why": dict(REFUSED_PKG[P])}
        return "not-indexed.never", {}
    whole, fns = mmlib.prints(u)
    wn = max(whole[3], 1)

    def fam_hit(p):
        return any(right_family(r[0], t) for r in ix.units.get(p[2], ()))

    def other_hit(p):
        return any(not right_family(r[0], t) for r in ix.units.get(p[2], ()))
    cov = mmlib.cover_by(u, fam_hit)
    if cov >= 0.5 * wn:
        if u["i"] in vetoed and right_family(vetoed[u["i"]], t):
            return "guard.veto", {"vetoed": vetoed[u["i"]]}
        r0 = dict(mmlib.RULES)
        h = mmlib.structural(ix, u, r0)
        if h and right_family(h[0], t):
            if h[1] < r0["strong"]:
                k = q_index.get(u["i"])
                ok = False
                if k is not None:
                    lo, hi = max(0, k - r0["support"]), min(len(q_list), k + r0["support"] + 1)
                    ok = any(j != k and claims_by_i.get(q_list[j]) is not None
                             and right_family(claims_by_i[q_list[j]]["package"], t) for j in range(lo, hi))
                if not ok:
                    return "guard.window", {"covered": h[1]}
            tied = best_families(ix, u)
            if multi_family(tied):
                rival = next((p for p in tied if not right_family(p, t)), "(shipper+origin)")
                return "guard.tie-fenced", {"covered": h[1], "rival": rival}
            return "guard.presence", {"covered": h[1], "queued": queued}
        if h:
            return "guard.tie", {"winner": h[0]}
        h = mmlib.structural(ix, u, dict(r0, spec=None))
        if h and right_family(h[0], t):
            return "guard.specificity", {}
        h = mmlib.structural(ix, u, dict(r0, spec=None, max_packages=10 ** 6))
        if h and right_family(h[0], t):
            owners = set()
            for p in [whole] + fns:
                owners |= {r[0] for r in ix.units.get(p[2], ())}
            return "guard.multi-package", {"owners": len(owners)}
        return "guard.other", {}
    if mmlib.cover_by(u, other_hit) >= 0.5 * wn:
        hold = collections.Counter()
        for p in [whole] + fns:
            for r in ix.units.get(p[2], ()):
                hold[r[0]] += 1
        return "held-by-other", {"holders": [p for p, _ in hold.most_common(3)]}
    mx = prints_max(u)
    if mx < ROW_FLOOR:
        return "sub-floor", {"maxNodes": mx}
    raw = raw_cover(u, fam_hit)
    if raw >= 0.5 * wn:
        # the family holds the unit's own prints, but the lookup's floors (30 nodes, 3 content tokens for a
        # whole print) keep them from being looked up
        why = "content<3" if whole[3] >= mmlib.RULES["min_nodes"] and whole[4] < mmlib.RULES["min_content"] \
            else "nodes<30"
        return "lookup-floor", {"maxNodes": mx, "why": why}
    has_v = ix.has_version(P, V) or (t.get("origin") is not None and ix.has_version(t["origin"], t.get("version")))
    if not has_v:
        d = {"refusedVersion": REFUSED_VER.get((P, V))}
        if F is not None and ix.file_versions(P, F):
            return "version-absent.skew", d
        return "version-absent.nofile", d
    if F is None:
        return "print-differs.nofile", {}
    if V in ix.file_versions(P, F):
        return "print-differs.file", {"maxNodes": mx}
    return "print-differs.absent", {}


def queue_spans(report):
    q = sorted((r["start"], r["end"]) for r in report.get("queue", []))
    return q, [a for a, _ in q]


def in_spans(spans, starts, s):
    k = bisect.bisect_right(starts, s) - 1
    return k >= 0 and spans[k][0] <= s < spans[k][1]


def snippet(code, s, e):
    t = code[s:min(e, s + SNIP)].decode("utf-8", errors="replace")
    return t + ("…" if e - s > SNIP else "")


class Tally:
    def __init__(self):
        self.bytes = collections.Counter()
        self.units = collections.Counter()
        self.pkg = collections.defaultdict(collections.Counter)
        self.mates = collections.defaultdict(collections.Counter)
        self.detail = collections.defaultdict(collections.Counter)
        self.examples = collections.defaultdict(list)
        self.bundles = collections.defaultdict(collections.Counter)

    def add(self, cause, b, pkg, mates, det, ex, bundle):
        self.bytes[cause] += b
        self.units[cause] += 1
        self.pkg[cause][pkg] += b
        self.mates[cause][mates] += b
        self.bundles[cause][bundle] += b
        for k, v in det.items():
            if k in ("winner", "claimed", "rule", "vetoed", "refusedVersion", "rival", "why"):
                self.detail[cause][f"{k}={v}"] += b
            elif k == "holders":
                self.detail[cause]["holder=" + (v[0] if v else "-")] += b
            elif k in ("maxNodes", "owners", "covered"):
                self.detail[cause][f"{k}:{bucket(v)}"] += b
        if ex is not None:
            self.examples[cause].append(ex)


def bucket(n):
    for lim in (10, 20, 30, 50, 100, 200, 500, 1000):
        if n < lim:
            return f"<{lim}"
    return ">=1000"


def entry_rows(entry, doc, ix, code, layers):
    """Every scored unit of one bun-dev bundle: (unit, label row, claim, verdict, cause, detail, mates)."""
    chm, om, o7m, cm = layers
    report = doc["libraryDiscovery"]
    units = score.run_units(report)
    truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
    origins = truth_audit.origin_layer(entry, om, o7m)
    truth, origins = truth_audit.redecode(entry, truth, origins)
    audit = truth_audit.Audit(entry, truth, cm)
    labels = score.label_units(truth, units, origins, audit)
    app_pkg = truth["appPackage"]
    every = score.scored_units(units)
    key = {(u["s"], u["e"]): u["i"] for u in every}
    claims = {key[(c["start"], c["end"])]: c for c in report.get("claims", []) if (c["start"], c["end"]) in key}
    vetoed = {key[k]: p for k, p in score.spans(report.get("vetoed", [])).items() if k in key}
    q = [u["i"] for u in units if u["k"] in ("Statement", "LazyBody", "FactoryBody") and not u["rt"] and "w" in u]
    q_index = {i: k for k, i in enumerate(q)}
    qs, qst = queue_spans(report)
    # module -> right-claimed families (for the mates attribute)
    mod_claims = collections.defaultdict(list)
    for i, c in claims.items():
        m = units[i]["m"] if units[i]["k"] == "FactoryBody" else None
        if m is not None:
            mod_claims[m].append(c["package"])
    placed = placed_packages(doc, units)
    out = []
    for u in every:
        t = labels[u["i"]]
        c = claims.get(u["i"])
        lab = t["label"]
        if PLANT and c is None and lab in ("<app>", "<self>") and lab not in planted:
            # --plant: a fabricated claim on the first <app> and <self> unit, to prove the reverse counters fire
            planted.add(lab)
            c = {"package": "lodash", "rule": "planted", "start": u["s"], "end": u["e"]}
        verdict = score.judge_origin(c["package"], t, app_pkg)[0] if c else None
        cause, det, mates = None, {}, None
        if "pkg" in t and verdict not in ("right", "family", "origin"):
            queued = in_spans(qs, qst, u["s"])
            cause, det = classify(ix, u, t, c if verdict == "wrongFamily" else None, vetoed, claims, q_index, q,
                                  queued)
            if c is not None and verdict != "wrongFamily":
                cause, det = "wrong-package", {"claimed": c["package"], "rule": c["rule"]}
            m = u["m"] if u["k"] == "FactoryBody" else None
            if m is None:
                mates = "statement"
            elif any(right_family(p, t) for p in mod_claims.get(m, ())):
                mates = "module-claimed-right"
            elif mod_claims.get(m):
                mates = "module-claimed-other"
            else:
                mates = "module-unclaimed"
        m = u["m"] if u["k"] == "FactoryBody" else u["i"] if u["k"] == "Factory" else None
        pv = None
        if m in placed:
            pv = score.judge_origin(placed[m], t, app_pkg)[0]
        out.append((u, t, c, verdict, cause, det, mates, lab, pv))
    return out


def vendor_package(path):
    """vendor/<pkg>/... or vendor/@scope/name/... -> the package; None for vendor/lib_<hash>.js."""
    parts = path.split("/")
    if len(parts) < 3 or parts[0] != "vendor":
        return None
    return "/".join(parts[1:3]) if parts[1].startswith("@") else parts[1]


def placed_packages(doc, units):
    """factory unit index -> the package names mode PLACES it under (bundleOrder's indexPath: a claim fold or the
    whole-module key). bundleOrder rows are in factory order."""
    facs = [u["i"] for u in units if u["k"] == "Factory"]
    bo = doc.get("bundleOrder") or []
    if len(bo) != len(facs):
        raise SystemExit(f"bundleOrder {len(bo)} rows vs {len(facs)} factories")
    out = {}
    for f, r in zip(facs, bo):
        p = r.get("indexPath")
        if p:
            vp = vendor_package(p)
            if vp:
                out[f] = vp
    return out


def simulate(ix, rows, app_pkg, vetoed_rows, sim):
    """First-order guard relaxations over every UNCLAIMED scored unit with a print (any label): what each relaxed
    guard would claim, judged by the truth (no propagation, no veto after it). Also the isolation veto's own drops
    judged (what the veto protected vs what it cost)."""
    r0 = dict(mmlib.RULES)
    vetoed_units = {u["i"] for u, _, _ in vetoed_rows}
    variants = {"multi-package": dict(r0, max_packages=10 ** 6), "specificity": dict(r0, spec=None),
                "lookup-floor-20": dict(r0, min_nodes=ROW_FLOOR),
                "lookup-floors-off": dict(r0, min_nodes=ROW_FLOOR, min_content=0)}
    for u, t, c, verdict, cause, det, mates, lab, pv in rows:
        if c is not None or "w" not in u or u["k"] == "Factory":
            continue
        base = mmlib.structural(ix, u, r0)
        for name, rules in variants.items():
            h = mmlib.structural(ix, u, rules)
            if h and not base:
                sim[name][score.judge_origin(h[0], t, app_pkg)[0]] += u["b"]
        if base:
            v = score.judge_origin(base[0], t, app_pkg)[0]
            sim["base-pick-unclaimed"][v] += u["b"]
            if u["i"] in vetoed_units:
                why = "vetoed"
            elif base[1] < r0["strong"]:
                why = "under-strong"
            elif multi_family(best_families(ix, u)):
                why = "tie-multi-family"
            else:
                why = "single-family"
            sim["base-pick-unclaimed:" + why][v] += u["b"]
    for u, t, p in vetoed_rows:
        sim["veto-drops"][score.judge_origin(p, t, app_pkg)[0]] += u["b"]


def run_bundev(reports_dir, out_dir, selftest=False):
    manifest = lib096.load_manifest()
    chm = truth_audit.load_chained_manifest()
    om = lib096.load_origins_manifest()
    o7m = truth_audit.load_origins7_manifest()
    cm = truth_audit.load_copies_manifest()
    layers = (chm, om, o7m, cm)
    indexes = {}
    vocab = collections.Counter()
    tot = collections.Counter()
    by_set = collections.defaultdict(collections.Counter)
    tally = Tally()
    reverse = collections.defaultdict(collections.Counter)
    reverse_ex = collections.defaultdict(list)
    pkg_apps = collections.defaultdict(set)
    failed = []
    simul = collections.defaultdict(collections.Counter)
    placed_t = collections.defaultdict(collections.Counter)
    os.makedirs(out_dir, exist_ok=True)
    for entry in manifest["entries"]:
        rp = os.path.join(reports_dir, entry["id"] + ".json")
        if not os.path.exists(rp):
            failed.append(entry["id"])
            continue
        doc = json.load(open(rp))
        if doc["libraryDiscovery"] is None:
            failed.append(entry["id"] + " (no discovery)")
            continue
        if doc["libraryDiscovery"]["indexId"] != INDEX_ID:
            raise SystemExit(f"{entry['id']}: report ran on another index")
        slice_ = doc["toolchain"]["slice"]
        if slice_ not in indexes:
            indexes[slice_] = score.load_index(INDEX_DIR, slice_, INDEX_ID)
        ix = indexes[slice_]
        code = open(lib096.corpus_path(entry, "bundle"), "rb").read()
        rows = entry_rows(entry, doc, ix, code, layers)
        span_row = {(r[0]["s"], r[0]["e"]): (r[0], r[1]) for r in rows}
        vrows = [(span_row[(v["start"], v["end"])][0], span_row[(v["start"], v["end"])][1], v["package"])
                 for v in doc["libraryDiscovery"].get("vetoed", []) if (v["start"], v["end"]) in span_row]
        simulate(ix, rows, entry.get("appPackage"), vrows, simul)
        app = entry["id"].split("-", 3)[3]
        for u, t, c, verdict, cause, det, mates, lab, pv in rows:
            b = u["b"]
            vocab[lab if lab.startswith("<") else "<package>"] += b
            if "pkg" in t:
                pkg_apps[t["pkg"]].add(app)
                tot["library"] += b
                by_set[entry["set"]]["library"] += b
                if verdict in ("right", "family", "origin"):
                    tot["named"] += b
                    by_set[entry["set"]]["named"] += b
                else:
                    tot["missed"] += b
                    by_set[entry["set"]]["missed"] += b
                    ex = None
                    if len(tally.examples[cause]) < 400:
                        ex = {"bundle": entry["id"], "s": u["s"], "e": u["e"], "b": b, "pkg": t["pkg"],
                              "version": t.get("version"), "file": t.get("file"), "kind": u["k"],
                              "detail": det, "mates": mates, "code": snippet(code, u["s"], u["e"])}
                    tally.add(cause, b, t["pkg"], mates, det, ex, entry["id"])
                    placed_t[cause][pv or "not-placed"] += b
                    if cause == "wrong-package":
                        tot["wrongFamily"] += b
            else:
                tot[lab] += b
                if pv is not None and c is None:
                    placed_t["non-library:" + lab][pv] += b
                if c is not None:
                    rv = score.judge_origin(c["package"], t, entry.get("appPackage"))[0]
                    reverse[lab + ":" + rv][(c["package"], c["rule"], app)] += b
                    if len(reverse_ex[lab + ":" + rv]) < 200:
                        reverse_ex[lab + ":" + rv].append({"bundle": entry["id"], "b": b, "claimed": c["package"],
                                                "rule": c["rule"], "kind": u["k"],
                                                "file": t.get("carriedFrom") or None,
                                                "code": snippet(code, u["s"], u["e"])})
        if selftest:
            break
        print(f"{entry['id']:70s} done", flush=True)
    res = {
        "corpus": CORPUS, "indexId": INDEX_ID, "failed": failed,
        "labelVocabularyBytes": dict(vocab), "totals": dict(tot), "bySet": {k: dict(v) for k, v in by_set.items()},
        "causes": {k: {"bytes": tally.bytes[k], "units": tally.units[k],
                       "packages": tally.pkg[k].most_common(),
                       "mates": dict(tally.mates[k]),
                       "detail": tally.detail[k].most_common(30),
                       "topBundles": tally.bundles[k].most_common(10)}
                   for k in sorted(tally.bytes, key=lambda k: -tally.bytes[k])},
        "reverse": {lab: sorted(([p, r, a, b] for (p, r, a), b in v.items()), key=lambda x: -x[3])
                    for lab, v in reverse.items()},
        "pkgApps": {p: len(a) for p, a in pkg_apps.items()},
        "simulation": {k: dict(v) for k, v in simul.items()},
        "placementOfMissed": {k: dict(v) for k, v in placed_t.items()},
    }
    json.dump(res, open(os.path.join(out_dir, "census.json"), "w"), indent=1, sort_keys=True)
    json.dump({"examples": tally.examples, "reverse": reverse_ex}, open(os.path.join(out_dir, "examples.json"), "w"),
              indent=1)
    return res


def run_cc(report_path, out_dir):
    """Claude Code: the labelled sample (truth rows), claims by span containment, the stratified estimate (exactly
    score.score_sample's reading), one cause per missed library row."""
    manifest = lib096.load_manifest()
    entry = manifest["entries"][0]
    truth = lib096.load_truth(entry)
    doc = json.load(open(report_path))
    report = doc["libraryDiscovery"]
    ix = score.load_index(INDEX_DIR, doc["toolchain"]["slice"], INDEX_ID)
    units = score.run_units(report)
    st = score.statement_units(units)
    by_s = {u["s"]: u for u in st}
    strata = truth["strata"]
    sb = collections.Counter()
    for t in truth["rows"]:
        sb[t["stratum"]] += t["b"]

    def est(t):
        return strata[t["stratum"]]["bytes"] * t["b"] / sb[t["stratum"]]
    cl = sorted((c["start"], c["end"], c["package"], c["rule"]) for c in report.get("claims", []))
    starts = [c[0] for c in cl]

    def claim_at(s):
        k = bisect.bisect_right(starts, s) - 1
        return cl[k] if k >= 0 and cl[k][0] <= s < cl[k][1] else None
    vet = {v["start"]: v["package"] for v in report.get("vetoed", [])}
    claims_by_i = {}
    key = {u["s"]: u["i"] for u in st}
    for c in report.get("claims", []):
        if c["start"] in key:
            claims_by_i[key[c["start"]]] = {"package": c["package"], "rule": c["rule"]}
    q = [u["i"] for u in st if "w" in u]
    q_index = {i: k for k, i in enumerate(q)}
    qs, qst = queue_spans(report)
    if PLANT:
        first_app = next(t for t in truth["rows"] if t["label"] == "<app>")
        cl.append((first_app["s"], first_app["s"] + 1, "lodash", "planted"))
        cl.sort()
        starts[:] = [c[0] for c in cl]
    vocab = collections.Counter()
    tot = collections.Counter()
    units_n = collections.Counter()
    tally = Tally()
    reverse = collections.Counter()
    for t in truth["rows"]:
        vocab[t["label"] if t["label"].startswith("<") else ("library:*" if "pkg" in t else t["label"])] += 1
        c = claim_at(t["s"])
        p = c[2] if c else None
        w = est(t)
        if "pkg" not in t:
            tot[t["label"]] += w
            units_n[t["label"]] += 1
            if p is not None:
                reverse[(t["label"], p, c[3])] += 1
            continue
        tot["library"] += w
        units_n["library"] += 1
        if p is not None and same(p, t["pkg"]):
            tot["named"] += w
            units_n["named"] += 1
            continue
        tot["missed"] += w
        units_n["missed"] += 1
        u = by_s.get(t["s"])
        if p is not None:
            cause, det = "wrong-package", {"claimed": p, "rule": c[3]}
        elif u is None or "w" not in u:
            cause, det = "unit-model", {}
        else:
            vetoed = {u["i"]: vet[u["s"]]} if u["s"] in vet else {}
            cause, det = classify(ix, u, t, None, vetoed, claims_by_i, q_index, q, in_spans(qs, qst, u["s"]))
        # count-only example: no code, no identifiers (CC content stays out of the repo)
        tally.add(cause, w, t["pkg"], "statement", det, {"stratum": t["stratum"], "b": t["b"], "pkg": t["pkg"]},
                  "cc-2.1.216")
    res = {
        "corpus": "cc", "indexId": INDEX_ID, "estimate": "stratified ratio estimate over the 620-unit sample",
        "labelVocabularyUnits": dict(vocab), "totals": {k: round(v) for k, v in tot.items()},
        "sampleUnits": dict(units_n),
        "causes": {k: {"bytes": round(tally.bytes[k]), "units": tally.units[k],
                       "topPackages": [[p, round(b)] for p, b in tally.pkg[k].most_common(25)],
                       "detail": [[d, round(b)] for d, b in tally.detail[k].most_common(30)]}
                   for k in sorted(tally.bytes, key=lambda k: -tally.bytes[k])},
        "reverseSampleUnits": [[a, p, r, n] for (a, p, r), n in reverse.most_common()],
    }
    os.makedirs(out_dir, exist_ok=True)
    json.dump(res, open(os.path.join(out_dir, "census-cc.json"), "w"), indent=1, sort_keys=True)
    return res


def main():
    if CORPUS == "cc":
        r = run_cc(sys.argv[2], sys.argv[3])
    else:
        r = run_bundev(sys.argv[2], sys.argv[3], "--selftest" in sys.argv or PLANT)
    print(json.dumps({k: r[k] for k in ("totals",)}, indent=1))
    for k, v in r["causes"].items():
        print(f"{k:28s} {v['bytes']:>12,} B {v['units']:>8} units")


if __name__ == "__main__":
    main()
