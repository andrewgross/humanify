"""Claim functions for the four evidence families, from the cached per-query scores.
Each returns {sha: pkg} for one configuration."""
import json, pickle, os
import numpy as np
from lib import *

NAMES = json.load(open(D + "pkgs.json")) if os.path.exists(D + "pkgs.json") else None


JNAMES = json.load(open(D + "pkgs-jidx.json")) if os.path.exists(D + "pkgs-jidx.json") else None  # id table jidx was built with


_JSTOP = {}


def _jstop(fam):
    """entries of the jidx that are stop-listed files (helpers, Stainless template, es-shims template)"""
    if fam not in _JSTOP:
        import re
        st = json.load(open(D + "stoplists.json"))
        stain, shim = set(st.get("stainless_pkgs", [])), set(st.get("shim_pkgs", []))
        HELPER = re.compile(r"^(tslib|@babel/runtime(-corejs\d)?|@babel/helpers|@swc/helpers|regenerator-runtime|@oxc-project/runtime)$")
        STAIN_FILE = re.compile(r"(^|/)(internal|core|_shims|_vendor)/|(^|/)(uploads|streaming|error|resource|core|api-promise|pagination)\.(m?js|cjs)$")
        SHIM_FILE = re.compile(r"(^|/)(implementation|polyfill|shim|auto)\.js$")
        keys = json.loads(str(np.load(D + f"jidx-{fam}.npz")["keys"]))
        _JSTOP[fam] = np.array([bool(HELPER.match(p) or (p in stain and f and STAIN_FILE.search(f)) or (p in shim and f and SHIM_FILE.search(f))) for p, v, f in keys])
    return _JSTOP[fam]


def jc_claims(fam, R, d=2, J=0.7, margin=0.1, gate=20, stop=True):
    z = np.load(D + f"jc-{fam}-R{R}-d{d}.npz")
    names = JNAMES
    ok = (z["J"] >= J) & (z["J"] - z["J2"] >= margin) & (z["qsize"] >= gate) & (z["Jpkg"] >= 0)
    if stop:
        ok &= ~_jstop(fam)[np.maximum(z["Jent"], 0)]
    return {s: names[p] for s, p, o in zip(z["sha"], z["Jpkg"], ok) if o}


_lt = {}


def lt_scores(R, sponge=False, nostop=False):
    k = (R, sponge, nostop)
    if k not in _lt:
        _lt.clear()
        _lt[k] = pickle.load(open(D + f"lt-{R}{'-sponge' if sponge else ''}{'-nostop' if nostop else ''}.pkl", "rb"))
    return _lt[k]


def lt_claims(R, T=20, C=0.9, M=10, minlits=6, sponge=False, family_margin=False, specific=False, nostop=False):
    """plain margin rule (near-ties refused; no smallest-file tie-break). family_margin: the runner-up is the best
    package of ANOTHER family (copies inside one family do not block)."""
    out = {}
    for s, (n, tot, top, topS) in lt_scores(R, sponge, nostop).items():
        if specific:
            top = topS
        if n < minlits or not top:
            continue
        p, sc, _ = top[0]
        if family_margin:
            s2 = max([x[1] for x in top[1:] if family(x[0]) != family(p)] or [0])
        else:
            s2 = top[1][1] if len(top) > 1 else 0
        if sc >= T and sc >= C * tot and sc - s2 >= M:
            out[s] = p
    return out


def uv_load(fam, R, nmin, cmin, fm, stop):
    return pickle.load(open(D + f"uv-votes-{fam}-R{R}-n{nmin}-c{cmin}-{int(fm)}-{stop or 'none'}.pkl", "rb"))


def uv_claims(votes, k=2, margin=2.0, big=None):
    out = {}
    for s, top in votes.items():
        p, v, mx = top[0]
        v2 = top[1][1] if len(top) > 1 else 0
        if (v >= k or (big and mx >= big)) and v >= margin * v2:
            out[s] = p
    return out
