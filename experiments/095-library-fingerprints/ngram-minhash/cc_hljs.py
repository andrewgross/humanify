"""Real-world check on Claude Code: its 188 highlight.js grammars (x2 releases) against every grammar of
highlight.js 10.7.3, 11.9.0 and 11.11.1 built by us with Bun 1.4.0 --minify (hljs/p<ver>/f/*.js).

  PYTHONHASHSEED=0 .venv/bin/python cc_hljs.py

For each CC grammar and each feature variant: the best-scoring grammar (and its version), its Jaccard and
containment, the runner-up from a DIFFERENT grammar name (the margin), and whether the best one declares
the same name. Writes results/cc_hljs.json.
"""
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fp  # noqa: E402
import lex  # noqa: E402

LAB = "/work/exp095-library-fingerprints/lab/"
VARIANTS = [("H", 5), ("B", 3), ("B", 5), ("B", 8), ("BX", 5), ("L", 3), ("L", 5), ("LX", 3), ("LIT", 0)]


def sets_of(text, stream, n):
    st, lits = lex.streams(text)
    if stream == "LIT":
        return np.unique(np.array([hash(x) for x in lits], dtype=np.int64).view(np.uint64))
    tok = np.fromiter((hash(t) for t in st[stream]), dtype=np.int64, count=len(st[stream])).view(np.uint64)
    if len(tok) < n:
        with np.errstate(over="ignore"):
            return fp.mix(np.array([(tok * fp.MULT[:len(tok)]).sum()], dtype=np.uint64))
    nw = len(tok) - n + 1
    h = np.zeros(nw, dtype=np.uint64)
    with np.errstate(over="ignore"):
        for k in range(n):
            h += tok[k:k + nw] * fp.MULT[k]
    return np.unique(fp.mix(h))


def main():
    ref = []
    for v in ("10.7.3", "11.9.0", "11.11.1"):
        for line in open(f"{fp.HERE}/hljs/p{v}/factories.jsonl"):
            r = json.loads(line)
            r["ver"] = v
            r["text"] = open(f"{fp.HERE}/hljs/{r['path']}", encoding="utf-8").read()
            ref.append(r)
    cc = []
    for cv in ("2.1.198", "2.1.216"):
        for line in open(f"{LAB}realworld/cc-{cv}/positives.jsonl"):
            r = json.loads(line)
            if r["pkg"] == "highlight.js" and r.get("member"):
                r["cc"] = cv
                r["text"] = open(LAB + r["textPath"], encoding="utf-8").read()
                cc.append(r)
    out = {}
    for s, n in VARIANTS:
        rs = [sets_of(r["text"], s, n) for r in ref]
        rows = []
        for q in cc:
            Q = sets_of(q["text"], s, n)
            best = []
            for r, R in zip(ref, rs):
                inter = len(np.intersect1d(Q, R, assume_unique=True))
                J = inter / (len(Q) + len(R) - inter)
                best.append((J, inter / len(Q), r["file"], r["ver"], r["text"]))
            best.sort(key=lambda b: (b[0], b[1]), reverse=True)
            top = best[0]
            second = next((b for b in best if b[2] != top[2]), (0, 0, None, None))
            # correct = the best reference grammar declares the CC module's grammar name
            ok = (f'name:"{q["member"]}"' in top[4]) or (f'name="{q["member"]}"' in top[4])
            tied = sorted(b[3] for b in best if b[2] == top[2] and b[0] == top[0])
            rows.append({"cc": q["cc"], "member": q["member"], "J": round(top[0], 4), "CQ": round(top[1], 4),
                         "file": top[2], "ver": "=".join(tied), "ok": ok, "second": round(second[0], 4),
                         "secondFile": second[2], "size": len(Q)})
        J = np.array([r["J"] for r in rows])
        ok = np.array([r["ok"] for r in rows])
        sec = np.array([r["second"] for r in rows])
        summ = {"n": len(rows), "top1_correct": int(ok.sum()),
                "J_p5_25_50": [round(float(x), 3) for x in np.percentile(J, [5, 25, 50])],
                "second_p50_99_max": [round(float(x), 3) for x in np.percentile(sec, [50, 99, 100])],
                "wrong": [r for r in rows if not r["ok"]][:12]}
        for t in (0.5, 0.6, 0.7, 0.8, 0.9):
            acc = J >= t
            summ[f"t{t}"] = {"accepted": int(acc.sum()), "correct": int((acc & ok).sum()), "wrong": int((acc & ~ok).sum())}
        vers = {}
        for r in rows:
            vers.setdefault(r["cc"], {}).setdefault(r["ver"], 0)
            vers[r["cc"]][r["ver"]] += 1
        summ["best_version_votes"] = vers
        out[f"{s}{n}"] = {"summary": summ, "rows": rows}
        print(s, n, json.dumps({k: v for k, v in summ.items() if k != "wrong"}), flush=True)
    json.dump(out, open(f"{fp.HERE}/results/cc_hljs.json", "w"), indent=1)


if __name__ == "__main__":
    main()
