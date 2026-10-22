"""exp136: every wrong index seed, read against the INSTALLED CODE (name evidence, dcheck.unit_evidence): does the truth's
file hold the unit, does the claimer's indexed file (the row the lookup read) hold it, and does the claimer's
package hold it anywhere? From noprint.py --dump-all rows (scratch; names never printed).

  truth file    holds / lacks / unknown (no version, not installed)
  claimer row   the claimer's index row of the unit's WHOLE print: its file holds / lacks / no-row (the claim rests
                on function prints) / not-installed
  claimer pkg   any installed file of the claimer's package (newest 3 installs, .js/.cjs/.mjs/.json) holds it

What a builder EVIDENCE CHECK (drop a unit row whose attributed file lacks the unit's evidence) would do to the
claim: drop it when `claimer row = lacks` for every claimer row of the print (approximated by the whole's rows).

usage: python3 -I seedcheck.py <corpus> <label> <all.jsonl> [--json out]   prints KB per (set, truth, row, pkg)."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import dcheck  # noqa: E402
import lib096  # noqa: E402
import whohas  # noqa: E402

WORK = dcheck.WORK


def holds(path, toks):
    try:
        src = open(path, encoding="utf8", errors="replace").read()
    except OSError:
        return None
    return sum(t in src for t in toks) >= 0.5 * len(toks)


def row_state(ix, whole, win, toks):
    """holds / lacks / no-row / not-installed for the claimer's index rows of the whole print."""
    states = []
    for row in ix.units.get(whole, ()):
        if row[0] != win:
            continue
        vs = sorted(ix.expand(win, row[4]))
        st = "not-installed"
        for v in reversed(vs):
            p = os.path.join(WORK, f"{win.replace('/', '__')}@{v}", "node_modules", win, row[1])
            h = holds(p, toks)
            if h is not None:
                st = "holds" if h else "lacks"
                break
        states.append(st)
    if not states:
        return "no-row"
    return "holds" if "holds" in states else "lacks" if "lacks" in states else "not-installed"


def main():
    corpus, label, path = sys.argv[1:4]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import score
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    agg = collections.Counter()
    pkg_cache = {}
    for line in open(path):
        r = json.loads(line)
        sl = r["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
        sp = dcheck.span(label, r["entry"], r["whole"], r["bytes"])
        if sp is None:
            continue
        toks = dcheck.unit_evidence(r["entry"], sp)
        if len(toks) < 3:
            agg[(r["set"], "no-evidence", "-", "-")] += r["bytes"]
            continue
        T, V, F, W = r["truth"], r["version"], r["file"], r["claimer"]
        t = None
        if T and V and F:
            t = holds(os.path.join(WORK, f"{T.replace('/', '__')}@{V}", "node_modules", T, F), toks)
        tstate = "unknown" if t is None else "holds" if t else "lacks"
        rstate = row_state(ixs[sl], r["whole"], W, toks)
        key = (W, tuple(sorted(toks)))
        if key not in pkg_cache:
            hit = False
            for wd in whohas.claimer_dirs(W):
                if any(pkg == W and sum(x in text for x in toks) >= 0.5 * len(toks)
                       for pkg, _, text in whohas.files_of(wd)):
                    hit = True
                    break
            pkg_cache[key] = "holds" if hit else "lacks"
        agg[(r["set"], tstate, rstate, pkg_cache[key])] += r["bytes"]
    out = [{"set": k[0], "truthFile": k[1], "claimerRow": k[2], "claimerPackage": k[3], "kb": round(v / 1e3, 1)}
           for k, v in sorted(agg.items())]
    for o in out:
        print(f"{o['set']:14} truth {o['truthFile']:12} row {o['claimerRow']:14} pkg {o['claimerPackage']:6} "
              f"{o['kb']:8.1f} KB")
    if "--json" in sys.argv:
        json.dump(out, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1)


if __name__ == "__main__":
    main()
