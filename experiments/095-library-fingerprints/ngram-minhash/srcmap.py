"""(pkg, version, file) -> sha256 of the installed SOURCE file (comments kept; sourceMappingURL lines were
stripped by the lab). Two labels with the same source sha are the same module: a match between them is
correct whatever their version/package says. Output data/srcsha.json {"pkg\\tver\\tfile": sha}.
Run:  .venv/bin/python srcmap.py
"""
import hashlib
import json
import os

LAB = "/work/exp095-library-fingerprints/lab/pkgs/"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data/srcsha.json")

roots = {}  # (name, version) -> dir
for app in os.listdir(LAB):
    nm = os.path.join(LAB, app, "node_modules")
    for dirpath, dirnames, filenames in os.walk(nm):
        if "package.json" in filenames and (os.path.basename(os.path.dirname(dirpath)) == "node_modules"
                                            or os.path.basename(os.path.dirname(os.path.dirname(dirpath))) == "node_modules"):
            try:
                pj = json.load(open(os.path.join(dirpath, "package.json")))
            except Exception:
                continue
            if isinstance(pj, dict) and "name" in pj and "version" in pj:
                roots.setdefault((pj["name"], pj["version"]), dirpath)
need = set()
for line in open(os.path.join(os.path.dirname(OUT), "spans.jsonl")):
    r = json.loads(line)
    if r["set"] == "lab":
        need.add((r["pkg"], r["ver"], r["file"]))
out = {}
paths = {}
miss = 0
for pkg, ver, f in need:
    d = roots.get((pkg, ver))
    p = os.path.join(d, f) if d else None
    if p and os.path.isfile(p):
        out[f"{pkg}\t{ver}\t{f}"] = hashlib.sha256(open(p, "rb").read()).hexdigest()[:16]
        paths[f"{pkg}\t{ver}\t{f}"] = p
    else:
        miss += 1
json.dump(out, open(OUT, "w"))
json.dump(paths, open(OUT.replace("srcsha", "srcpath"), "w"))
print("files", len(need), "found", len(out), "missing", miss, "distinct sources", len(set(out.values())))
