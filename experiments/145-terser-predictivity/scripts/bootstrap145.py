"""exp145: per-app precision deltas between two DEV labels, and their 36-app subsample distribution.

usage: python3 bootstrap145.py <results dir> <set> <base label> <cand label> [--n 20000] [--k 36] [--seed 145]
Reads only cards/<set>-*.json of the two labels (DEV corpora only: never pass a judge set).
Prints totals, the concentration of added wrong bytes over apps, and the subsample distribution of dP
(without replacement when the set has more apps than k, else a with-replacement bootstrap).
Output JSON on stdout's last line (key `json`)."""
import glob
import json
import os
import random
import sys

JUDGE = ("test-", "test2-", "remin2-")


def cards(results, label, set_):
    if set_.startswith(JUDGE) or label.startswith(JUDGE):
        raise SystemExit("judge corpora are never read")
    out = {}
    for p in glob.glob(os.path.join(results, label, "cards", set_ + "-*.json")):
        c = json.load(open(p))
        if c.get("set") != set_:
            continue
        t = c.get("totals") or {}
        app = c["entry"][len(set_) + 1:]
        out[app] = (t.get("namedBytes", 0), t.get("wrongFamilyBytes", 0), t.get("appClaimedBytes", 0))
    return out


def prec(rows):
    n = sum(r[0] for r in rows)
    w = sum(r[1] + r[2] for r in rows)
    return n / (n + w) if n + w else 1.0


def pct(xs, q):
    xs = sorted(xs)
    i = min(len(xs) - 1, max(0, int(round(q * (len(xs) - 1)))))
    return xs[i]


def main(argv):
    args = [a for a in argv if not a.startswith("--")]
    opts = dict(zip(argv[::1], argv[1::1]))
    n_iter = int(opts.get("--n", 20000))
    k = int(opts.get("--k", 36))
    seed = int(opts.get("--seed", 145))
    results, set_, base, cand = args[:4]
    b, c = cards(results, base, set_), cards(results, cand, set_)
    apps = sorted(set(b) & set(c))
    if set(b) ^ set(c):
        print("WARNING unmatched apps", sorted(set(b) ^ set(c)))
    pb, pc = prec([b[a] for a in apps]), prec([c[a] for a in apps])
    dw = {a: (c[a][1] + c[a][2]) - (b[a][1] + b[a][2]) for a in apps}
    dn = {a: c[a][0] - b[a][0] for a in apps}
    added = sum(max(0, v) for v in dw.values())
    top = sorted(apps, key=lambda a: -dw[a])
    conc = {f"top{t}": (sum(max(0, dw[a]) for a in top[:t]) / added if added else 0) for t in (1, 3, 5)}
    rng = random.Random(seed)
    replace = len(apps) <= k
    ds = []
    for _ in range(n_iter):
        s = [rng.choice(apps) for _ in range(k)] if replace else rng.sample(apps, k)
        ds.append(prec([c[a] for a in s]) - prec([b[a] for a in s]))
    res = {
        "set": set_, "base": base, "cand": cand, "apps": len(apps), "k": k, "replace": replace,
        "P_base": pb, "P_cand": pc, "dP": pc - pb,
        "d_named": sum(dn.values()), "d_wrong": sum(dw.values()), "added_wrong": added,
        "removed_wrong": -sum(min(0, v) for v in dw.values()),
        "apps_wrong_up": sum(1 for v in dw.values() if v > 0), "apps_wrong_down": sum(1 for v in dw.values() if v < 0),
        "conc": conc, "top_apps_dw": [dw[a] for a in top[:5]],
        "boot_p05": pct(ds, 0.05), "boot_p50": pct(ds, 0.5), "boot_p95": pct(ds, 0.95),
        "boot_share_below0": sum(1 for d in ds if d < 0) / len(ds),
    }
    print("json", json.dumps(res))


if __name__ == "__main__":
    main(sys.argv[1:])
