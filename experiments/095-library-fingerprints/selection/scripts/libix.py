"""The library-index LOOKUP for the selection and density labs (docs/design/library-index.md §8, not built in the
pipeline yet), over humanify-index's own data: an index directory, or the shard store directly.

The design rule, as the scale lane re-derived it at top-2,000 (scale/README.md §4–§6):
  WM  the query module's content key (humanify-core `vendor_content_key`, the key the index's module rows hold) is
      held by owners of exactly ONE family; the query carries a string (a stringless core agrees on strings
      trivially); the query module is >= 60 bytes (the lab's normLen 400); helper packages are not in the index;
      Stainless / es-shims template files are stop-listed.
  L   literal containment from npm SOURCE: package-level IDF (w = log2(N/df)), identifier-like S:/P: literals capped
      at 2 bits, a literal no indexed package has weighs log2(N)+1; only SPECIFIC files compete (a file may claim
      only if it holds <= 3x the query's literals + 20); per package its best file; claim when the query has > 5
      literals, best >= 20 bits, best >= 0.9 x the query's total weight, and best - runner-up (another package)
      >= 10 bits.
  S∧L the design: both claim, and their packages are in one family. The name is L's package if WM's owners hold
      it, else WM's owner.
Families (crates/humanify-core/src/libindex/families.json): the five named families, same npm scope, the scoped forks.
"""
import json, math, os, re, collections
import numpy as np
import scipy.sparse as sp

DATA = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../../crates/humanify-index/data")
FAM = json.load(open(os.path.join(DATA, "../../humanify-core/src/libindex/families.json")))
STOPL = json.load(open(os.path.join(DATA, "stoplists.json")))
STAIN_FILE = re.compile(STOPL["lookupFilePatterns"]["stainless"]["file"])
SHIM_FILE = re.compile(STOPL["lookupFilePatterns"]["shim"]["file"])
WORDY = re.compile(r"^[SP]:[A-Za-z_$][A-Za-z0-9_$-]*$")
CAP, RHO, T, COVER, MARGIN, MINLITS, MINBYTES = 2.0, 3.0, 20.0, 0.9, 10.0, 6, 60
_FORK = {}
for a, b in FAM["rules"]["scopedForks"]:
    _FORK[a] = _FORK[b] = "fork:" + a


def is_helper(p):
    return p in STOPL["helperPackages"] or any(p.startswith(x) for x in STOPL["helperPackagePrefixes"])


def family(p):
    if p is None:
        return None
    for f in FAM["families"]:
        for m in f["members"]:
            if (m.endswith("*") and p.startswith(m[:-1])) or p == m:
                return "fam:" + f["name"]
    if p in _FORK:
        return _FORK[p]
    if p.startswith("@"):
        return "scope:" + p.split("/")[0]
    return "base:" + p


def nstr(lits):
    return sum(1 for l in lits if l.startswith("S:") and l not in ("S:", "S:u"))


