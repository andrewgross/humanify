"""Experiment A step 4: real recognition on held-out apps — greedy-selected index vs download-ranked index of the
same size, both built by humanify-index, looked up with the design rule (libix.py).

usage:
  python3 -I heldout.py bundles             Bun 1.4.0 --minify builds of every held-out app's entry (same entry,
                                            externals, loaders as its closure) + fpq with the sourcemap
  python3 -I heldout.py lists <codeMB>...   package lists: greedy order and download order cut at each code-MB budget
                                            -> $SEL/lists/{greedy,download}-<MB>.txt
  (build the indexes: build-index.sh <list> <name>)
  <venv python> -I heldout.py eval <index name>...   -> $SEL/results/heldout-<name>.json

Truth: a module's owner by the Bun build's own sourcemap (humanify-index's attribution: >= 90% of the module's
mapping segments from one source file, owner = nearest package.json under the last node_modules/). A module
attributed to the app's own files (the CLI's own package, the entry stub) or not attributable is APP code: any
name on it is a false name. Library bytes in the denominator are ALL bytes of the bundle the sourcemap attributes
to a dependency (fpq's whole-bundle attribution), so recall counts what the index cannot key (ESM-hoisted code,
lazy ESM) as missed.
"""
import json, os, subprocess, sys, glob, collections

SEL = os.environ.get("SEL", "/work/exp095-library-fingerprints/selection")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
BUN = "/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun"
FPQ = SEL + "/fpq-target/release/fpq"


def heldout_apps():
    sel = json.load(open(SEL + "/selection.json"))
    out = []
    for f in sorted(glob.glob(SEL + "/apps/*/closure.json")):
        r = json.load(open(f))
        if r["id"] in sel["heldout"] and r.get("packages"):
            out.append(r)
    return out


def bundles():
    import closure
    jobs = []
    for a in heldout_apps():
        d = f"{SEL}/apps/{a['slug']}"
        o = f"{SEL}/heldout/{a['slug']}"
        os.makedirs(o, exist_ok=True)
        if not os.path.exists(o + "/entry.js") and not (os.path.exists(o + "/build.json") and json.load(open(o + "/build.json")).get("ok")):
            ok, ext, ld, esm, log = closure.bundle(d, tool=BUN, out_dir=o)
            if not ok and "top-level await" in log:
                # Bun refuses require() of an ES module with top-level await: the same entry, as static imports
                import re as _re
                specs = _re.findall(r'require\(("[^"]+")\)', open(d + "/entry.cjs").read())
                open(d + "/entry.mjs", "w").write("".join(f"import * as m{i} from {x};\nexport {{ m{i} }};\n" for i, x in enumerate(specs)))
                ok, ext, ld, esm, log = closure.bundle(d, tool=BUN, out_dir=o, entry_file="entry.mjs")
                if ok and os.path.exists(o + "/entry.js"):
                    pass
            json.dump({"ok": ok, "externals": ext, "loaders": ld, "esm": esm, "log": log}, open(o + "/build.json", "w"))
            if not ok:
                print("FAILED", a["id"], log[-300:])
                continue
        js = o + "/entry.js"
        if os.path.exists(js) and not os.path.exists(o + "/q.jsonl"):
            jobs.append(f"bun\t{js}\t{js}.map\t{o}/q.jsonl")
    open(SEL + "/heldout/jobs.tsv", "w").write("\n".join(jobs) + "\n")
    subprocess.run([FPQ, SEL + "/heldout/jobs.tsv"], env=dict(os.environ, RAYON_NUM_THREADS="8"), check=False)


def lists(budgets):
    sel = json.load(open(SEL + "/selection.json"))
    os.makedirs(SEL + "/lists", exist_ok=True)
    for strat in ("greedy", "download"):
        for b in budgets:
            out, spent = [], 0.0
            for n in sel[strat]:
                c = sel["cost"][n]
                if spent + c > b * 1e6:
                    break
                out.append(n)
                spent += c
            open(f"{SEL}/lists/{strat}-{b}.txt", "w").write("\n".join(out) + "\n")
            print(strat, b, len(out), round(spent / 1e6, 2))


