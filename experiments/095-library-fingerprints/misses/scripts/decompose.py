"""exp095 "misses": why library code the truth knows is not named — one primary cause per missed unit, in the
brief's precedence, by statement bytes.

usage: <venv python> -I decompose.py heldout|wrangler|cc   -> $MM/decomp/<set>.json (+ per-unit rows .jsonl)

Classes (first that applies):
  a  package not in the index at all
  b  package in, the bundle's version not, and no indexed version of the family has the unit (cover < half)
  c  the unit cannot be looked up: under the floors (30 nodes; whole statements 3 content tokens) — even if every
     eligible print hit, they would cover under half its nodes
  e  the hash IS in the index under the right family (cover >= half, no guards) but no right claim:
       e-spec (specificity), e-ambig (> 3 owners, or a tie the rule broke to another family), e-support (no
       same-package claim within 10 statements, under 100 nodes), e-agree (the literal rule refused / disagreed),
       e-veto (claimed, then dropped by discovery's isolation veto), e-other
  f  the unit's code is in the index under ANOTHER package (cover >= half by another family's rows) — a vendored or
     pre-bundled copy, a fork, or a mislabelled index row — or a claim named another family
  d  version indexed, the hash differs:
       d-file  the same file of that version has rows in the index (same source, different canonical form)
       d-variant  that version has rows for a file with the same name elsewhere in the package (dist/esm vs cjs …)
       d-absent  no row of that file at that version (the index's build never emitted it: entry, tree-shaking,
                 attribution)
       d-nofile  Claude Code (no map: file unknown)
"""
import collections
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mmlib  # noqa: E402
from mmlib import MM, family, load_dump  # noqa: E402

E = "/work/exp095-index-v2-data/eval"


def judge(claimed, truth):
    if claimed is None:
        return None
    if claimed == truth:
        return "right"
    return "family" if family(claimed) == family(truth) else "wrongFamily"


def claims_by_unit(report, units):
    """unit i -> claimed package (claims whose span starts at a unit start), and the set of vetoed units."""
    by = {u["s"]: u["i"] for u in units}
    r = json.load(open(report))
    out = {by[c["start"]]: c["package"] for c in r.get("claims", []) if c["start"] in by}
    vetoed = {by[v["start"]]: v["package"] for v in r.get("vetoed", []) if v["start"] in by}
    seeds = {by[c["start"]] for c in r.get("claims", []) if c["start"] in by and c["rule"] == "seed"}
    return out, vetoed, seeds


_BASE = {}


def basenames(ix):
    """(owner, file basename) -> versions with rows (cached)."""
    if id(ix) not in _BASE:
        d = collections.defaultdict(set)
        for (o, f), vs in ix.files.items():
            d[(o, f.rsplit("/", 1)[-1])].update(vs)
        _BASE[id(ix)] = d
    return _BASE[id(ix)]


def classify(ix, u, t, claim, vetoed, struct_claims, rule):
    """One missed library unit -> class string."""
    P, V, F = t["pkg"], t.get("version"), t.get("file")
    fam = family(P)
    whole, fns = mmlib.prints(u)
    wn = max(whole[3], 1)
    rows = ix.units

    def fam_hit(p):
        return any(family(r[0]) == fam for r in rows.get(p[2], ()))

    def other_hit(p):
        return any(family(r[0]) != fam for r in rows.get(p[2], ()))
    cov_any = mmlib.cover_by(u, fam_hit)
    if not ix.has_package(P):
        return "a"
    if not ix.has_version(P, V) and cov_any < 0.5 * wn:
        return "b"
    mc, _ = mmlib.max_cover(u)
    if mc < 0.5 * wn:
        return "c"
    if cov_any >= 0.5 * wn:
        if u["i"] in vetoed and family(vetoed[u["i"]]) == fam:
            return "e-veto"
        if rule == "agree" and u["i"] in struct_claims and family(struct_claims[u["i"]]) == fam:
            return "e-agree"
        if claim is not None and family(claim) != fam:
            return "f"
        # which guard refused it: relax one at a time
        r0 = dict(mmlib.RULES)
        h = mmlib.structural(ix, u, r0)
        if h and family(h[0]) == fam:
            return "e-support" if h[1] < r0["strong"] else ("e-agree" if rule == "agree" else "e-other")
        if h and family(h[0]) != fam:
            return "e-ambig"
        h = mmlib.structural(ix, u, dict(r0, spec=None))
        if h and family(h[0]) == fam:
            return "e-spec"
        h = mmlib.structural(ix, u, dict(r0, spec=None, max_packages=10 ** 6))
        if h and family(h[0]) == fam:
            return "e-ambig"
        return "e-other"
    if claim is not None and family(claim) != fam:
        return "f"
    if mmlib.cover_by(u, other_hit) >= 0.5 * wn:
        return "f"
    if F is None:
        return "d-nofile"
    if V in ix.file_versions(P, F):
        return "d-file"
    base = F.rsplit("/", 1)[-1]
    if V in basenames(ix).get((P, base), ()):
        return "d-variant"
    return "d-absent"


