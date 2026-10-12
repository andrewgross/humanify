"""Experiment B steps 2-3: index size and recall per version-density policy.

usage: <venv python> -I evaluate.py [--slice bun|esbuild] [--background <index dir>] [--tag <name>]
   -> $DEN/results/density-<slice><tag>.json

DATA. Every stable release of each density package was built by humanify-index (shards in $DEN/store); its builds
are the query bundles (queries.py: fpq, root-owned modules, exact owner + version from the sourcemap).

THE INDEX UNDER A POLICY. A policy keeps, per density package p, a set K_p of p's versions. The index then holds p's
module rows and literal rows whose version sets meet K_p (rows identical across versions are ONE row: humanify-index
stores versions as ranges). Rows of every other owner — the density packages' dependencies, and the optional
background index (a selection-lab index minus the density packages) — are held at all their versions; they are the
competition (IDF, margins, multi-owner keys) and do not vary with the policy (a small approximation, toward MORE
competition for p).

THE LOOKUP is libix's design rule (WM ∧ L, families, specificity, stop-lists). To evaluate many policies, each
distinct query module (p, key, literal set) is scored once against every row: p's rows keep their version sets
(those within 10 bits of a claim), other owners keep their best two. A policy then reads p's best score over rows
that meet K_p. IDF is computed once over the full index (owners, not versions: a policy changes p's literal
VOCABULARY only at the margins).

POLICIES (per package, over its stable versions in semver order):
  all             every release
  patch/N         every Nth patch within each major.minor line, counted back from the line's latest (N = 2, 5)
  minor           the latest patch of every minor
  major           the latest of every major (0.x: every minor, as humanify-index's as-of policy)
  latest          the latest release only
  asof-default    humanify-index's as-of policy today: latest, previous patch, major-1, major-2
  year-all+minor  every release of the last year (published >= 2025-10-11) + the latest patch of every minor
  minor-last2+major  the latest patch of every minor in the two newest major lines + the latest of every major
SCENARIOS (per query version X, X never indexed): nearest-patch (the closest other patch of X's minor, earlier
first), same-minor (all other patches of X's minor), same-major (every other version of X's major, X's minor
excluded), latest-before (the release published immediately before X), other-major (only versions of other
majors). And exact ({X} alone).

REPORTED per policy: index rows/bytes for the density packages (deduplicated, and summed per version = no dedup),
recall (bytes of p's modules named p or p's family) and wrong names (named another family), averaged per package
over versions uniformly and weighted by last-week downloads per version, then averaged over packages.
"""
import json, os, sys, re, collections, math, glob
import numpy as np
import scipy.sparse as sp

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../selection/scripts"))
import libix
from libix import family, is_helper, WORDY, CAP, RHO, T, COVER, MARGIN, MINLITS, MINBYTES

DEN = os.environ.get("DEN", "/work/exp095-library-fingerprints/density")
arg = lambda k, d=None: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d
SLICE = arg("--slice", "bun")
BG = arg("--background")
TAG = arg("--tag", "")


def sv(v):
    return tuple(int(x) for x in v.split("."))


def line(v):
    s = sv(v)
    return (0, s[1]) if s[0] == 0 else (s[0], 0)


RECENT_SINCE = "2025-10-11"  # one year before the as-of date (2026-10-11)


def policies(vers, pub=None):
    """vers: semver-sorted list of version strings -> {policy: set}."""
    out = {"all": set(vers), "latest": {vers[-1]}}
    by_minor = collections.defaultdict(list)
    by_line = collections.defaultdict(list)
    for v in vers:
        by_minor[sv(v)[:2]].append(v)
        by_line[line(v)].append(v)
    for N in (2, 5):
        out[f"patch/{N}"] = {v for vs in by_minor.values() for i, v in enumerate(vs) if (len(vs) - 1 - i) % N == 0}
    out["minor"] = {vs[-1] for vs in by_minor.values()}
    out["major"] = {vs[-1] for vs in by_line.values()}
    L = vers[-1]
    d = {L}
    same = [v for v in by_minor[sv(L)[:2]] if v != L]
    if same:
        d.add(same[-1])
    lines = sorted((l for l in by_line if l != line(L)), reverse=True)[:2]
    d |= {by_line[l][-1] for l in lines}
    out["asof-default"] = d
    # hybrids: dense where bundles are (recent, current lines), sparse in the past
    recent_lines = set(sorted(by_line, reverse=True)[:2])
    out["minor-last2+major"] = {vs[-1] for k, vs in by_minor.items() if line(vs[-1]) in recent_lines} | out["major"]
    if pub:
        out["year-all+minor"] = {v for v in vers if pub.get(v, "") >= RECENT_SINCE} | out["minor"]
    return out


