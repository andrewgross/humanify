"""exp137 scorer 14: the ADVERSARIAL AUDIT draw. From ledger14.py pools (dev corpora only), a seeded sample
stratified by set of (a) units the rule relabels and (b) wrong-family claims it left alone; for each, a PRIVATE
dossier (written under the given out dir, never committed: it holds bundle and package excerpts) laying the unit's
code beside the labelled package's installed closure and the other package's installed files, for a hand verdict.

usage: <venv python> -I audit14.py <pool.jsonl>... --relabel N --missed M --seed S --out <private dir>
                                   [--min-bytes B]  (missed claims of at least B bytes only)
       writes <out>/draw.json (what was drawn: ids, sets, spans, packages) and <out>/<k>.txt dossiers; prints
       counts per set only."""
import collections
import json
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402
import mapdefect14 as M  # noqa: E402


def stratified(rows, n, rnd):
    """n rows, spread evenly over sets (each set at most what it has; the rest filled from the larger sets)."""
    by = collections.defaultdict(list)
    for r in sorted(rows, key=lambda r: (r["set"], r["entry"], r["s"], r["e"])):
        by[r["set"]].append(r)
    for v in by.values():
        rnd.shuffle(v)
    out = []
    while len(out) < n and any(by.values()):
        for s in sorted(by):
            if by[s] and len(out) < n:
                out.append(by[s].pop())
    return out


def excerpt(text, at, width=300):
    a = max(0, at - width // 3)
    return text[a:a + width].replace("\n", " ")


def dossier(r):
    lib096.use(r["corpus"])
    entry = next(e for e in lib096.load_manifest()["entries"] if e["id"] == r["entry"])
    role = "innerMap" if "innerMap" in entry["files"] else "map"
    smap = json.load(open(lib096.corpus_path(entry, role)))
    paths = M.source_paths(entry, smap, role)
    tops = [t for t in sorted({p[:p.index("/node_modules/")] + "/node_modules" for p in paths
                               if "/node_modules/" in p}) if os.path.isdir(t)]
    if not tops:
        return json.dumps(r, sort_keys=True) + "\n\nNO INSTALL TREE: the map's sources name no installed node_modules\n"
    stop = os.path.commonpath([os.path.dirname(t) for t in tops])
    files = M.tree_files(tops)
    body = open(lib096.corpus_path(entry, "bundle"), "rb").read()[r["s"]:r["e"]].decode("utf8", "replace")
    toks = sorted(M.clean(M.node({"mode": "units", "bundle": lib096.corpus_path(entry, "bundle"),
                                  "spans": [[r["s"], r["e"]]]})[0]))
    toks = M.sample(toks) if len(toks) > M.SAMPLE else toks
    out = [json.dumps({k: v for k, v in r.items()}, sort_keys=True), "",
           f"UNIT ({len(body)} chars) head: " + body[:500].replace("\n", " "),
           "UNIT tail: " + body[-200:].replace("\n", " "),
           f"EVIDENCE ({len(toks)} sampled, not df-filtered): " + " | ".join(t[:40] for t in toks[:24]), ""]

    def by_pkg(pkg):
        return [f for f in files if f[0] == pkg]
    old_roots = sorted({f[1] for f in by_pkg(r["old"])})
    for root in old_roots:
        cl = M.closure(root, stop)
        ctext = "\n".join(open(f[2], encoding="utf8", errors="replace").read() for f in files if f[1] in cl)
        out.append(f"LABEL {r['old']} root {os.path.relpath(root, stop)} closure {len(cl)} packages: "
                   f"share {M.share_in(toks, ctext):.2f}")
    if not old_roots:
        out.append(f"LABEL {r['old']}: not installed in the app tree")
    other = r.get("new") or r.get("claim")
    for pkg in dict.fromkeys([other, r.get("claim")]):
        if not pkg:
            continue
        best = []
        for f in by_pkg(pkg):
            t = open(f[2], encoding="utf8", errors="replace").read()
            best.append((M.share_in(toks, t), f[2], t))
        best.sort(key=lambda x: -x[0])
        if not best:
            out.append(f"OTHER {pkg}: not installed in the app tree")
            continue
        for sh, p, t in best[:2]:
            hit = next((t.find(x) for x in toks if x in t), -1)
            out.append(f"OTHER {pkg} file {os.path.relpath(p, stop)} ({len(t)} chars) share {sh:.2f}")
            if hit >= 0:
                out.append("   at first token: " + excerpt(t, hit))
    # every installed file holding most of the evidence (who else has it)
    holders = []
    for f in files:
        t = open(f[2], encoding="utf8", errors="replace").read()
        sh = M.share_in(toks, t)
        if sh >= 0.5:
            holders.append((round(sh, 2), f[0], os.path.relpath(f[2], stop)))
    out.append("FILES HOLDING >= 50%: " + "; ".join(f"{p} {h} {s}" for s, p, h in sorted(holders, reverse=True)[:8]))
    return "\n".join(out) + "\n"


def main():
    argv = sys.argv[1:]
    pools = [a for a in argv if a.endswith(".jsonl")]
    n_rel = int(argv[argv.index("--relabel") + 1])
    n_mis = int(argv[argv.index("--missed") + 1])
    seed = int(argv[argv.index("--seed") + 1])
    out = argv[argv.index("--out") + 1]
    min_bytes = int(argv[argv.index("--min-bytes") + 1]) if "--min-bytes" in argv else 0
    rows = [json.loads(x) for p in pools for x in open(p)]
    if any(r["corpus"] in lib096.PRIVATE for r in rows):
        raise SystemExit("REFUSED: a frozen judge is never audited")
    # one row per (entry, span): several labels share the layer
    uniq = {}
    for r in rows:
        uniq.setdefault((r["kind"], r["entry"], r["s"], r["e"]), r)
    rows = list(uniq.values())
    rnd = random.Random(seed)
    draw = stratified([r for r in rows if r["kind"] == "relabel"], n_rel, rnd) + \
        stratified([r for r in rows if r["kind"] == "missed" and r["bytes"] >= min_bytes], n_mis, rnd)
    os.makedirs(out, exist_ok=True)
    open(os.path.join(out, "draw.json"), "w").write(json.dumps(draw, indent=1, sort_keys=True))
    for k, r in enumerate(draw):
        open(os.path.join(out, f"{k:03d}-{r['kind']}.txt"), "w").write(dossier(r))
    print(collections.Counter((r["kind"], r["set"]) for r in draw))


if __name__ == "__main__":
    main()