def run_bundle(ix, units, truth, reports):
    """Per rule: Counter of class -> bytes over library units, plus rows."""
    tl = {t["i"]: t for t in truth if t.get("i") is not None}
    cl = {rule: claims_by_unit(p, units) for rule, p in reports.items()}
    struct_claims = cl.get("struct", ({}, {}, set()))[0]
    out, rows = {}, []
    for rule in reports:
        claims, vetoed, _ = cl[rule]
        c = collections.Counter()
        for u in units:
            t = tl.get(u["i"])
            if not t or "pkg" not in t or "w" not in u:
                continue
            j = judge(claims.get(u["i"]), t["pkg"])
            if j in ("right", "family"):
                k = "named"
            else:
                k = classify(ix, u, t, claims.get(u["i"]), vetoed, struct_claims, rule)
            c[k] += u["b"]
            rows.append({"rule": rule, "i": u["i"], "s": u["s"], "b": u["b"], "pkg": t["pkg"], "v": t.get("version"),
                         "file": t.get("file"), "pos": t.get("pos"), "cls": k, "claim": claims.get(u["i"])})
        out[rule] = c
    return out, rows


def heldout():
    ix = mmlib.Index("bun")
    tot = {"struct": collections.Counter(), "agree": collections.Counter()}
    per_app, allrows = {}, []
    for slug, js, app in json.load(open(f"{E}/heldout-apps.json")):
        reps = {"struct": f"{E}/v2s/heldout/{slug}.struct.json", "agree": f"{E}/v2a/heldout/{slug}.agree.json"}
        if not all(os.path.exists(p) and "claims" in json.load(open(p)) for p in reps.values()):
            print("skip", slug)
            continue
        units = load_dump(f"{MM}/dump/{slug}.jsonl")
        truth = [json.loads(l) for l in open(f"{MM}/truth/{slug}.jsonl")]
        res, rows = run_bundle(ix, units, truth, reps)
        per_app[slug] = {r: dict(c) for r, c in res.items()}
        for r, c in res.items():
            tot[r].update(c)
        for x in rows:
            x["app"] = slug
        allrows += rows
    return tot, per_app, allrows


def wrangler():
    ix = mmlib.Index("esbuild")
    units = load_dump(f"{MM}/dump/wrangler.jsonl")
    truth = [json.loads(l) for l in open(f"{MM}/truth/wrangler.jsonl")]
    res, rows = run_bundle(ix, units, truth, {"struct": f"{E}/v2/wrangler.struct.json",
                                               "agree": f"{E}/v2/wrangler.agree.json"})
    return res, {}, rows


def cc():
    """Labelled sample: claims judged by span containment (index_score's unit mapping differs slightly); the
    stratified estimator turns sample bytes into src bytes."""
    ix = mmlib.Index("bun")
    units = load_dump(f"{MM}/dump/cc-2.1.216.jsonl")
    by_s = {u["s"]: u for u in units}
    truth = [json.loads(l) for l in open(f"{MM}/truth/cc-2.1.216.jsonl")]
    meta = json.load(open("/work/exp095-library-fingerprints/negatives/data/sample-meta.json"))
    sb = collections.Counter()
    for t in truth:
        sb[t["stratum"]] += t["b"]
    res, rows = {}, []
    struct_claims = None
    for rule in ("struct", "agree"):
        r = json.load(open(f"{E}/v2/cc-2.1.216-{rule}.json"))
        spans = sorted((c["start"], c["end"], c["package"]) for c in r["claims"])
        vet = {v["start"]: v["package"] for v in r["vetoed"]}
        starts = [s[0] for s in spans]
        import bisect

        def claim_at(s):
            k = bisect.bisect_right(starts, s) - 1
            return spans[k][2] if k >= 0 and spans[k][0] <= s < spans[k][1] else None
        claims = {t["s"]: claim_at(t["s"]) for t in truth}
        if rule == "struct":
            struct_claims = {by_s[s]["i"]: p for s, p in claims.items() if p and s in by_s}
        c, est = collections.Counter(), collections.Counter()
        for t in truth:
            if "pkg" not in t:
                continue
            u = by_s.get(t["s"])
            j = judge(claims[t["s"]], t["pkg"])
            if j in ("right", "family"):
                k = "named"
            elif u is None or "w" not in u:
                k = "unit-model"
            else:
                vetoed = {u["i"]: vet[u["s"]]} if u["s"] in vet else {}
                k = classify(ix, u, t, claims[t["s"]], vetoed, struct_claims, rule)
            c[k] += t["b"]
            est[k] += meta["strata"][t["stratum"]]["bytes"] * t["b"] / sb[t["stratum"]]
            rows.append({"rule": rule, "s": t["s"], "b": t["b"], "stratum": t["stratum"], "pkg": t["pkg"],
                         "v": t.get("version"), "cls": k, "claim": claims[t["s"]], "i": u["i"] if u else None})
        res[rule] = est
        res[rule + "/sampleBytes"] = c
    return res, {}, rows


def main():
    which = sys.argv[1]
    tot, per, rows = {"heldout": heldout, "wrangler": wrangler, "cc": cc}[which]()
    os.makedirs(f"{MM}/decomp", exist_ok=True)
    json.dump({"totals": {k: dict(v) for k, v in tot.items()}, "perApp": per}, open(f"{MM}/decomp/{which}.json", "w"),
              indent=1)
    with open(f"{MM}/decomp/{which}.rows.jsonl", "w") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
    for rule, c in tot.items():
        n = sum(c.values())
        print(which, rule, f"lib {n:,.0f}")
        for k, v in sorted(c.items(), key=lambda x: -x[1]):
            print(f"   {k:10s} {v:14,.0f} {v / max(n, 1):7.1%}")


if __name__ == "__main__":
    main()