def scenarios(vers, pub, X):
    s = sv(X)
    minor = [v for v in vers if sv(v)[:2] == s[:2] and v != X]
    out = {"exact": {X}}
    if minor:
        before = [v for v in minor if sv(v) < s]
        out["nearest-patch"] = {before[-1] if before else minor[0]}
        out["same-minor"] = set(minor)
    else:
        out["nearest-patch"] = set()
        out["same-minor"] = set()
    out["same-major"] = {v for v in vers if line(v) == line(X) and sv(v)[:2] != s[:2]}
    earlier = [v for v in vers if pub.get(v, "") < pub.get(X, "") and v != X]
    out["latest-before"] = {max(earlier, key=lambda v: pub[v])} if earlier else set()
    out["other-major"] = {v for v in vers if line(v) != line(X)}
    return out


def load_rows(versions):
    """-> mods: {(key): {owner: set(versions) | None}}, lits: {(owner, litset): set(versions) | None}, files."""
    B = set(versions)
    mods = collections.defaultdict(dict)
    lits = {}
    lfile = {}
    per_version = collections.Counter()
    for p, vs in versions.items():
        for v in vs:
            fn = f"{p.replace('/', '__')}@{v['version']}.json"
            for kind in (SLICE, "literals"):
                f = f"{DEN}/store/shards/{kind}/{fn}"
                if not os.path.exists(f):
                    continue
                s = json.load(open(f))
                if kind == "literals":
                    for r in s["literals"]:
                        o = r["owner"]
                        if is_helper(o) or not r["lits"]:
                            continue
                        k = (o, tuple(r["lits"]))
                        if o in B:
                            lits.setdefault(k, set()).add(r["version"])
                            if o == p and r["version"] == v["version"]:
                                per_version[("lit", p, v["version"])] += 1
                        else:
                            lits.setdefault(k, None)
                        lfile.setdefault(k, r["file"])
                else:
                    for r in s["modules"]:
                        o = r["owner"]
                        if is_helper(o):
                            continue
                        e = mods[r["key"]]
                        if o in B:
                            e.setdefault(o, set()).add(r["version"])
                            if o == p and r["version"] == v["version"]:
                                per_version[("mod", p, v["version"])] += 1
                        else:
                            e.setdefault(o, None)
    return mods, lits, lfile, per_version


def add_background(mods, lits, lfile, B):
    if not BG:
        return
    for l in open(os.path.join(BG, f"modules.{SLICE}.jsonl")):
        r = json.loads(l)
        if r["owner"] in B or is_helper(r["owner"]):
            continue
        mods[r["key"]].setdefault(r["owner"], None)
    for l in open(os.path.join(BG, "literals.jsonl")):
        r = json.loads(l)
        if r["owner"] in B or is_helper(r["owner"]) or not r["lits"]:
            continue
        k = (r["owner"], tuple(r["lits"]))
        lits.setdefault(k, None)
        lfile.setdefault(k, r["file"])


def stopped_rows(lits, lfile):
    stain = {o for (o, l) in lits if any(x.lower() in ("s:x-stainless-lang", "p:x-stainless-lang") for x in l)}
    files_of = collections.defaultdict(set)
    for k, f in lfile.items():
        files_of[k[0]].add(f.rsplit("/", 1)[-1])
    shim = {o for o, fs in files_of.items() if "polyfill.js" in fs and "shim.js" in fs}
    return {k for k, f in lfile.items() if (k[0] in stain and libix.STAIN_FILE.search(f)) or (k[0] in shim and libix.SHIM_FILE.search(f))}


