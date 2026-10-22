"""exp136: is a census row's TRUTH file really where the unit's code comes from? For every row of a class (default
d-print-differs and f-filtered), read the unit's text from the bundle and the truth version's installed file
(node_modules/<T>/<F> in the index's work dir), and compare the unit's NAME EVIDENCE — its string and template
values and `.property` names of 4+ characters, which no minifier renames, read by acorn's tokenizer
(evtokens.mjs) and cleaned (`clean`) — against the file's text:
  file-lacks-unit     under half of the unit's evidence tokens occur in F: the source map credits F with code F
                      does not hold (a map defect: the truth label is wrong, the claimer may be right)
  file-holds-unit     at least half occur: the truth's file does hold this code, the index print differs
  no-evidence         the unit has fewer than 3 evidence tokens
  file-missing        T@V's file is not installed in the work dir
With --index, a `file-lacks-unit` row is split by the CLAIMER: the claimer's index row of the unit's whole print
(file, newest version it spans) read the same way -> `/claimer-holds` (the code is the claimer's file's: the truth
label is the defect) or `/claimer-lacks` (neither holds it).

usage: python3 -I dcheck.py <corpus> <label> <rows.jsonl>... [--classes d-print-differs,f-filtered] [--json out]
                            [--index]  (the label's index, as noprint.py reads it)
prints KB per (set, class, sub-cause); never a package name."""
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

WORK = "/work/exp135-data/full/work"
ACORN = "/work/exp105-data/tools/node_modules"
# property / string names nearly every file holds: no evidence of WHICH file a unit came from
COMMON = set("""length prototype call apply bind push pop shift unshift slice splice concat join split indexOf
includes forEach map filter reduce some every keys values entries assign create freeze defineProperty
getOwnPropertyNames getOwnPropertyDescriptor hasOwnProperty toString valueOf constructor name type value
data default exports module then catch finally resolve reject done next return throw charCodeAt charAt
substring substr replace match test exec trim toLowerCase toUpperCase startsWith endsWith isArray
string number object function boolean undefined symbol bigint message stack error code options args
start end size from emit listeners parent node props state self window document process env""".split())


def clean(toks):
    """The tokens that can be looked up verbatim in a source file: 4..200 characters with three letters in a row,
    no character a source would have to escape (quote, backslash, newline, tab), less COMMON."""
    return {t for t in toks if 4 <= len(t) <= 200 and re.search(r"[A-Za-z]{3}", t)
            and not any(c in t or c < " " for c in "\"'`\\") and all(" " <= c for c in t)} - COMMON


def acorn_evidence(bundle_path, spans):
    """Per span, the unit's evidence read by acorn's tokenizer (evtokens.mjs): string / template values and
    `.property` names, cleaned (`clean`)."""
    import subprocess
    r = subprocess.run(["node", os.path.join(HERE, "evtokens.mjs")], input=json.dumps({"bundle": bundle_path,
                       "spans": spans}), capture_output=True, text=True, env={**os.environ, "NODE_PATH": ACORN})
    return [clean(t) for t in json.loads(r.stdout)]


def unit_evidence(eid, sp):
    """One bundle unit's evidence (acorn_evidence of its span)."""
    return acorn_evidence(bundle_path(eid), [list(sp)])[0]


def bundle_path(eid):
    d = os.path.join(lib096.DATA, "corpus", eid)
    return os.path.join(d, sorted(x for x in os.listdir(d) if x.endswith((".js", ".mjs", ".cjs")))[0])


_BUNDLE = {}


def bundle(eid):
    if eid not in _BUNDLE:
        _BUNDLE.clear()
        d = os.path.join(lib096.DATA, "corpus", eid)
        js = sorted(x for x in os.listdir(d) if x.endswith((".js", ".mjs", ".cjs")))
        _BUNDLE[eid] = open(os.path.join(d, js[0]), "rb").read()
    return _BUNDLE[eid]


_SPANS = {}


def span(label, eid, whole, nbytes):
    if eid not in _SPANS:
        _SPANS.clear()
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", eid + ".json")))
        m = collections.defaultdict(list)
        for u in doc["libraryDiscovery"].get("unitRows", ()):
            if "whole" in u:
                m[u["whole"]["hash"]].append((u["start"], u["end"]))
        _SPANS[eid] = m
    c = _SPANS[eid].get(whole, ())
    exact = [s for s in c if s[1] - s[0] == nbytes]
    return (exact or list(c) or [None])[0]


def claimer_holds(ix, r, toks):
    """Does the claimer's indexed file holding the unit's whole print hold the unit's evidence?"""
    win = r["claimer"]
    for row in ix.units.get(r["whole"], ()):
        if row[0] != win:
            continue
        vs = sorted(ix.expand(win, row[4]), key=lambda v: [int(x) if x.isdigit() else 0 for x in re.split(r"[.+-]", v)])
        for v in reversed(vs):
            path = os.path.join(WORK, f"{win.replace('/', '__')}@{v}", "node_modules", win, row[1])
            if os.path.isfile(path):
                src = open(path, encoding="utf8", errors="replace").read()
                return "claimer-holds" if sum(t in src for t in toks) >= 0.5 * len(toks) else "claimer-lacks"
        return "claimer-not-installed"
    return "claimer-no-whole-row"


def main():
    corpus, label = sys.argv[1], sys.argv[2]
    files = [a for a in sys.argv[3:] if a.endswith(".jsonl")]
    classes = (sys.argv[sys.argv.index("--classes") + 1].split(",") if "--classes" in sys.argv
               else ["d-print-differs", "f-filtered"])
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    agg = collections.Counter()
    index = None
    if "--index" in sys.argv:
        import score
        pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
        ixs = {}

        def index(sl):
            if sl not in ixs:
                ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
            return ixs[sl]
    for f in files:
        for line in open(f):
            r = json.loads(line)
            if r["cls"] not in classes or not r.get("truth") or not r.get("whole"):
                continue
            sub = r["sub"].split("/")[0].split("-same-size")[0]
            key = (r["set"], r["cls"], sub)
            T, V, F = r["truth"], r.get("version"), r.get("file")
            path = os.path.join(WORK, f"{T.replace('/', '__')}@{V}", "node_modules", T, F or "")
            if not V or not F or not os.path.isfile(path):
                agg[key + ("file-missing",)] += r["bytes"]
                continue
            sp = span(label, r["entry"], r["whole"], r["bytes"])
            if sp is None:
                agg[key + ("no-span",)] += r["bytes"]
                continue
            toks = unit_evidence(r["entry"], sp)
            if len(toks) < 3:
                agg[key + ("no-evidence",)] += r["bytes"]
                continue
            src = open(path, encoding="utf8", errors="replace").read()
            share = sum(t in src for t in toks) / len(toks)
            cause = "file-holds-unit" if share >= 0.5 else "file-lacks-unit"
            if cause == "file-lacks-unit" and index is not None:
                cause += "/" + claimer_holds(index(r["slice"]), r, toks)
            agg[key + (cause,)] += r["bytes"]
    out = [{"set": k[0], "cls": k[1], "sub": k[2], "cause": k[3], "kb": round(v / 1e3, 1)} for k, v in sorted(agg.items())]
    for o in out:
        print(f"{o['set']:14} {o['cls']:16} {o['sub']:22} {o['cause']:18} {o['kb']:8.1f} KB")
    if "--json" in sys.argv:
        json.dump(out, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1)


if __name__ == "__main__":
    main()
