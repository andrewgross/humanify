"""exp136: for census rows whose truth file lacks the unit's code (dcheck.py's file-lacks-unit), WHO holds it? Search
every source file installed in the truth version's work dir (the package and its dependencies) and in the
claimer's newest installed version holding the whole print, for the file holding the most of the unit's name
evidence (dcheck.evidence), and class the best file's package against the truth T and the claimer W:
  truth-other-file     T, another file (scored right anyway: the scorer judges the package)
  claimer              W's package (the claim is right; the truth label is the map's defect)
  third-package        another package (both the truth label and the claim are wrong)
  nobody               no installed file holds half the evidence
usage: python3 -I whohas.py <corpus> <label> <rows.jsonl>... [--classes d-print-differs] ; prints KB per class only."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import dcheck  # noqa: E402
import lib096  # noqa: E402

WORK = dcheck.WORK
_FILES = {}


def files_of(work_dir):
    """[(package, path, text)] of every .js/.cjs/.mjs/.json under work_dir/node_modules (1 MB cap per file)."""
    if work_dir not in _FILES:
        if len(_FILES) > 8:
            _FILES.clear()
        out = []
        root = os.path.join(work_dir, "node_modules")
        for d, _, fs in os.walk(root, followlinks=True):
            for f in fs:
                if not f.endswith((".js", ".cjs", ".mjs", ".json")):
                    continue
                p = os.path.join(d, f)
                try:
                    if os.path.getsize(p) > 4 << 20:
                        continue
                    text = open(p, encoding="utf8", errors="replace").read()
                except OSError:
                    continue
                rel = os.path.relpath(p, root).split(os.sep)
                parts = [x for x in rel]
                # the innermost node_modules/<pkg> segment owns the file
                k = len(parts) - 1 - parts[::-1].index("node_modules") if "node_modules" in parts else -1
                seg = parts[k + 1:]
                pkg = "/".join(seg[:2]) if seg[0].startswith("@") else seg[0]
                out.append((pkg, p, text))
        _FILES[work_dir] = out
    return _FILES[work_dir]


def claimer_dirs(w, n=3):
    """The claimer's newest installed work dirs (by name order; at most n)."""
    pre = w.replace("/", "__") + "@"
    ds = sorted((d for d in os.listdir(WORK) if d.startswith(pre)), reverse=True)
    return [os.path.join(WORK, d) for d in ds if os.path.isdir(os.path.join(WORK, d, "node_modules"))][:n]


def main():
    corpus, label = sys.argv[1], sys.argv[2]
    rows = [a for a in sys.argv[3:] if a.endswith(".jsonl")]
    classes = sys.argv[sys.argv.index("--classes") + 1].split(",") if "--classes" in sys.argv else ["d-print-differs"]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    agg = collections.Counter()
    for f in rows:
        for line in open(f):
            r = json.loads(line)
            if r["cls"] not in classes or not r.get("truth") or not r.get("version") or not r.get("file"):
                continue
            T, V, F, W = r["truth"], r["version"], r["file"], r["claimer"]
            wd = os.path.join(WORK, f"{T.replace('/', '__')}@{V}")
            path = os.path.join(wd, "node_modules", T, F)
            sp = dcheck.span(label, r["entry"], r["whole"], r["bytes"])
            if not os.path.isfile(path) or sp is None:
                continue
            toks = dcheck.evidence(dcheck.bundle(r["entry"])[sp[0]:sp[1]].decode("utf8", "replace"))
            if len(toks) < 3:
                continue
            src = open(path, encoding="utf8", errors="replace").read()
            if sum(t in src for t in toks) >= 0.5 * len(toks):
                continue  # the truth file holds it
            best, who = 0.0, None
            for pkg, p, text in files_of(wd):
                s = sum(t in text for t in toks) / len(toks)
                if s > best:
                    best, who = s, pkg
            if best < 0.5:
                cls = "nobody-in-truth-install"
                for wd2 in claimer_dirs(W):
                    hit = max((sum(t in text for t in toks) / len(toks) for pkg, p, text in files_of(wd2)
                               if pkg == W), default=0.0)
                    if hit >= 0.5:
                        cls = "claimer"
                        break
            elif who == T:
                cls = "truth-other-file"
            elif who == W:
                cls = "claimer"
            else:
                cls = "third-package"
            agg[(r["set"], r["cls"], cls)] += r["bytes"]
    for k, v in sorted(agg.items()):
        print(f"{k[0]:14} {k[1]:16} {k[2]:26} {v / 1e3:8.1f} KB")


if __name__ == "__main__":
    main()
