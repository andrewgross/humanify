#!/usr/bin/env python3
"""exp180 step 3: size a per-package DETECTOR index and simulate package discovery with it, offline.

usage: <exp095 venv python> -I simulate.py <pass dir> <data dir> <out.json> [--plant]

<pass dir>: candidates.tsv in index_pass.py's format (prints held by at most 3 owners across the whole index, both
            slices) — written by pickle_pass.py for exp180's numbers.
<data dir>: extract.py's x-<corpus>.json.gz, versions.py's support-<corpus>.json.gz, the text discovery's
            seeds-<corpus>.jsonl and bundles-<corpus>.txt.

A detector = for every indexed owner, its K most distinctive prints under a RULE (below). A bundle FLAGS an owner
when at least `m` of that owner's detector prints appear among the bundle's prints (every unit row's whole and
function hashes); a hit on a print with several owners flags every one of them (they would all be fetched). Then,
per corpus (totals over bundles):

  detection recall   of the packages truly in the bundle (scorer-14 shipper labels), the share flagged — FAMILY (any
                     member of the package's family flagged) and STRICT (the package itself) — by library bytes
                     and by count; also over the INDEXED packages only (those a detector can ever flag)
  false positives    flagged owners with no unit of their family in the bundle (truth label, origin or via; not the
                     app's own package), and what fetching them costs (their indexed versions)
  end-to-end         today's claims restricted to claims whose package is flagged (STRICT: the exact owner; FAMILY:
                     any family member flagged), as a share of library bytes (the scorer's recall) and of today's
                     named bytes; wrong-family bytes kept (precision side)
  version pick       the same, keeping per flagged owner only the N versions its detector hits point at (most hits
                     first, newer on ties) — the claim stays when one of the versions supporting it is kept

The same detection and end-to-end numbers are computed for the text discovery (`--library-index-from-bundle`'s
seeds: banners and inlined package.json declarations), and for the union of the two.
"""
import collections
import gzip
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "../096-library-matching-score")))
import lib096  # noqa: E402

lib096.use("bun-dev")
import score  # noqa: E402

PASS, DATA, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
CORPORA = [c for c in ("bun-dev", "bun-held", "cc") if os.path.exists(os.path.join(DATA, f"x-{c}.json.gz"))]
KS = (1, 3, 5, 10, 20, 50, 100, 0)  # 0 = every eligible print (the upper reference)
MS = (1, 2)
VERSION_NS = (1, 2, 3, 5)
NAMED = ("right", "family", "origin")
NOT_LIBRARY = score.NOT_LIBRARY
NPM_NAME = re.compile(r"^(@[a-z0-9][a-z0-9._~-]*/)?[a-z0-9][a-z0-9._~-]*$")
INDEX = "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28"
RECENT = 10  # index_pass.py's RECENT
FLOOR_NODES, FLOOR_CONTENT = 30, 3  # the lookup's own floors (mmlib.RULES): a print the matcher would read
PLANT = "--plant" in sys.argv


# ---------------------------------------------------------------- the candidates

class C:
    __slots__ = ("h", "nodes", "content", "cov", "nver", "both", "file", "recent", "vmask", "owners")

    def __init__(self, f):
        self.h = f[1]
        self.nodes, self.content, self.cov = int(f[2]), int(f[3]), int(f[4])
        self.nver = max(int(f[5]), 1)
        self.both = f[6] == "3"
        self.file = f[9]
        self.recent = int(f[10]) / min(self.nver, RECENT)
        self.vmask = int(f[11], 16)
        self.owners = tuple(f[13].split(","))

    @property
    def unique(self):
        return len(self.owners) == 1


def load_candidates():
    by_owner = collections.defaultdict(list)
    with open(os.path.join(PASS, "candidates.tsv")) as fh:
        next(fh)
        for line in fh:
            f = line.rstrip("\n").split("\t")
            c = C(f)
            if c.nodes >= FLOOR_NODES and (c.content < 0 or c.content >= FLOOR_CONTENT):  # -1: provisional pass
                by_owner[f[0]].append(c)
    return by_owner