def main():
    versions = json.load(open(DEN + "/versions.json"))
    B = set(versions)
    mods, lits, lfile, per_version = load_rows(versions)
    add_background(mods, lits, lfile, B)
    stop = stopped_rows(lits, lfile)
    rows = [k for k in lits if k not in stop]
    owners = sorted({o for o, _ in rows})
    oid = {o: i for i, o in enumerate(owners)}
    vocab = {}
    cols = [np.array(sorted({vocab.setdefault(w, len(vocab)) for w in l}), np.int64) for _, l in rows]
    flen = np.array([len(c) for c in cols])
    fown = np.array([oid[o] for o, _ in rows])
    V = len(vocab)
    rr = np.repeat(np.arange(len(rows)), flen)
    cc = np.concatenate(cols)
    F = sp.csr_matrix((np.ones(len(cc), np.float32), (rr, cc)), shape=(len(rows), V))
    P = sp.csr_matrix((np.ones(len(cc), np.float32), (fown[rr], cc)), shape=(len(owners), V))
    P.sum_duplicates()
    P.data[:] = 1
    df = np.asarray(P.sum(0)).ravel()
    N = len(owners)
    wordy = np.zeros(V, bool)
    for w_, i in vocab.items():
        wordy[i] = bool(WORDY.match(w_))
    w = np.log2(N / np.maximum(df, 1)).astype(np.float32)
    w = np.where(wordy & (w > CAP), CAP, w).astype(np.float32)
    wunk = math.log2(N) + 1
    print("rows", len(rows), "owners", N, "vocab", V, "module keys", len(mods), flush=True)

    # distinct queries: (p, key, lits) -> {version: bytes}
    uq = {}
    for p, vs in versions.items():
        for v in vs:
            f = f"{DEN}/q/{SLICE}/{p.replace('/', '__')}@{v['version']}.jsonl"
            if not os.path.exists(f):
                continue
            for l in open(f):
                r = json.loads(l)
                k = (p, r["key"], tuple(r["lits"]))
                e = uq.setdefault(k, {"bytes": r["end"] - r["start"], "at": {}})
                e["at"][v["version"]] = r["end"] - r["start"]
    keys = list(uq)
    print("distinct queries", len(keys), flush=True)
    rows_versions = [lits[k] for k in rows]

    def score(keys):
        """per query (p, key, lits): (p's rows within reach of a claim, with version sets; best two other owners)."""
        rec = [None] * len(keys)
        nq = np.array([len(k[2]) for k in keys])
        qcols, tots = [], np.zeros(len(keys))
        for j, (p, key, L) in enumerate(keys):
            ids = [vocab.get(x, -1) for x in L]
            kn = np.array([i for i in ids if i >= 0], np.int64)
            tots[j] = w[kn].sum() + sum(min(wunk, CAP) if WORDY.match(x) else wunk for x, i in zip(L, ids) if i < 0)
            qcols.append(kn)
        todo = np.nonzero((nq >= MINLITS) & (tots >= T))[0]
        todo = todo[np.argsort(nq[todo], kind="stable")]
        CH = 256
        for a in range(0, len(todo), CH):
            ix = todo[a:a + CH]
            fsel = np.nonzero(flen <= RHO * nq[ix].max() + 20)[0]
            r_ = np.repeat(np.arange(len(ix)), [len(qcols[i]) for i in ix])
            c_ = np.concatenate([qcols[i] for i in ix])
            Qm = sp.csr_matrix((w[c_], (r_, c_)), shape=(len(ix), V))
            S = (Qm @ F[fsel].T).tocoo()
            f = fsel[S.col]
            ok = (flen[f] <= RHO * nq[ix][S.row] + 20) & (S.data >= COVER * tots[ix][S.row] - MARGIN)
            by = collections.defaultdict(list)
            for qr, fr, val in zip(S.row[ok].tolist(), f[ok].tolist(), S.data[ok].tolist()):
                by[qr].append((fr, val))
            for qr, lst in by.items():
                j = ix[qr]
                p = keys[j][0]
                mine, other = [], {}
                for fr, val in lst:
                    o = rows[fr][0]
                    if o == p:
                        mine.append((val, rows_versions[fr]))
                    elif val > other.get(o, 0):
                        other[o] = val
                rec[j] = (mine, sorted(other.items(), key=lambda x: -x[1])[:2])
            if a // CH % 50 == 0:
                print("scored", a + len(ix), "/", len(todo), flush=True)
        return rec, nq, tots

    def make_claim(keys, qbytes, rec, nq, tots):
        def claim(j, K):
            p, key, L = keys[j]
            ln = None
            if rec[j] is not None and nq[j] >= MINLITS:
                mine, other = rec[j]
                pb = max([val for val, vs in mine if vs and vs & K] or [0.0])
                cand = sorted([(pb, p)] + [(val, o) for o, val in other], key=lambda x: -x[0])
                s1, w1 = cand[0]
                s2 = cand[1][0] if len(cand) > 1 else 0.0
                if s1 >= T and s1 >= COVER * tots[j] and s1 - s2 >= MARGIN and s1 > 0:
                    ln = w1
            holders = set()
            e = mods.get(key) or {}
            for o, vs in e.items():
                if o == p:
                    if vs & K:
                        holders.add(o)
                else:
                    holders.add(o)
            wm = None
            if key and qbytes[j] >= MINBYTES and libix.nstr(L) > 0 and holders and len({family(o) for o in holders}) == 1:
                wm = holders
            if wm and ln and family(ln) == family(next(iter(wm))):
                return ln if ln in wm else sorted(wm)[0]
            return None
        return claim

    rec, nq, tots = score(keys)
    claim = make_claim(keys, [uq[k]["bytes"] for k in keys], rec, nq, tots)

    pmods, plits = collections.defaultdict(list), collections.defaultdict(list)
    for key, e in mods.items():
        for o, vs in e.items():
            if vs:
                pmods[o].append(vs)
    for k, vs in lits.items():
        if vs:
            plits[k[0]].append((k, vs))
    by_pkg = collections.defaultdict(list)
    for j, k in enumerate(keys):
        by_pkg[k[0]].append(j)
    res = {"slice": SLICE, "background": BG, "rows": len(rows), "owners": N, "distinctQueries": len(keys), "packages": {}}
    pol_names = ["all", "patch/2", "patch/5", "minor", "year-all+minor", "minor-last2+major", "major", "asof-default", "latest"]
    sc_names = ["exact", "nearest-patch", "same-minor", "same-major", "latest-before", "other-major"]
    for p, vs in versions.items():
        vers = [v["version"] for v in vs]
        if not vers or p not in by_pkg:
            continue
        pub = {v["version"]: v["published"] for v in vs}
        dl = {v["version"]: v["downloads"] for v in vs}
        pols = policies(vers, pub)
        # per version: bytes, and per policy/scenario right/wrong bytes
        acc = {X: collections.Counter() for X in vers}
        for j in by_pkg[p]:
            at = uq[keys[j]]["at"]
            for nm, K in pols.items():
                name = claim(j, K)
                v_ = libix.verdict(p, name)
                for X, b in at.items():
                    acc[X][f"{nm}:{v_}"] += b
            for X, b in at.items():
                acc[X]["bytes"] += b
                for nm, K in scenarios(vers, pub, X).items():
                    if not K:
                        acc[X][f"sc:{nm}:unavailable"] += b
                        continue
                    acc[X][f"sc:{nm}:{libix.verdict(p, claim(j, K))}"] += b
        P = {"versions": len(vers), "queried": sum(1 for X in vers if acc[X]["bytes"]), "policies": {}, "scenarios": {}}
        for nm, K in pols.items():
            P["policies"][nm] = {"kept": len(K)}
        q_versions = [X for X in vers if acc[X]["bytes"]]
        tot_dl = sum(dl[X] for X in q_versions) or 1
        for nm in pol_names + ["sc:" + s for s in sc_names]:
            def rate(X, what):
                b = acc[X]["bytes"]
                if nm.startswith("sc:"):
                    avail = b - acc[X][f"{nm}:unavailable"]
                    if avail <= 0:
                        return None
                    b = avail
                good = acc[X][f"{nm}:right"] + acc[X][f"{nm}:family"] if what == "recall" else acc[X][f"{nm}:wrong"]
                return good / b
            vals = [(X, rate(X, "recall"), rate(X, "wrong")) for X in q_versions]
            vals = [x for x in vals if x[1] is not None]
            if not vals:
                continue
            d = {"recall": round(sum(x[1] for x in vals) / len(vals), 4), "wrong": round(sum(x[2] for x in vals) / len(vals), 5),
                 "recallDl": round(sum(x[1] * dl[x[0]] for x in vals) / sum(dl[x[0]] for x in vals), 4) if sum(dl[x[0]] for x in vals) else None,
                 "wrongDl": round(sum(x[2] * dl[x[0]] for x in vals) / sum(dl[x[0]] for x in vals), 5) if sum(dl[x[0]] for x in vals) else None,
                 "n": len(vals)}
            if nm.startswith("sc:"):
                P["scenarios"][nm[3:]] = d
            else:
                P["policies"][nm].update(d)
        # sizes for this package under each policy
        for nm, K in pols.items():
            mrows = sum(1 for vs in pmods[p] if vs & K)
            lrows = [(k, vs) for k, vs in plits[p] if vs & K]
            P["policies"][nm].update({
                "moduleRows": mrows, "literalRows": len(lrows),
                "literalBytes": sum(len(json.dumps({"owner": p, "file": lfile[k], "lits": list(k[1])})) for k, _ in lrows),
                "moduleRowsNoDedup": sum(per_version[("mod", p, v)] for v in K),
                "literalRowsNoDedup": sum(per_version[("lit", p, v)] for v in K)})
        res["packages"][p] = P
        print(p, {k: (v.get("kept"), v.get("recall"), v.get("wrong"), v.get("literalRows")) for k, v in P["policies"].items()}, flush=True)
    os.makedirs(DEN + "/results", exist_ok=True)
    json.dump(res, open(f"{DEN}/results/density-{SLICE}{TAG}.json", "w"), indent=1)
    if SLICE == "bun" and os.path.isdir(DEN + "/cc"):
        cc_eval(versions, score, make_claim, mods)


