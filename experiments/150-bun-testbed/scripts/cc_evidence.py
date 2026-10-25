"""exp150: labelling aid for the Claude Code sample — per sampled unit, its text and which reference packages hold its
distinctive literals. Never the library index (the labels judge it).

usage: python3 -I cc_evidence.py <neg dir> <bundle> <reference install dir> [--jobs N]
reads  <neg dir>/data/sample.jsonl (sample.py's draw)
writes <neg dir>/units/<i>.js (the unit's bytes), <neg dir>/evidence/<i>.json:
       {"strings": n, "idents": n, "packages": [[pkg, distinct literals matched, example literals], ...]}
Literals: string literals of >= 8 characters (not all-lowercase words, not paths of the app), and property / member
names of >= 10 characters. A package is credited once per literal found in any of its .js/.cjs/.mjs files. The labels
are judgement (README conventions); this only says where to look. Everything written stays under <neg dir> (it quotes
Claude Code: /work/cc-library-truth/, never committed).
"""
import collections
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

STR = re.compile(r'"((?:[^"\\\n]|\\.){8,200})"|\'((?:[^\'\\\n]|\\.){8,200})\'')
IDENT = re.compile(r"\.([A-Za-z_$][\w$]{9,60})\b|\b([A-Za-z_$][\w$]{9,60}):")
MAX_PATTERNS = 60


def literals(text):
    strs, ids = collections.Counter(), collections.Counter()
    for m in STR.finditer(text):
        s = m.group(1) or m.group(2)
        if "\\" in s or re.fullmatch(r"[a-z]+", s):
            continue
        strs[s] += 1
    for m in IDENT.finditer(text):
        ids[m.group(1) or m.group(2)] += 1
    pick = lambda c: sorted(c, key=lambda s: (-len(s), s))[:MAX_PATTERNS // 2]  # noqa: E731
    return pick(strs), pick(ids)


def package_of(path):
    parts = path.split("/")
    k = len(parts) - 1 - parts[::-1].index("node_modules")
    return "/".join(parts[k + 1:k + 3]) if parts[k + 1].startswith("@") else parts[k + 1]


def search(ref, pats, word):
    if not pats:
        return {}
    cmd = ["rg", "-F", "-o", "--no-line-number", "--with-filename", "-g", "*.js", "-g", "*.cjs", "-g", "*.mjs",
           "-g", "!*.min.js", "--no-ignore", "--hidden"] + (["-w"] if word else [])
    for p in pats:
        cmd += ["-e", p]
    out = subprocess.run(cmd + [ref], capture_output=True, text=True, errors="replace").stdout
    hits = collections.defaultdict(set)
    for line in out.splitlines():
        f, _, m = line.partition(":")
        if "node_modules" in f:
            hits[package_of(f)].add(m)
    return hits


def one(job):
    neg, src, ref, u = job
    text = src[u["s"]:u["e"]]
    open(f"{neg}/units/{u['i']}.js", "w").write(text)
    strs, ids = literals(text)
    hs, hi = search(ref, strs, False), search(ref, ids, True)
    pk = {}
    for p in set(hs) | set(hi):
        found = sorted(hs.get(p, set()) | hi.get(p, set()), key=len, reverse=True)
        pk[p] = (len(hs.get(p, ())), len(hi.get(p, ())), found[:6])
    ranked = sorted(pk.items(), key=lambda kv: (-(kv[1][0] * 2 + kv[1][1]), kv[0]))[:12]
    ev = {"unit": u["i"], "stratum": u["stratum"], "bytes": u["bytes"], "strings": len(strs), "idents": len(ids),
          "packages": [[p, a, b, ex] for p, (a, b, ex) in ranked]}
    open(f"{neg}/evidence/{u['i']}.json", "w").write(json.dumps(ev, indent=1))
    return u["i"]


def main():
    neg, bundle, ref = sys.argv[1:4]
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    src = open(bundle, encoding="utf8").read()
    os.makedirs(f"{neg}/units", exist_ok=True)
    os.makedirs(f"{neg}/evidence", exist_ok=True)
    sample = [json.loads(line) for line in open(f"{neg}/data/sample.jsonl")]
    with ThreadPoolExecutor(jobs) as ex:
        n = sum(1 for _ in ex.map(one, [(neg, src, ref, u) for u in sample]))
    print(f"evidence for {n} units -> {neg}/evidence/")


if __name__ == "__main__":
    main()
