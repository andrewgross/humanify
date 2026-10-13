"""Query sets and metrics shared by every detector.
Sets (per bundler family slice of the query):
  foreign   flab / neg / fresh library spans whose package is in NO index at the largest scale (either family):
            any name given is false. Fixed across scales.
  pos       library spans (lab, flab/neg/fresh dependencies, CC positives) whose package IS in the index at this
            scale; right = claimed package == truth (strict) or same family (fam).
  app       Claude Code app negatives: cc-negatives (100 groups) and cc-negative-functions (222).
"""
import collections, json
from lib import *

_Q = None
_IDX = {}


def Q():
    global _Q
    if _Q is None:
        _Q = queries()
    return _Q


def idx_pkgs(R, fam):
    k = (R, fam)
    if k not in _IDX:
        _IDX[k] = indexed_pkgs(R, fam)
    return _IDX[k]


_IDXV = {}


def idx_pkgvers(R, fam):
    k = (R, fam)
    if k not in _IDXV:
        _IDXV[k] = {(s["pkg"], s["ver"]) for s in index_rows(fam) if s["pos"] is not None and s["pos"] <= R}
    return _IDXV[k]


def all_max():
    return idx_pkgs(max(SCALES), "bun") | idx_pkgs(max(SCALES), "esbuild")


def qfam(q):
    return q["fam"]


def classify_set(q, R):
    """'foreign' | 'pos' | 'app' | 'cc' | None"""
    c = q["corpus"]
    if c in ("cc-negatives", "cc-negative-functions"):
        return "app"
    if c == "cc216":
        return "cc"
    if c == "ccsrc":
        return "ccsrc"  # the negatives lane's hand-labelled src/ statements: evaluated separately (stepb.py)
    if c == "cc-positives":
        return "pos" if q["pkg"] in idx_pkgs(R, "bun") else None
    if not is_lib(q["pkg"]):
        return None
    if c in FOREIGN_CORPORA and q["pkg"] not in all_max():
        return "foreign"
    if q["pkg"] in idx_pkgs(R, qfam(q)):
        return "pos"
    return None


def metrics(claim_of, R, famq=("bun",), corpora=None):
    """claim_of(q) -> pkg or None.  Returns dict of counts."""
    m = collections.Counter()
    ex = collections.defaultdict(list)
    for q in Q():
        if q["fam"] not in famq:
            continue
        if corpora and q["corpus"] not in corpora:
            continue
        st = classify_set(q, R)
        if st is None or st in ("cc", "ccsrc"):
            continue
        c = claim_of(q)
        b = q["bytes"]
        if st == "foreign":
            m["F"] += 1; m["Fb"] += b
            if c:
                m["Fn"] += 1; m["Fnb"] += b
                if same(c, q["pkg"]):
                    m["Fcopy"] += 1
                ex["F"].append((q["corpus"], q["pkg"], q["file"], b, c))
        elif st == "pos":
            m["P"] += 1; m["Pb"] += b
            exact = (q["pkg"], q["ver"]) in idx_pkgvers(R, q["fam"])
            m["Pex" if exact else "Pnx"] += 1
            if c:
                if c == q["pkg"]:
                    m["Pright"] += 1; m["Prightb"] += b; m["Pright_ex" if exact else "Pright_nx"] += 1
                elif same(c, q["pkg"]):
                    m["Pfam"] += 1; m["Pfamb"] += b
                else:
                    m["Pwrong"] += 1
                    ex["Pwrong"].append((q["corpus"], q["pkg"], q["file"], b, c))
        elif st == "app":
            k = "A" + ("g" if q["corpus"] == "cc-negatives" else "f")
            m[k] += 1
            if c:
                m[k + "n"] += 1
                ex["A"].append((q["corpus"], q["idx"], b, c))
    return m, ex


def summary(m):
    return dict(
        foreign=m["F"], foreignNamed=m["Fn"], foreignRate=pct(m["Fn"], m["F"]), foreignBytesRate=pct(m["Fnb"], m["Fb"]), foreignFamilyCopies=m["Fcopy"],
        pos=m["P"], recall=pct(m["Pright"] + m["Pfam"], m["P"]), recallStrict=pct(m["Pright"], m["P"]), byteRecall=pct(m["Prightb"] + m["Pfamb"], m["Pb"]),
        recallExactVer=pct(m["Pright_ex"], m["Pex"]), recallOtherVer=pct(m["Pright_nx"], m["Pnx"]), wrongPkg=m["Pwrong"],
        appGroupsNamed=f'{m["Agn"]}/{m["Ag"]}', appFnsNamed=f'{m["Afn"]}/{m["Af"]}')
