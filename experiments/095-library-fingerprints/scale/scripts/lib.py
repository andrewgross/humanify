"""Shared loading for the scale analysis (run with .venv/bin/python, PYTHONHASHSEED=0)."""
import json, os, pickle, re, collections
import numpy as np

X = "/work/exp095-library-fingerprints/"
S = X + "scale/"
D = S + "data/"
SCALES = [125, 250, 500, 1000, 2000]
ROOTS = json.load(open(D + "roots.json"))
HELD = set(ROOTS["held"])
FOREIGN_CORPORA = ("flab", "neg", "fresh")
QUERY_KINDS = ("cjs", "lazy-esm")


def is_lib(p):
    return p is not None and not p.startswith("<")


_spans = None


def spans():
    """every span row; adds 'sid' (row in tasks/shas order)"""
    global _spans
    if _spans is None:
        sp = pickle.load(open(D + "spans.pkl", "rb"))
        shas = json.load(open(D + "shas.json"))
        sid = {s: i for i, s in enumerate(shas)}
        for s in sp:
            s["sid"] = sid[s["sha"]]
        _spans = sp
    return _spans


def index_rows(fam=None):
    """ix library spans that may enter the index: pure, not held-out. fam: 'bun' | 'esbuild' | None"""
    for s in spans():
        if s["corpus"] != "ix" or not is_lib(s["pkg"]) or s["pkg"] in HELD:
            continue
        if s["purity"] is not None and s["purity"] < 0.9:
            continue
        if fam and s["fam"] != fam:
            continue
        yield s


def indexed_pkgs(R, fam=None):
    return {s["pkg"] for s in index_rows(fam) if s["pos"] is not None and s["pos"] <= R}


def pkg_ids():
    """stable package id table over everything we ever name"""
    names = sorted({s["pkg"] for s in spans() if is_lib(s["pkg"])})
    return names, {n: i for i, n in enumerate(names)}


def queries():
    """Query spans: one per (corpus, sha) outside ix, lib or app, pure, vendor-shaped kinds (+ every CC unit)."""
    out, seen = [], set()
    for s in spans():
        c = s["corpus"]
        if c == "ix":
            continue
        if c.startswith("cc"):
            key = (c, s["idx"])
        else:
            if s["purity"] is not None and s["purity"] < 0.9:
                continue
            if s["kind"] not in QUERY_KINDS:
                continue
            if not is_lib(s["pkg"]):
                continue
            # one per module occurrence regardless of entry mode (foreign.mjs dedupe)
            key = (c, s["fam"], s["pkg"], s["ver"], s["file"], s["part"])
        if key in seen:
            continue
        seen.add(key)
        out.append(s)
    return out


def units():
    z = np.load(D + "units.npz")
    return {k: z[k] for k in z.files}


def fph():
    return pickle.load(open(D + "fph.pkl", "rb"))


def qlits():
    return pickle.load(open(D + "qlits.pkl", "rb"))


# ---------- package families (SYNTHESIS §4 list + data-driven additions, see families.py) ----------
FAMILY_RULES = [
    ("msal", re.compile(r"^@azure/msal-")),
    ("aws", re.compile(r"^@(smithy|aws-sdk)/")),
    ("isaacs-glob", re.compile(r"^(glob|minimatch|lru-cache|path-scurry)$")),
    ("lodash", re.compile(r"^(lodash|lodash-es|lodash\..*|es-toolkit)$")),
    ("uri", re.compile(r"^(uri-js|fast-uri)$")),
]


def load_family_extra():
    p = D + "families-extra.json"
    return json.load(open(p)) if os.path.exists(p) else {}


# FAMILY_MODE: "none" (exact package only) | "brief" (the five SYNTHESIS §4 families) |
# "full" (brief + data-driven: same npm scope; an unscoped package and its scoped fork of the same bare name;
#          packages that ship byte-equivalent modules, families-extra.json from families.py)
FAMILY_MODE = os.environ.get("FAMILY_MODE", "full")
_FX = None
_UF = {}


def _brief(p):
    for name, rx in FAMILY_RULES:
        if rx.match(p):
            return "fam:" + name
    return None


def family(p):
    global _FX
    if p is None:
        return None
    if p.startswith(("fam:", "scope:", "base:")):
        return p
    if FAMILY_MODE == "none":
        return p
    b = _brief(p)
    if b or FAMILY_MODE == "brief":
        return b or p
    if _FX is None:
        _FX = load_family_extra()
    if p in _FX:
        return _FX[p]
    if p.startswith("@"):
        return "scope:" + p.split("/")[0]
    return "base:" + p


def _base(p):
    return p.split("/")[-1] if p and not p.startswith(("fam:", "scope:", "base:")) else p


def same(a, b, fam=True):
    if a is None or b is None:
        return False
    if a == b:
        return True
    if not fam or FAMILY_MODE == "none":
        return False
    fa, fb = family(a), family(b)
    if fa == fb:
        return True
    if FAMILY_MODE == "full":
        # an unscoped package and a scoped fork of the same bare name (colors / @colors/colors)
        ua, ub = a.startswith("@"), b.startswith("@")
        if ua != ub and _base(a) == _base(b) and not a.startswith(("fam:", "scope:", "base:")) and not b.startswith(("fam:", "scope:", "base:")):
            return True
    return False


def pct(a, b):
    return f"{100.0 * a / b:.2f}%" if b else "-"