def evaluate(names):
    import libix
    apps = heldout_apps()
    qsets = []
    for a in apps:
        o = f"{SEL}/heldout/{a['slug']}"
        if not os.path.exists(o + "/q.jsonl"):
            continue
        q = [json.loads(l) for l in open(o + "/q.jsonl")]
        own = json.load(open(o + "/q.jsonl.owners.json"))
        app_name = a["id"].split(":", 1)[1] if a["kind"] == "cli" else None
        lib = sum(v for k, v in own.items() if not k.startswith(("app:", "(unmapped)")) and k.rsplit("@", 1)[0] != app_name)
        for r in q:
            r["cls"] = "mixed" if r.get("mixed") else ("self" if app_name and r.get("owner") == app_name else ("lib" if r.get("owner") else "app"))
            # a mixed span (no file owns >= 90% of it) is judged against its majority owner; the CLI's own package
            # is judged as itself (a CLI that is also a library, e.g. browserslist, is rightly named)
            r["truth"] = r.get("majorOwner") if r["cls"] == "mixed" else r.get("owner")
        qsets.append((a, q, lib))
    for name in names:
        ixd = f"{SEL}/ix/{name}"
        ix = libix.Index.from_dir(ixd)
        mb = sum(os.path.getsize(os.path.join(ixd, f)) for f in os.listdir(ixd)) / 1e6
        tot = collections.Counter()
        per_app, wrong_examples = [], []
        for a, q, lib in qsets:
            cl = ix.claims(q)
            c = collections.Counter()
            for r, x in zip(q, cl):
                b = r["end"] - r["start"]
                v = libix.verdict(r["truth"], x["name"])
                cls = r["cls"]
                c[f"{cls}:{v}"] += b
                c[f"{cls}:keyed"] += b
                if v == "wrong" and len(wrong_examples) < 200:
                    wrong_examples.append({"app": a["id"], "cls": cls, "truth": r["truth"], "file": r.get("file") or r.get("source"), "name": x["name"], "bytes": b})
            c["libBytes"] += lib
            tot.update(c)
            per_app.append({"app": a["id"], "libBytes": lib, "right": c["lib:right"] + c["lib:family"], "wrong": c["lib:wrong"] + c["app:wrong"]})
        res = {"index": name, "indexMB": round(mb, 2), "size": ix.size(), "apps": len(qsets), "bytes": dict(tot),
               "recall": round((tot["lib:right"] + tot["lib:family"]) / max(tot["libBytes"], 1), 4),
               "recallKeyed": round((tot["lib:right"] + tot["lib:family"]) / max(tot["lib:keyed"], 1), 4),
               "wrongShareOfNamed": round((tot["lib:wrong"] + tot["app:wrong"]) /
                                          max(tot["lib:right"] + tot["lib:family"] + tot["lib:wrong"] + tot["app:wrong"], 1), 4),
               "mixedNamed": {k: tot[f"mixed:{k}"] for k in ("right", "family", "wrong")},
               "selfNamed": {k: tot[f"self:{k}"] for k in ("right", "family", "wrong")},
               "perAppMeanRecall": round(sum(p["right"] / p["libBytes"] for p in per_app if p["libBytes"]) / max(1, sum(1 for p in per_app if p["libBytes"])), 4),
               "perApp": per_app, "wrongExamples": wrong_examples}
        os.makedirs(SEL + "/results", exist_ok=True)
        json.dump(res, open(f"{SEL}/results/heldout-{name}.json", "w"), indent=1)
        print(name, {k: res[k] for k in ("indexMB", "apps", "recall", "recallKeyed", "wrongShareOfNamed", "perAppMeanRecall")}, flush=True)


def cc(names):
    """Claude Code 2.1.216's 1,647 CJS factories (fpq, density/cc/cc-2.1.216.jsonl). No sourcemap: the reference
    names are (1) the pipeline's declared identities (vendor manifest identity.package; exact) and (2) the scale
    lane's design-rule claims at top-2,000 (results/cc-vendor-design-R2000.tsv; hand-checked 39/40 right). A name
    the reference does not cover is counted as 'unreferenced'."""
    import libix
    q = [json.loads(l) for l in open("/work/exp095-library-fingerprints/density/cc/cc-2.1.216.jsonl")]
    man = json.load(open("/tmp/eval-work/candidate-e214db7a-scratch/2.1.216/vendor/_bun-modules.json"))["factories"]
    ref = {}
    for l in open("/work/exp095-library-fingerprints/scale/results/cc-vendor-design-R2000.tsv"):
        f = l.rstrip("\n").split("\t")
        if f[0].isdigit():
            ref[int(f[0])] = ("scale-R2000", f[4])
    for i, m in enumerate(man):
        if m.get("identity"):
            ref[i] = ("declared", m["identity"]["package"])
    for name in names:
        ix = libix.Index.from_dir(f"{SEL}/ix/{name}")
        cl = ix.claims(q)
        c = collections.Counter()
        for i, (r, x) in enumerate(zip(q, cl)):
            b = r["end"] - r["start"]
            c["bytes"] += b
            if not x["name"]:
                continue
            c["named"] += b
            c["namedN"] += 1
            src, rp = ref.get(i, (None, None))
            v = "unreferenced" if rp is None else libix.verdict(rp, x["name"])
            c[f"{src}:{v}"] += b
            c[f"{src}:{v}:n"] += 1
        res = {"index": name, "factories": len(q), "bytes": dict(c)}
        json.dump(res, open(f"{SEL}/results/cc-{name}.json", "w"), indent=1)
        print(name, dict(c), flush=True)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "bundles":
        bundles()
    elif cmd == "lists":
        lists([float(x) for x in sys.argv[2:]])
    elif cmd == "eval":
        evaluate(sys.argv[2:])
    elif cmd == "cc":
        cc(sys.argv[2:])