# ---------------------------------------------------------------- the rules (each returns an ordered print list)

def cov_key(c):
    return (-c.cov / c.nver, -c.both, -c.nodes, c.h)


def recent_key(c):
    return (-c.recent, -c.cov / c.nver, -c.both, -c.nodes, c.h)


def rule_coverage(cs, k):
    """COVERAGE first: the prints present in the most of the owner's indexed versions (so whichever version is
    bundled, the print is there), then in both Bun slices, then the largest (least likely to collide)."""
    return sorted((c for c in cs if c.unique), key=cov_key)[:k]


def rule_size(cs, k):
    """SIZE first: among prints in at least half the owner's versions (else all), the largest ones — the most
    unambiguous code, least likely to occur by chance in app code."""
    u = [c for c in cs if c.unique]
    wide = sorted((c for c in u if c.cov / c.nver >= 0.5), key=lambda c: (-c.nodes, c.h))
    narrow = sorted((c for c in u if c.cov / c.nver < 0.5), key=lambda c: (-c.nodes, c.h))
    return (wide + narrow)[:k]


def spread(order, k):
    seen, first, rest = set(), [], []
    for c in order:
        (rest if c.file in seen else first).append(c)
        seen.add(c.file)
    return (first + rest)[:k]


def rule_spread(cs, k):
    """SPREAD across files: the coverage order, but at most one print per source file until every file has given
    one — a tree-shaken bundle keeps only the files it uses, so K prints from one file are K chances on one bet."""
    return spread(sorted((c for c in cs if c.unique), key=cov_key), k)


def rule_recent(cs, k):
    """RECENT coverage first: present in the most of the owner's NEWEST indexed versions (RECENT = 10), then overall
    coverage, both slices, size. Coverage-first is dragged to old code when a package has many old versions indexed
    (typescript: 80 versions from 0.8.3; its widest prints sit in the OLD half)."""
    return sorted((c for c in cs if c.unique), key=recent_key)[:k]


def greedy_cover(pool, k):
    """Pick prints so that EVERY indexed version of the owner holds as many picked prints as possible: each step
    takes the print covering the most versions among those currently covered the fewest times (so the first picks
    cover every version once, the next ones twice, ...), ties to unique prints, both slices, larger. Whatever version
    is bundled, the detector holds prints of it."""
    if not pool:
        return []
    nver = pool[0].nver
    counts = [0] * nver
    picked, left = [], list(pool)
    while left and len(picked) < k:
        low = min(counts)
        want = sum(1 << i for i, n in enumerate(counts) if n == low)
        best = max(left, key=lambda c: ((c.vmask & want).bit_count(), c.unique, c.both, c.nodes, c.h))
        if not best.vmask & want:
            # no print left holds any least-covered version (it owns no eligible print): drop those from the minimum
            if not any(c.vmask for c in left):
                break
            for i in range(nver):
                if counts[i] == low:
                    counts[i] = 1 << 30
            continue
        picked.append(best)
        left.remove(best)
        for i in range(nver):
            if best.vmask >> i & 1:
                counts[i] += 1
    return picked


def greedy_file_cover(pool, k):
    """greedy_cover, but a version's count is the number of DISTINCT SOURCE FILES among the picked prints that hold
    it, so a pick from a file that already covers a version adds nothing for it. Whatever version is bundled and
    whichever of its files tree-shaking kept, the detector holds prints of it from as many files as K allows
    (zod 4.6.7: plain cover picked three prints of `dist/esm/v3/types.js`, the v3 compatibility layer present in
    3.x and 4.x alike, which no bundle using the v4 API contains)."""
    if not pool:
        return []
    nver = pool[0].nver
    counts = [0] * nver
    fmask = collections.defaultdict(int)  # file -> versions its picked prints already cover
    picked, left = [], list(pool)
    while left and len(picked) < k:
        low = min(counts)
        want = sum(1 << i for i, n in enumerate(counts) if n == low)
        best = max(left, key=lambda c: ((c.vmask & want & ~fmask[c.file]).bit_count(), c.unique, c.both, c.nodes,
                                        c.h))
        gain = best.vmask & want & ~fmask[best.file]
        if not gain:
            if not any(c.vmask & ~fmask[c.file] for c in left):
                break
            for i in range(nver):
                if counts[i] == low:
                    counts[i] = 1 << 30
            continue
        picked.append(best)
        left.remove(best)
        new = best.vmask & ~fmask[best.file]
        fmask[best.file] |= best.vmask
        for i in range(nver):
            if new >> i & 1:
                counts[i] += 1
    return picked


