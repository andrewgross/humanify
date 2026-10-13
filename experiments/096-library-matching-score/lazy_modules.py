"""exp096 STATUS finding: what the esbuild app's lazy (`__esm`) modules did to its score. Compares two labels on the
esbuild app BYTE BY BYTE under the origin-aware truth: bytes named before and not after (and the reverse), each
with the unit kind and miss class it has AFTER; and how many of the lost bytes were named before through ONE
whole lazy-wrapper unit (`x=__esm(()=>{...})`, the shape the index's esbuild shards still hold).

usage: <venv python> -I lazy_modules.py [<before label> <after label>]   (default ix-v2-structural integ2-structural)
       <venv python> -I lazy_modules.py --rebuilt <index dir> <pkg>...   whole-hash coverage of those shippers'
                                                                         units, v2 index vs a rebuilt one"""
import collections
import json
import os
import re
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib096  # noqa: E402
import score  # noqa: E402

import decompose  # noqa: E402  (the exp095 misses lane's classifier)

E = "esbuild-task-master-ai"
WRAPPER = re.compile(rb"^(?:var )?[\w$]+=[\w$]+\(\(\)=>\{")


def entry():
    return {x["id"]: x for x in lib096.load_manifest()["entries"]}[E]


def label_state(label, raw, truth, org):
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ix = score.load_index(pipe["indexDir"], "esbuild", pipe["indexId"])
    rep = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", E + ".json")))["libraryDiscovery"]
    units = score.run_units(rep)
    st = score.statement_units(units)
    labels = score.label_units(truth, units, org)
    key = {(u["s"], u["e"]): u["i"] for u in st}
    claims = {key[(c["start"], c["end"])]: c for c in rep["claims"] if (c["start"], c["end"]) in key}
    vet = {key[(v["start"], v["end"])]: v["package"] for v in rep.get("vetoed", []) if (v["start"], v["end"]) in key}
    cls = np.full(len(raw), "", dtype=object)
    kind = np.full(len(raw), "", dtype=object)
    wrapper = np.zeros(len(raw), bool)
    for u in st:
        t = labels[u["i"]]
        if t["label"] in score.NOT_LIBRARY:
            continue
        c = claims.get(u["i"])
        v = score.judge_origin(c["package"], t, truth["appPackage"])[0] if c else None
        if v in ("right", "family", "origin"):
            name = "named"
            wrapper[u["s"]:u["e"]] = WRAPPER.match(raw[u["s"]:u["s"] + 40]) is not None
        elif v == "wrongFamily":
            name = "wrong"
        elif "w" not in u:
            name = "unit-model"
        else:
            name = "miss:" + decompose.classify(ix, u, t, c["package"] if c else None, vet, {}, "struct")
        txt = raw[u["s"]:u["e"]]
        k = u["k"]
        if k == "LazyBody":
            k += ":assign-seq" if re.match(rb"^[\w$]+=", txt) else (":call" if re.match(rb"^[\w$.]+\(", txt)
                                                                   else ":other")
        cls[u["s"]:u["e"]] = name
        kind[u["s"]:u["e"]] = k
    return cls, kind, wrapper


def table(title, mask, cls, kind):
    print(f"{title}: {int(mask.sum())} bytes")
    c = collections.Counter(zip(cls[mask].tolist(), kind[mask].tolist()))
    for (a, k), n in c.most_common(12):
        print(f"  {n:9d}  {a or 'non-library':16s} {k}")


def compare(before, after):
    e = entry()
    truth = lib096.load_truth(e)
    org = lib096.load_origins(e, lib096.load_origins_manifest())
    raw = open(lib096.corpus_path(e, "bundle"), "rb").read()
    ca, ka, wa = label_state(before, raw, truth, org)
    cb, kb, _ = label_state(after, raw, truth, org)
    lost = (ca == "named") & (cb != "named")
    table(f"named in {before}, not in {after} (state after)", lost, cb, kb)
    table(f"named in {after}, not in {before} (state before, kind after)", (cb == "named") & (ca != "named"), ca, kb)
    print(f"lost bytes named before through ONE whole lazy-wrapper unit: {int((lost & wa).sum())}")


def rebuilt(index_dir, pkgs):
    e = entry()
    truth = lib096.load_truth(e)
    org = lib096.load_origins(e, lib096.load_origins_manifest())

    def rows(path):
        d = collections.defaultdict(set)
        for line in open(path):
            r = json.loads(line)
            if r["owner"] in pkgs:
                d[r["hash"]].add(r["owner"])
        return d
    v2 = rows("/work/exp095-index-v2-data/final/ix/v2/units.esbuild.jsonl")
    new = rows(os.path.join(index_dir, "units.esbuild.jsonl"))
    for label in ("ix-v2-structural", "integ2-structural"):
        rep = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", E + ".json")))["libraryDiscovery"]
        units = score.run_units(rep)
        st = score.statement_units(units)
        labels = score.label_units(truth, units, org)
        c = collections.Counter()
        for u in st:
            p = labels[u["i"]]["label"]
            if p not in pkgs:
                continue
            c["bytes"] += u["b"]
            if "w" in u:
                c["wholeHashInV2"] += u["b"] if p in v2.get(u["w"][0], ()) else 0
                c["wholeHashInRebuilt"] += u["b"] if p in new.get(u["w"][0], ()) else 0
        print(label, dict(c))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--rebuilt":
        rebuilt(sys.argv[2], set(sys.argv[3:]))
    else:
        compare(*(sys.argv[1:3] or ["ix-v2-structural", "integ2-structural"]))
