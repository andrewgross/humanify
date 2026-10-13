"""exp095 "misses": shared pieces — the real index's unit table, families, and an emulation of the in-run
structural rule (libindex::lookup::LibraryIndex::structural + claim_units) with every guard switchable, so a miss
can be traced to the guard that refused it.

The index rows are cached as a pickle next to the scratch data (MM env var, default the session scratchpad)."""
import collections
import json
import os
import pickle
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../selection/scripts"))
from libix import family  # noqa: E402,F401

IX = os.environ.get("IX", "/work/exp095-index-v2-data/final/ix/v2")
MM = os.environ.get("MM", "/tmp/claude-1000/-Users-andrewgross-Development-humanify/a3a6231d-5923-4c13-976d-1f7a4f545a0f/scratchpad/mm")


class Index:
    """units: hash -> [(owner, file, nodes, moduleBytes, version ranges)]; versions: owner -> [indexed versions]."""

    def __init__(self, slice_):
        cache = f"{MM}/ix-{slice_}.pickle"
        if os.path.exists(cache):
            self.__dict__.update(pickle.load(open(cache, "rb")))
            return
        self.versions = json.load(open(f"{IX}/versions.json"))
        units = collections.defaultdict(list)
        files = collections.defaultdict(set)  # (owner, file) -> versions holding any row
        for line in open(f"{IX}/units.{slice_}.jsonl"):
            r = json.loads(line)
            vr = tuple(tuple(x) for x in r["versions"])
            units[r["hash"]].append((r["owner"], r["file"], r["nodes"], r["moduleBytes"], vr))
            files[(r["owner"], r["file"])].update(self.expand(r["owner"], vr))
        self.units = dict(units)
        self.files = dict(files)
        pickle.dump({"versions": self.versions, "units": self.units, "files": self.files}, open(cache, "wb"), protocol=5)

    def expand(self, owner, ranges):
        vs = self.versions.get(owner, [])
        pos = {v: i for i, v in enumerate(vs)}
        out = set()
        for lo, hi in ranges:
            if lo in pos and hi in pos:
                out.update(vs[pos[lo]:pos[hi] + 1])
            else:
                out.update((lo, hi))
        return out

    def has_package(self, p):
        return p in self.versions

    def has_version(self, p, v):
        return v in self.versions.get(p, ())

    def file_versions(self, p, f):
        return self.files.get((p, f), set())

    def owners(self, h, module_bytes=None, spec=(4, 4096)):
        rows = self.units.get(h, ())
        if spec and module_bytes is not None:
            rows = [r for r in rows if r[3] <= spec[0] * module_bytes + spec[1]]
        return {r[0] for r in rows}


RULES = dict(min_nodes=30, min_content=3, min_function_content=0, cover=0.5, max_packages=3, support=10,
             strong=100, spec=(4, 4096))


def prints(u):
    """(whole, functions) as (start, end, hash, nodes, content) tuples."""
    w = u["w"]
    return (u["s"], u["e"], w[0], w[1], w[2]), [tuple(f) for f in u["f"]]


def eligible(u, rules=RULES):
    whole, fns = prints(u)
    ok_w = whole[3] >= rules["min_nodes"] and whole[4] >= rules["min_content"]
    ok_f = [f for f in fns if f[3] >= rules["min_nodes"] and f[4] >= rules["min_function_content"]]
    return whole, ok_w, ok_f


def greedy_cover(fns):
    """Largest non-overlapping functions first (the lookup's order): total nodes."""
    taken, tot = [], 0
    for f in sorted(fns, key=lambda f: -f[3]):
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        tot += f[3]
    return tot


def max_cover(u, rules=RULES):
    """Nodes the structural rule could cover if every eligible print hit; and the whole's nodes."""
    whole, ok_w, ok_f = eligible(u, rules)
    if ok_w:
        return whole[3], whole[3]
    return greedy_cover(ok_f), whole[3]


def cover_by(u, hit, rules=RULES):
    """Nodes covered when a print counts only if hit(print) — whole first, else functions (the rule's order)."""
    whole, ok_w, ok_f = eligible(u, rules)
    if ok_w and hit(whole):
        return whole[3]
    return greedy_cover([f for f in ok_f if hit(f)])


def structural(ix, u, rules=RULES, previous=None):
    """The rule's structural step on one unit: (package, covered) or None, plus the reason it refused."""
    whole, ok_w, ok_f = eligible(u, rules)
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
        return None
    best = max(score.values())
    tied = sorted(p for p, v in score.items() if v == best)
    p = previous if previous in tied else tied[0]
    if best < rules["cover"] * max(whole[3], 1):
        return None
    return p, best


def claim_all(ix, units, rules=RULES):
    """claim_units over statement units in order (structural + support; no agreement): {unit i: package}."""
    q = [u for u in units if u["k"] in ("Statement", "LazyBody") and not u["rt"] and "w" in u]
    prev = None
    st = []
    for u in q:
        h = structural(ix, u, rules, prev)
        if h:
            prev = h[0]
        st.append(h)
    out = {}
    for i, u in enumerate(q):
        h = st[i]
        if not h:
            continue
        p, cov = h
        if rules["support"] > 0 and cov < rules["strong"]:
            lo, hi = max(0, i - rules["support"]), min(len(q), i + rules["support"] + 1)
            if not any(j != i and st[j] and st[j][0] == p for j in range(lo, hi)):
                continue
        out[u["i"]] = p
    return out


def load_dump(path):
    return [json.loads(l) for l in open(path)]