def rule_fcover(cs, k):
    """FILE-DIVERSE VERSION COVER over unique prints (greedy_file_cover)."""
    return greedy_file_cover([c for c in cs if c.unique], k)


def rule_fcover3(cs, k):
    """FILE-DIVERSE VERSION COVER over prints held by at most 3 owners (flagging only the picked owner)."""
    return greedy_file_cover(cs, k)


def rule_cover(cs, k):
    """VERSION COVER over unique prints (greedy_cover)."""
    return greedy_cover([c for c in cs if c.unique], k)


def rule_cover3(cs, k):
    """VERSION COVER over prints held by at most 3 owners, unique ones preferred on ties: a package every one of
    whose prints is also carried by another package (typescript 5.9 inside @ts-morph/common and monaco-editor) can
    still be detected, at the price of also fetching the carrier."""
    return greedy_cover(cs, k)


RULES = {"coverage": rule_coverage, "size": rule_size, "spread": rule_spread, "recent": rule_recent,
         "cover": rule_cover, "cover3": rule_cover3, "cover3p": rule_cover3, "fcover": rule_fcover,
         "fcover3p": rule_fcover3}
GREEDY = {"cover", "cover3", "cover3p", "fcover", "fcover3p"}
# cover3p: cover3's picks, but a hit flags only the owners the print was PICKED for, never its other holders (a
# carrier's copy of typescript flags typescript, whose rows then claim it as its origin; the carrier is not fetched)
PICKED_ONLY = {"cover3p", "fcover3p"}
GREEDY_KMAX = 100


def build_detectors(cands):
    """{(rule, K): {print: {owner: vmask or None}}} (None: the owner shares a print picked for another owner) and
    per-detector size."""
    out, sizes = {}, {}
    for rule, fn in RULES.items():
        top = {o: fn(cs, GREEDY_KMAX if rule in GREEDY else len(cs)) for o, cs in cands.items()}
        for k in KS:
            if k == 0 and rule not in ("coverage", "cover3", "cover3p"):
                continue  # every unique print / every few-owner print: the same detector whatever the order
            if k == 0:
                pool = {o: [c for c in cs if rule in ("cover3", "cover3p") or c.unique] for o, cs in cands.items()}
            else:
                pool = {o: cs[:k] for o, cs in top.items()}
            d = collections.defaultdict(dict)
            for o, cs in pool.items():
                for c in cs:
                    if rule not in PICKED_ONLY:
                        for owner in c.owners:
                            d[c.h].setdefault(owner, None)
                    d[c.h][o] = c.vmask
            out[(rule, k)] = dict(d)
            owners = {o for v in d.values() for o in v}
            sizes[(rule, k)] = {"entries": len(d), "owners": len(owners),
                                # binary estimate: 8-byte print + 4 bytes per owner id, plus the owner-name table
                                "bytesBinary": sum(8 + 4 * len(v) for v in d.values()) + sum(len(o) + 1 for o in owners)}
            print("detector", rule, k, sizes[(rule, k)], flush=True)
    return out, sizes


# ---------------------------------------------------------------- judging a flagged set against one bundle

_keys = {}


def keys(p):
    """A package's two family keys: score.same_family(a, b) is exactly `keys(a) & keys(b)` non-empty (the base
    family — families.json, scope, fork — or the derived-links root)."""
    if p not in _keys:
        _keys[p] = {("b", score.families_lib.base_family(p)), ("d", score.FAMILIES._find(p))}
    return _keys[p]


