"""Collect every span of every corpus into one table, and the DISTINCT span texts as feature tasks.
usage: python3 scripts/collect.py
-> data/spans.pkl   list of dicts (corpus, build, fam, app, mode, idx, pkg, ver, file, kind, marker, part, parts,
                    purity, bytes, sha, path, bundle, pos)   pos = root rank position of an ix app (index scale)
-> data/tasks.jsonl one row per distinct sha: {k: sha, path, slice?}   (features.mjs / b5.py input)
-> data/fphash-jobs.jsonl
Corpora:
  ix     the new index builds (scale/ix), Bun 1.4.0 + esbuild 0.28.2, req + dyn
  lab    the shared lab, minified bun-1.4.0 and esbuild-0.28.2 builds (positives for recall; versions may differ)
  flab   subtree-hashes/verify/flab: 83 foreign packages, Bun 1.4.0 min, req + dyn
  neg    immutable-literals/verify/neg: 75 packages, Bun 1.4.0 min, imp/req/dyn (relabelled here: q/neg)
  fresh  exact-module-hash/verify/fresh: 400 packages (rank 150-3000) + deps, Bun 1.4.0 min, req/imp/dyn
  cc216  Claude Code 2.1.216: every CJS factory and lazy-ESM group of the raw bundle (lab/realworld/layout-2.1.216.json)
  ccneg  lab/realworld/cc-2.1.216 negatives (100 app groups) and negative-functions (222)
"""
import json, hashlib, pickle, collections, os
X = "/work/exp095-library-fingerprints/"
S = X + "scale/"
CCB = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js"

pos_of = {}
for l in open(S + "data/versions.jsonl"):
    r = json.loads(l)
    d = r["name"].replace("/", "__") + "@" + r["version"]
    pos_of[d] = min(pos_of.get(d, 10**9), r["pos"])

flab_foreign = {f'{p["name"].replace("/", "__")}@{p["versions"][0]}' for p in json.load(open(X + "subtree-hashes/verify/flab/versions.json")) if p.get("set") == "foreign"}

spans = []


def add(corpus, root, labels, keep):
    n = 0
    for l in open(labels):
        r = json.loads(l)
        if not keep(r):
            continue
        b = r["build"]
        spans.append(dict(corpus=corpus, build=b, fam=r["bundler"], app=r["app"], mode=r["mode"], idx=r["idx"], pkg=r["pkg"],
                          ver=r["pkgVersion"], file=r["file"], kind=r["kind"], marker=r.get("marker"), part=r["part"], parts=r["parts"],
                          purity=r["purity"], bytes=r["bytes"], sha=r["sha256"], path=root + r["textPath"],
                          bundle=root + "builds/" + b + "/out.js", pos=pos_of.get(r["app"]) if corpus == "ix" else None))
        n += 1
    print(corpus, n, flush=True)


add("ix", S + "ix/", S + "ix/all.jsonl", lambda r: True)
add("lab", X + "lab/", X + "lab/labels/all.jsonl", lambda r: r["build"].split("/")[0] in ("bun-1.4.0", "esbuild-0.28.2") and r["minified"] and r["mode"] != "some")
add("flab", X + "subtree-hashes/verify/flab/", X + "subtree-hashes/verify/flab/labels/all.jsonl",
    lambda r: r["build"].startswith("bun-1.4.0/min/") and r["app"] in flab_foreign)
add("neg", S + "q/neg/", S + "q/neg/all.jsonl", lambda r: True)
add("fresh", X + "exact-module-hash/verify/fresh/", X + "exact-module-hash/verify/fresh/labels-all.jsonl", lambda r: r["build"].startswith("bun-1.4.0/"))

