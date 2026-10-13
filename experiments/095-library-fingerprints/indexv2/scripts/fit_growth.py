"""How much bigger a package's index rows get when the index holds more of its releases (lane "indexv2").

usage: python3 -I fit_growth.py [<density-bun.json>]   -> prints the fit, writes $V2/growth.json

The density lane measured, per package, the deduplicated rows under every policy (results/density-bun.json). Model:
rows(k releases) = B * (1 + a * (k - 1)) -- each extra release adds a share `a` of one release's rows (identical rows
collapse). Then a package's cost under policy P, from its cost under the as-of policy, is
    cost_P = cost_asof * (1 + a*(k_P - 1)) / (1 + a*(k_asof - 1)).
`a` is fitted per table kind (module rows, literal bytes) so the predicted TOTAL over the 101 density packages
equals the measured total for P = year-all+minor (a budget is a total; a log fit over packages is pulled by small
packages and over-predicted totals 1.6-2.6x), and the per-package error is reported.
"""
import json, os, sys

V2 = os.environ.get("V2", "/work/exp095-index-v2-data")
SRC = sys.argv[1] if len(sys.argv) > 1 else "/work/exp095-library-fingerprints/density/results/density-bun.json"


def ratio(a, kp, ka):
    return (1 + a * (kp - 1)) / (1 + a * (ka - 1))


def fit(rows):
    """rows: (k_policy, k_asof, cost_asof, cost_policy) -> a with sum(predicted) == sum(measured) (bisection)."""
    want = sum(r[3] for r in rows)
    lo, hi = 0.0, 5.0
    for _ in range(60):
        a = (lo + hi) / 2
        if sum(c * ratio(a, kp, ka) for kp, ka, c, _ in rows) > want:
            hi = a
        else:
            lo = a
    return (lo + hi) / 2


def main():
    pk = json.load(open(SRC))["packages"]
    out = {}
    for field in ("moduleRows", "literalBytes"):
        rows = []
        for p, r in pk.items():
            a, y = r["policies"].get("asof-default"), r["policies"].get("year-all+minor")
            if a and y and a.get(field) and y.get(field):
                rows.append((y["kept"], a["kept"], a[field], y[field]))
        a = fit(rows)
        errs = sorted(abs(ratio(a, kp, ka) * ca / cy - 1) for kp, ka, ca, cy in rows)
        out[field] = {"a": round(a, 4), "packages": len(rows), "medianAbsErr": round(errs[len(errs) // 2], 3),
                      "p90AbsErr": round(errs[int(len(errs) * 0.9)], 3),
                      "asofTotal": sum(r[2] for r in rows), "policyTotal": sum(r[3] for r in rows),
                      "measuredRatio": round(sum(r[3] for r in rows) / sum(r[2] for r in rows), 2)}
        print(field, out[field])
    os.makedirs(V2, exist_ok=True)
    json.dump(out, open(V2 + "/growth.json", "w"), indent=1)


if __name__ == "__main__":
    main()
