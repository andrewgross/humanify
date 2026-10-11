"""Stop-lists: unit hashes and whole-module hashes that must never vote for a package.
usage: PYTHONHASHSEED=0 .venv/bin/python scripts/stoplists.py
-> data/stoplists.json  {"<fam>:<list>": [unit hashes as decimal strings]}, and "<fam>:<list>:M" for module hashes
Lists:
  runtime    every unit of the index builds' bundler runtime, unmapped namespace glue and app stubs (both bundlers)
  helpers    compiler / transpiler helper packages: tslib, @babel/runtime*, @babel/helpers, @swc/helpers,
             regenerator-runtime, @oxc-project/runtime  (their units are what tsc/Babel/SWC inline into other packages)
  stainless  Stainless-generated SDK template: files under internal/, core/, _shims/, _vendor/, uploads, streaming,
             error, resource of every indexed package whose source carries the X-Stainless-Lang header (here only
             @anthropic-ai/sdk is indexed; openai and groq-sdk are held-out foreign roots)
  shim       the ljharb es-shims template: implementation.js / polyfill.js / shim.js / auto.js of any package that ships
             both polyfill.js and shim.js
"""
import json, re, collections
import numpy as np
from lib import *

U = units()
HELPER = re.compile(r"^(tslib|@babel/runtime(-corejs\d)?|@babel/helpers|@swc/helpers|regenerator-runtime|@oxc-project/runtime)$")
STAIN_FILE = re.compile(r"(^|/)(internal|core|_shims|_vendor)/|(^|/)(uploads|streaming|error|resource|core|api-promise|pagination)\.(m?js|cjs)$")
SHIM_FILE = re.compile(r"(^|/)(implementation|polyfill|shim|auto)\.js$")
# Stainless packages: source carries the X-Stainless-Lang header
stain_csha = set()
for l in open(D + "srclits-content.jsonl", encoding="utf8", errors="surrogatepass"):
    if "X-Stainless-Lang" in l or "x-stainless-lang" in l:
        try:
            stain_csha.add(json.loads(l)["csha"])
        except Exception:
            pass
stain = set()
files_of = collections.defaultdict(set)
for l in open(D + "srcfiles.jsonl"):
    r = json.loads(l)
    files_of[r["pkg"]].add(r["file"])
    if r["csha"] in stain_csha:
        stain.add(r["pkg"])
shimpk = {p for p, fs in files_of.items() if any(f.endswith("polyfill.js") for f in fs) and any(f.endswith("shim.js") for f in fs)}
print("stainless packages", sorted(stain), "shim-template packages", len(shimpk))
import os
F = fph() if os.path.exists(D + "fph.pkl") else {}
shas = json.load(open(D + "shas.json"))
out = {}
for fam in ("bun", "esbuild"):
    sids = collections.defaultdict(set)
    for s in spans():
        if s["corpus"] != "ix":
            continue
        if s["pkg"] in ("<runtime>", "<unmapped>", "<app>", "<bundler-helper>"):
            sids["runtime"].add(s["sid"])  # both bundlers' runtimes: never a library name
        if s["fam"] != fam or not is_lib(s["pkg"]):
            continue
        if HELPER.match(s["pkg"]):
            sids["helpers"].add(s["sid"])
        if s["pkg"] in stain and s["file"] and STAIN_FILE.search(s["file"]):
            sids["stainless"].add(s["sid"])
        if s["pkg"] in shimpk and s["file"] and SHIM_FILE.search(s["file"]):
            sids["shim"].add(s["sid"])
    for name, ss in sids.items():
        a = np.array(sorted(ss))
        m = (U["typ"] != 0) & np.isin(U["sid"], a)
        out[f"{fam}:{name}"] = [str(int(x)) for x in np.unique(U["h"][m])]
        out[f"{fam}:{name}:M"] = sorted({F[shas[i]][0] for i in ss if shas[i] in F and F[shas[i]][0]})
        print(fam, name, "spans", len(ss), "unit hashes", len(out[f"{fam}:{name}"]), "module hashes", len(out[f"{fam}:{name}:M"]), flush=True)
out["stainless_pkgs"] = sorted(stain)
out["shim_pkgs"] = sorted(shimpk)
json.dump(out, open(D + "stoplists.json", "w"))
