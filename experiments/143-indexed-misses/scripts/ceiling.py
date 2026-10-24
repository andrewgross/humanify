"""exp143: a lever's simulated P / R / F0.5 per dev set from the base labels' summary.json (exp142-base: main
673c72cc0 defaults, k2-all-r5-origin-prod, scorer 14): named += right bytes gained, false positives += wrong bytes
let in (wrongFamily + app, as score.prf). Nothing else moves (no propagation from new seeds)."""
import json
import os

RES_142 = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score/results")
BASE = ("exp142-base", "exp142-base-apps", "exp142-base-bundlers", "pop-exp142-base", "remin-exp142-base",
        "remin3-exp142-base")


def base_sets():
    out = {}
    for lab in BASE:
        out.update(json.load(open(os.path.join(RES_142, lab, "summary.json")))["sets"])
    return out


def prf(named, fp, library):
    p = named / (named + fp) if named + fp else 0.0
    r = named / library if library else 0.0
    f = 1.25 * p * r / (0.25 * p + r) if p + r else 0.0
    return 100 * p, 100 * r, 100 * f


def sim(row, right_b, wrong_b):
    """(base P, R, F0.5), (new P, R, F0.5)."""
    n, fp, lib = row["namedBytes"], row["wrongFamilyBytes"] + row["appClaimedBytes"], row["libraryBytes"]
    return prf(n, fp, lib), prf(n + right_b, fp + wrong_b, lib)