def fam_keys(packages):
    out = set()
    for p in packages:
        out |= keys(p)
    return out


class Bundle:
    def __init__(self, b, index_keys, support):
        self.id = b["id"]
        self.prints = set(b["prints"])
        self.app = b.get("appPackage")
        self.units = b["units"]
        self.support = support  # unit position -> int mask of the claimed owner's versions supporting the claim
        lib = collections.Counter()
        present = set()
        for bytes_, lab, _ver, origin, via, _c, _v, _p in self.units:
            if lab not in NOT_LIBRARY:
                lib[lab] += bytes_
                present.add(lab)
            for x in (origin, via):
                if x:
                    present.add(x)
        self.lib = lib  # truth package -> library bytes
        self.present = present  # every package the truth names anywhere in the bundle (the FP judge)
        self.indexed = {p for p in lib if keys(p) & index_keys}
        self.named = sum(u[0] for u in self.units if u[6] in NAMED)
        self.wrong = sum(u[0] for u in self.units if u[6] == "wrongFamily")


def judge(bundle, flagged, versions, picks=None):
    r = collections.Counter()
    fk = fam_keys(flagged)
    present = fam_keys(bundle.present)
    for p, b in bundle.lib.items():
        hit = bool(keys(p) & fk)
        r["libBytes"] += b
        r["libCount"] += 1
        r["libCount1k"] += b >= 1024
        r["indexedBytes"] += b * (p in bundle.indexed)
        r["indexedCount"] += p in bundle.indexed
        if p in flagged:
            r["hitStrictBytes"] += b
            r["hitStrictCount"] += 1
        if hit:
            r["hitBytes"] += b
            r["hitCount"] += 1
            r["hitCount1k"] += b >= 1024
            r["hitIndexedBytes"] += b * (p in bundle.indexed)
            r["hitIndexedCount"] += p in bundle.indexed
    for o in flagged:
        if bundle.app and keys(o) & keys(bundle.app):
            r["flagApp"] += 1
        elif keys(o) & present:
            r["flagTrue"] += 1
        else:
            r["flagFalse"] += 1
            r["flagFalseVersions"] += len(versions.get(o, ()))
    r["bundles"] += 1
    r["flagged"] += len(flagged)
    r["flaggedVersions"] += sum(len(versions.get(o, ())) for o in flagged)
    r["flaggedRows"] += sum(ROWS.get(o, 0) for o in flagged)
    for i, (bytes_, _lab, _ver, _o, _v, claim, verdict, _p) in enumerate(bundle.units):
        if claim is None:
            continue
        strict = claim in flagged
        family = strict or bool(keys(claim) & fk)
        if verdict in NAMED:
            r["keptStrict"] += bytes_ * strict
            r["keptFamily"] += bytes_ * family
            if picks is not None and strict:
                sup = bundle.support.get(i)
                for n in VERSION_NS:
                    if sup is None or sup & picks[claim][n]:  # a grown claim (no print support) stays with its owner
                        r[f"keptPick{n}"] += bytes_
        elif verdict == "wrongFamily":
            r["wrongStrict"] += bytes_ * strict
            r["wrongFamily"] += bytes_ * family
    r["named"] += bundle.named
    r["wrongToday"] += bundle.wrong
    return r


def detect(bundle, det, m, with_picks=False):
    hits = collections.defaultdict(list)
    for h in bundle.prints:
        v = det.get(h)
        if v:
            for o, mask in v.items():
                hits[o].append(mask)
    flagged = {o for o, ms in hits.items() if len(ms) >= m}
    if not with_picks:
        return flagged, None
    picks = {}
    for o in flagged:
        votes = collections.Counter()
        for mask in hits[o]:
            if mask is None:
                continue  # a hit on a print picked for ANOTHER owner says nothing of this owner's version
            i = 0
            while mask:
                if mask & 1:
                    votes[i] += 1
                mask >>= 1
                i += 1
        order = sorted(votes, key=lambda i: (-votes[i], -i))
        picks[o] = {n: sum(1 << i for i in order[:n]) for n in VERSION_NS}
    return flagged, picks