def cc_eval(versions, score, make_claim, mods):
    """Claude Code CJS factories at several CC versions (density/cc, fpq; no sourcemap). Pass 1 names each module
    with EVERY version of every density package indexed; a module named a density package p is then a query of p.
    p's version in that CC build, X: the declared one (an inlined package.json in the bundle, density/cc/
    declared.json) or else INFERRED — the version of p whose module rows hold the most of these modules' keys (ties:
    the latest such version). Recall under a policy is relative to all-versions recall (= 100% by construction)."""
    decl = json.load(open(DEN + "/cc/declared.json"))
    qs = []
    for f in sorted(glob.glob(DEN + "/cc/cc-*.jsonl")):
        ccv = re.match(r".*/cc-(.+)\.jsonl$", f).group(1)
        for l in open(f):
            r = json.loads(l)
            qs.append((ccv, r))
    keys1 = [("", r["key"], tuple(r["lits"] or ())) for _, r in qs]
    rec1, nq1, tot1 = score(keys1)
    claim1 = make_claim(keys1, [r["end"] - r["start"] for _, r in qs], rec1, nq1, tot1)
    allK = set()
    named = [claim1(j, allK) for j in range(len(keys1))]
    B = set(versions)
    keys2, qb, meta = [], [], []
    for (ccv, r), n in zip(qs, named):
        if n in B:
            keys2.append((n, r["key"], tuple(r["lits"] or ())))
            qb.append(r["end"] - r["start"])
            meta.append(ccv)
    rec2, nq2, tot2 = score(keys2)
    claim2 = make_claim(keys2, qb, rec2, nq2, tot2)
    out = {}
    groups = collections.defaultdict(list)
    for j, ccv in enumerate(meta):
        groups[(keys2[j][0], ccv)].append(j)
    for (p, ccv), js in sorted(groups.items()):
        vers = [v["version"] for v in versions[p]]
        pub = {v["version"]: v["published"] for v in versions[p]}
        dv = sorted({v for i, (n, v) in decl.get(ccv, {}).items() if n == p})
        if dv:
            X, how = dv[-1], "declared"
        else:
            hits = collections.Counter()
            for j in js:
                for v in (mods.get(keys2[j][1]) or {}).get(p) or ():
                    hits[v] += qb[j]
            if not hits:
                continue
            best = max(hits.values())
            X = max((v for v in hits if hits[v] == best), key=lambda v: tuple(int(x) for x in v.split(".")))
            how = "inferred"
        if X not in vers:
            continue
        tot = sum(qb[j] for j in js)
        row = {"p": p, "cc": ccv, "X": X, "how": how, "modules": len(js), "bytes": tot, "policies": {}, "scenarios": {}}
        for nm, K in list(policies(vers, pub).items()) + [("sc:" + k, v) for k, v in scenarios(vers, pub, X).items()]:
            if not K:
                continue
            c = collections.Counter()
            for j in js:
                c[libix.verdict(p, claim2(j, K))] += qb[j]
            d = {"recall": round((c["right"] + c["family"]) / tot, 4), "wrong": round(c["wrong"] / tot, 4), "kept": len(K)}
            (row["scenarios"] if nm.startswith("sc:") else row["policies"])[nm.removeprefix("sc:")] = d
        out[f"{p}@{ccv}"] = row
        print("cc", p, ccv, X, how, {k: v["recall"] for k, v in row["policies"].items()}, flush=True)
    json.dump({"modules": len(qs), "namedDensity": len(keys2), "rows": out}, open(f"{DEN}/results/density-cc{TAG}.json", "w"), indent=1)


if __name__ == "__main__":
    main()