# Claude Code 2.1.216: layout units (slices of the raw bundle)
code = open(CCB, encoding="utf8").read()
lay = json.load(open(X + "lab/realworld/layout-2.1.216.json"))["modules"]
cjs_i = 0
for i, m in enumerate(lay):
    t = code[m["startChar"]:m["endChar"]]
    sha = hashlib.sha256(t.encode()).hexdigest()[:16]
    spans.append(dict(corpus="cc216", build="cc-2.1.216", fam="bun", app="cc", mode="cc", idx=i, pkg="<cc>", ver=None, file=None,
                      kind=m["kind"], marker=None, part=0, parts=1, purity=None, bytes=len(t.encode()), sha=sha, path=CCB,
                      slice=[m["startChar"], m["endChar"]], bundle=None, pos=None, cjsIndex=cjs_i if m["kind"] == "cjs" else None))
    if m["kind"] == "cjs":
        cjs_i += 1
print("cc216", len(lay))
for st in ("negatives", "negative-functions", "positives"):
    for i, l in enumerate(open(X + f"lab/realworld/cc-2.1.216/{st}.jsonl")):
        r = json.loads(l)
        p = X + "lab/" + r["textPath"]
        t = open(p, encoding="utf8").read()
        spans.append(dict(corpus="cc-" + st, build="cc-2.1.216", fam="bun", app="cc", mode="cc", idx=i, pkg=r.get("pkg", "<app>") if st == "positives" else "<app>",
                          ver=r.get("pkgVersion"), file=None, kind=r.get("kind", "lazy-esm"), marker=None, part=0, parts=1, purity=None,
                          bytes=len(t.encode()), sha=hashlib.sha256(t.encode()).hexdigest()[:16], path=p, bundle=None, pos=None,
                          member=r.get("member"), vendorFile=r.get("vendorFile")))
# the negatives lane's hand-labelled CC 2.1.216 src/ statement units (random sample + review queue + traps)
NL = X + "negatives/"
seen_u = set()
for fn in ("labels.jsonl", "traps.jsonl"):
    if not os.path.exists(NL + fn):
        continue
    for l in open(NL + fn):
        r = json.loads(l)
        if "unit" not in r:  # review-queue rows carry units: [...] and queueIndex
            r["unit"] = "q%d" % r["queueIndex"]
        if r["unit"] in seen_u:
            continue
        seen_u.add(r["unit"])
        t = code[r["start"]:r["end"]]
        lab = r["label"]
        spans.append(dict(corpus="ccsrc", build="cc-2.1.216", fam="bun", app="cc", mode="cc", idx=r["unit"],
                          pkg=(r.get("pkg") or lab.split(":", 1)[1]) if lab.startswith("library:") else "<" + lab + ">",
                          ver=r.get("version_if_known"), file=None, kind="stmt", marker=None, part=0, parts=1, purity=None,
                          bytes=len(t.encode()), sha=hashlib.sha256(t.encode()).hexdigest()[:16], path=CCB, slice=[r["start"], r["end"]],
                          bundle=None, pos=None, stratum=r.get("stratum"), trap=r.get("trap"), label=lab, fork_of=r.get("fork_of")))
print("ccsrc", len(seen_u))
print("total spans", len(spans))
pickle.dump(spans, open(S + "data/spans.pkl", "wb"))

tasks = {}
fj = {}
for s in spans:
    if s["sha"] in tasks:
        continue
    t = {"k": s["sha"], "path": s["path"]}
    if s.get("slice"):
        t["slice"] = s["slice"]
    tasks[s["sha"]] = t
    j = {"id": s["sha"], "kind": s["kind"] if s["kind"] in ("cjs", "lazy-esm", "eager-esm") else "other"}
    if s.get("slice"):
        j["text"] = code[s["slice"][0]:s["slice"][1]]
    else:
        j["path"] = s["path"]
    if s.get("bundle"):
        j["bundle"] = s["bundle"]
    fj[s["sha"]] = j
with open(S + "data/tasks.jsonl", "w") as f:
    for t in tasks.values():
        f.write(json.dumps(t) + "\n")
with open(S + "data/fphash-jobs.jsonl", "w") as f:
    for j in fj.values():
        f.write(json.dumps(j) + "\n")
print("distinct texts", len(tasks), collections.Counter(s["corpus"] for s in spans))