def text_seeds(path):
    """bundle path -> the seed names the from-bundle path would build from (valid npm names only, as
    runtime_index.rs filters them before building)."""
    out = {}
    for line in open(path):
        d = json.loads(line)
        out[d["bundle"]] = {s["name"] for s in d["seeds"] if NPM_NAME.match(s["name"])}
    return out


def summarize(r):
    def sh(a, b):
        return round(100.0 * a / b, 3) if b else None
    out = {
        "detectRecallBytes": sh(r["hitBytes"], r["libBytes"]),
        "detectRecallCount": sh(r["hitCount"], r["libCount"]),
        "detectRecallCount1k": sh(r["hitCount1k"], r["libCount1k"]),
        "detectRecallStrictBytes": sh(r["hitStrictBytes"], r["libBytes"]),
        "detectRecallStrictCount": sh(r["hitStrictCount"], r["libCount"]),
        "detectRecallIndexedBytes": sh(r["hitIndexedBytes"], r["indexedBytes"]),
        "detectRecallIndexedCount": sh(r["hitIndexedCount"], r["indexedCount"]),
        "flagged": r["flagged"], "flagTrue": r["flagTrue"], "flagApp": r["flagApp"], "flagFalse": r["flagFalse"],
        "flagFalseVersions": r["flagFalseVersions"], "flaggedVersions": r["flaggedVersions"],
        "flaggedPerBundle": round(r["flagged"] / r["bundles"], 1) if r["bundles"] else None,
        "targetedRowsPerBundle": round(r["flaggedRows"] / r["bundles"]) if r["bundles"] else None,
        "e2eRecallStrict": sh(r["keptStrict"], r["libBytes"]),
        "e2eRecallFamily": sh(r["keptFamily"], r["libBytes"]),
        "keptOfTodayStrict": sh(r["keptStrict"], r["named"]),
        "keptOfTodayFamily": sh(r["keptFamily"], r["named"]),
        "todayRecall": sh(r["named"], r["libBytes"]),
        "wrongBytesKeptStrict": r["wrongStrict"], "wrongBytesToday": r["wrongToday"],
        "libBytes": r["libBytes"], "libCount": r["libCount"], "indexedCount": r["indexedCount"],
        "namedBytesToday": r["named"], "keptBytesStrict": r["keptStrict"],
    }
    if any(f"keptPick{n}" in r for n in VERSION_NS):
        out["versionPick"] = {n: {"e2eRecall": sh(r[f"keptPick{n}"], r["libBytes"]),
                                  "keptOfToday": sh(r[f"keptPick{n}"], r["named"])} for n in VERSION_NS}
    return out


ROWS = {}
PICK_CONFIGS = {("cover", 5), ("cover", 20), ("cover3", 5), ("cover3", 20), ("recent", 20), ("cover", 100),
                ("cover3", 100), ("cover3p", 5), ("cover3p", 20), ("cover3p", 100), ("fcover3p", 5), ("fcover3p", 20),
                ("fcover3p", 100), ("fcover", 20)}


def misses(bundles, flagged_of, top=15):
    """The packages with the most library bytes the detector did not flag (family), summed over bundles."""
    lost, apps = collections.Counter(), collections.Counter()
    for b in bundles:
        fk = fam_keys(flagged_of(b))
        for p, n in b.lib.items():
            if not keys(p) & fk:
                lost[p] += n
                apps[p] += 1
    return [[p, n, apps[p]] for p, n in lost.most_common(top)]


def load_support(corpus):
    p = os.path.join(DATA, f"support-{corpus}.json.gz")
    if not os.path.exists(p):
        return {}
    return {bid: {int(i): int(m, 16) for i, m in d.items()} for bid, d in json.load(gzip.open(p, "rt")).items()}