class Index:
    """modules: iterable of (key, owner, files); literals: iterable of (owner, file, lits)."""

    def __init__(self, modules, literals):
        lit_rows = [(o, f, tuple(l)) for o, f, l in literals if not is_helper(o)]
        stain = {o for o, f, l in lit_rows if any(x.lower() in ("s:x-stainless-lang", "p:x-stainless-lang") for x in l)}
        files_of = collections.defaultdict(set)
        for o, f, _ in lit_rows:
            files_of[o].add(f.rsplit("/", 1)[-1])
        shim = {o for o, fs in files_of.items() if "polyfill.js" in fs and "shim.js" in fs}

        def stopped(o, f):
            return (o in stain and STAIN_FILE.search(f)) or (o in shim and SHIM_FILE.search(f))

        self.stopped_files = 0
        seen, rows = set(), []
        for o, f, l in lit_rows:
            if stopped(o, f):
                self.stopped_files += 1
                continue
            k = (o, l)
            if k in seen or not l:
                continue
            seen.add(k)
            rows.append((o, l))
        self.by_key = collections.defaultdict(set)
        for key, o, files in modules:
            if is_helper(o) or not key:
                continue
            if files and all(stopped(o, f) for f in files):
                continue
            self.by_key[key].add(o)
        self.vocab = {}
        owners = sorted({o for o, _ in rows})
        self.owners = owners
        oid = {o: i for i, o in enumerate(owners)}
        cols = [np.array(sorted({self.vocab.setdefault(w, len(self.vocab)) for w in l}), np.int64) for _, l in rows]
        self.flen = np.array([len(c) for c in cols], np.int64)
        self.fown = np.array([oid[o] for o, _ in rows], np.int64)
        V = len(self.vocab)
        r = np.repeat(np.arange(len(rows)), self.flen)
        c = np.concatenate(cols) if cols else np.zeros(0, np.int64)
        self.F = sp.csr_matrix((np.ones(len(c), np.float32), (r, c)), shape=(len(rows), V))
        P = sp.csr_matrix((np.ones(len(c), np.float32), (self.fown[r], c)), shape=(len(owners), V))
        P.sum_duplicates()
        P.data[:] = 1
        self.df = np.asarray(P.sum(0)).ravel()
        self.N = max(len(owners), 1)
        wordy = np.zeros(V, bool)
        for w, i in self.vocab.items():
            if WORDY.match(w):
                wordy[i] = True
        with np.errstate(divide="ignore"):
            w = np.where(self.df > 0, np.log2(self.N / np.maximum(self.df, 1)), 0.0)
        self.w = np.where(wordy & (w > CAP), CAP, w).astype(np.float32)
        self.wunk = math.log2(self.N) + 1
        self.rows = len(rows)
        self.module_keys = len(self.by_key)

    # --- loaders -------------------------------------------------------------------------------------------
    @classmethod
    def from_dir(cls, d, slice="bun"):
        def mods():
            for l in open(os.path.join(d, f"modules.{slice}.jsonl")):
                r = json.loads(l)
                yield r["key"], r["owner"], r.get("files") or []

        def lits():
            for l in open(os.path.join(d, "literals.jsonl")):
                r = json.loads(l)
                yield r["owner"], r["file"], r["lits"]
        return cls(mods(), lits())

    @classmethod
    def from_shards(cls, store, pkgvers, slice="bun", owner_filter=None):
        """pkgvers: iterable of (package, version) whose shards (the ROOT's build, incl. its dependencies' rows)
        go in. owner_filter(owner, version) -> bool keeps only some rows (e.g. a density policy)."""
        mods, lits = [], []
        for p, v in pkgvers:
            fn = f"{p.replace('/', '__')}@{v}.json"
            for kind, out in ((slice, mods), ("literals", lits)):
                f = os.path.join(store, "shards", kind, fn)
                if not os.path.exists(f):
                    continue
                s = json.load(open(f))
                if kind == "literals":
                    for r in s["literals"]:
                        if owner_filter is None or owner_filter(r["owner"], r["version"]):
                            out.append((r["owner"], r["file"], r["lits"]))
                else:
                    for r in s["modules"]:
                        if owner_filter is None or owner_filter(r["owner"], r["version"]):
                            out.append((r["key"], r["owner"], [r["file"]]))
        return cls(mods, lits)

    # --- claims --------------------------------------------------------------------------------------------
    def wm(self, q):
        """q: dict with key, lits, start, end. -> set of owners, or None."""
        if not q.get("key") or q["end"] - q["start"] < MINBYTES or nstr(q.get("lits") or []) == 0:
            return None
        o = self.by_key.get(q["key"])
        if not o or len({family(x) for x in o}) != 1:
            return None
        return o

    def lit_claims(self, qs, chunk=256):
        """-> list of (package or None, score, total) per query, the L rule with specific files."""
        out = [(None, 0.0, 0.0)] * len(qs)
        V = len(self.vocab)
        qids, tots = [], []
        for q in qs:
            ids = [self.vocab.get(x, -1) for x in set(q.get("lits") or [])]
            known = np.array([i for i in ids if i >= 0], np.int64)
            unk = [x for x, i in zip(set(q.get("lits") or []), ids) if i < 0]
            tot = float(self.w[known].sum()) + sum(min(self.wunk, CAP) if WORDY.match(x) else self.wunk for x in unk)
            qids.append(known)
            tots.append(tot)
        n = np.array([len(set(q.get("lits") or [])) for q in qs])
        tots = np.array(tots)
        todo = np.nonzero((n >= MINLITS) & (tots >= T))[0]
        todo = todo[np.argsort(n[todo], kind="stable")]
        for a in range(0, len(todo), chunk):
            ix = todo[a:a + chunk]
            nmax = int(n[ix].max())
            fsel = np.nonzero(self.flen <= RHO * nmax + 20)[0]
            if not len(fsel):
                continue
            rr = np.repeat(np.arange(len(ix)), [len(qids[i]) for i in ix])
            cc = np.concatenate([qids[i] for i in ix]) if len(rr) else np.zeros(0, np.int64)
            Qm = sp.csr_matrix((self.w[cc], (rr, cc)), shape=(len(ix), V))
            S = (Qm @ self.F[fsel].T).tocoo()
            if not S.nnz:
                continue
            f = fsel[S.col]
            ok = self.flen[f] <= RHO * n[ix][S.row] + 20
            r_, f_, v_ = S.row[ok], f[ok], S.data[ok]
            o_ = self.fown[f_]
            order = np.lexsort((-v_, o_, r_))
            r_, o_, v_ = r_[order], o_[order], v_[order]
            first = np.ones(len(r_), bool)
            first[1:] = (r_[1:] != r_[:-1]) | (o_[1:] != o_[:-1])
            r_, o_, v_ = r_[first], o_[first], v_[first]
            order = np.lexsort((-v_, r_))
            r_, o_, v_ = r_[order], o_[order], v_[order]
            starts = np.nonzero(np.r_[True, r_[1:] != r_[:-1]])[0]
            for k, s0 in enumerate(starts):
                j = ix[r_[s0]]
                best, sc = self.owners[o_[s0]], float(v_[s0])
                s2 = float(v_[s0 + 1]) if s0 + 1 < len(r_) and r_[s0 + 1] == r_[s0] else 0.0
                if sc >= T and sc >= COVER * tots[j] and sc - s2 >= MARGIN:
                    out[j] = (best, sc, float(tots[j]))
                else:
                    out[j] = (None, sc, float(tots[j]))
        return out

    def claims(self, qs):
        """-> per query: dict(wm=owners|None, lit=pkg|None, name=design name|None)."""
        lc = self.lit_claims(qs)
        res = []
        for q, (lp, _, _) in zip(qs, lc):
            wo = self.wm(q)
            name = None
            if wo and lp and family(lp) == family(next(iter(wo))):
                name = lp if lp in wo else sorted(wo)[0]
            res.append({"wm": sorted(wo) if wo else None, "lit": lp, "name": name})
        return res

    def size(self):
        return {"moduleKeys": self.module_keys, "moduleRows": sum(len(v) for v in self.by_key.values()), "literalRows": self.rows,
                "owners": len(self.owners), "vocab": len(self.vocab)}


def verdict(truth, name):
    """right / family / wrong / none — truth None means app code (any name is wrong)."""
    if name is None:
        return "none"
    if truth is None:
        return "wrong"
    if name == truth:
        return "right"
    if family(name) == family(truth):
        return "family"
    return "wrong"