def plant_check(bundles, versions):
    """The counters fire: a fabricated flag on an absent package counts one false positive, a flag on a present
    package one true, and flagging everything reproduces today's named bytes to the byte."""
    b = max(bundles, key=lambda b: b.named)
    absent = next(o for o in sorted(versions) if not keys(o) & fam_keys(b.present) and
                  not (b.app and keys(o) & keys(b.app)))
    one = max(b.lib, key=lambda p: b.lib[p])
    r = judge(b, {absent, one}, versions)
    assert r["flagFalse"] == 1 and r["flagTrue"] == 1, r
    allr = judge(b, set(versions) | {u[5] for u in b.units if u[5]}, versions)
    assert allr["keptStrict"] == b.named, (allr["keptStrict"], b.named)
    print("plant: counters fire on", b.id, flush=True)


def main():
    versions = json.load(open(os.path.join(INDEX, "versions.json")))
    index_keys = fam_keys(versions)
    op = os.path.join(PASS, "owners.json")
    if os.path.exists(op):
        ROWS.update({o: v["rows"] for o, v in json.load(open(op))["owners"].items()})
    cands = load_candidates()
    dets, sizes = build_detectors(cands)
    out = {"indexOwners": len(versions), "ownersWithEligible": len(cands),
           "ownersWithUniqueEligible": sum(1 for cs in cands.values() if any(c.unique for c in cs)),
           "totalRows": sum(ROWS.values()),
           "sizes": {f"{r}/K{k}": s for (r, k), s in sizes.items()}, "corpora": {}}
    for corpus in CORPORA:
        x = json.load(gzip.open(os.path.join(DATA, f"x-{corpus}.json.gz"), "rt"))
        paths = dict(line.split() for line in open(os.path.join(DATA, f"bundles-{corpus}.txt")))
        seeds = text_seeds(os.path.join(DATA, f"seeds-{corpus}.jsonl"))
        support = load_support(corpus)
        bundles = [Bundle(b, index_keys, support.get(b["id"], {})) for b in x["bundles"]]
        if PLANT and corpus == "bun-dev":
            plant_check(bundles, versions)
        res = {}
        for (rule, k), det in dets.items():
            for m in MS:
                if k and m > k:
                    continue
                with_picks = m == 1 and (rule, k) in PICK_CONFIGS and bool(support)
                tot = collections.Counter()
                for b in bundles:
                    flagged, picks = detect(b, det, m, with_picks)
                    tot.update(judge(b, flagged, versions, picks))
                res[f"{rule}/K{k}/m{m}"] = summarize(tot)
        owners = set(versions)
        for name, fl in (("text", lambda b: seeds[paths[b.id]] & owners),
                         ("text-raw", lambda b: seeds[paths[b.id]]),
                         ("everything", lambda b: owners | {u[5] for u in b.units if u[5]})):
            tot = collections.Counter()
            for b in bundles:
                tot.update(judge(b, fl(b), versions))
            res[name] = summarize(tot)
        for (rule, k) in (("cover", 20), ("cover3p", 5), ("cover3p", 20), ("fcover3p", 5), ("fcover3p", 20)):
            tot = collections.Counter()
            for b in bundles:
                tot.update(judge(b, detect(b, dets[(rule, k)], 1)[0] | (seeds[paths[b.id]] & owners), versions))
            res[f"text+{rule}/K{k}/m1"] = summarize(tot)
        out["corpora"][corpus] = {"label": x["label"], "bundles": len(bundles), "results": res}
        if corpus == "bun-dev":  # a dev corpus: package names may be listed (bun-held is private, cc is a sample)
            out["corpora"][corpus]["missedPackages"] = {
                f"{r}/K{k}": misses(bundles, lambda b, d=dets[(r, k)]: detect(b, d, 1)[0])
                for r, k in (("cover", 20), ("cover3p", 20), ("fcover", 20), ("fcover3p", 5), ("fcover3p", 20),
                                     ("fcover3p", 100), ("coverage", 0), ("cover3", 0))}
        print(corpus, "done", flush=True)
    json.dump(out, open(OUT, "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main()
