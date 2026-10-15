"""exp112: which published package versions carry the two pre-bundled-copy shapes the test wrong-family bytes
came from, read from the packages' OWN published files (npm tarballs), never from any scored set.

usage: python3 -I scan_pkgs.py <closures dir> <out.jsonl> [--min-bytes N] [--jobs J]
  <closures dir>: exp108's training installs (<slug>/closure.json, the packages each app's bundle pulled in)
For every package version some training app bundled at least --min-bytes of (default 150,000): npm pack it
(the shared npm cache, --prefer-offline), and over its .js/.mjs/.cjs files measure
  foreignRegionBytes  bytes inside rolldown/tsdown `//#region <path>` markers naming node_modules/<X>, X != itself
  lazyAfterBytes      bytes after such a region's `//#endregion` when the next line starts a lazy module
                      initialiser (`__esmMin(` / `__esm(`), up to the next marker or top-level `export`
  nestedRegionBytes   bytes of foreign regions whose marker path is a file of X's own build output
                      (node_modules/X/(dist|build|lib|out)/...): a copy of X's pre-bundled chunk
"""
import collections
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
from concurrent.futures import ThreadPoolExecutor

REGION = re.compile(r"^//#region (\S+)$")
END = re.compile(r"^//#endregion")
NM = re.compile(r"node_modules/(@[^/\s]+/[^/\s]+|[^/@\s][^/\s]*)/(\S*)")
LAZY = re.compile(r"^\s*(?:var \w+ = )?__esm(?:Min)?\(")
EXPORT = re.compile(r"^export ")
BUILD_DIR = re.compile(r"^(?:dist|build|lib|out|bundle)/")


def scan_text(text, self_name):
    st = collections.Counter()
    lines = text.split("\n")
    cur = None  # (pkg, nested?) of the open region
    after = None  # pkg whose lazy body follows its endregion
    for i, line in enumerate(lines):
        n = len(line) + 1
        m = REGION.match(line)
        if m:
            after = None
            path = m.group(1)
            pk = NM.match(path[path.rfind("node_modules/"):]) if "node_modules/" in path else None
            if pk and pk.group(1) != self_name:
                cur = (pk.group(1), bool(BUILD_DIR.match(pk.group(2))))
            else:
                cur = None
            continue
        if END.match(line):
            if cur is not None and i + 1 < len(lines) and LAZY.match(lines[i + 1]):
                after = cur[0]
            cur = None
            continue
        if after is not None and EXPORT.match(line):
            after = None
        if cur is not None:
            st["foreignRegionBytes"] += n
            if cur[1]:
                st["nestedRegionBytes"] += n
        elif after is not None:
            st["lazyAfterBytes"] += n
    return st


def one(job):
    name, ver = job
    with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as td:
        r = subprocess.run(["npm", "pack", f"{name}@{ver}", "--pack-destination", td, "--prefer-offline", "--silent",
                            "--ignore-scripts"], capture_output=True, text=True, timeout=300)
        tgz = [f for f in os.listdir(td) if f.endswith(".tgz")]
        if r.returncode or not tgz:
            return {"name": name, "version": ver, "error": (r.stderr or "")[-300:]}
        st = collections.Counter()
        with tarfile.open(os.path.join(td, tgz[0])) as tf:
            for m in tf:
                if not m.isfile() or not m.name.endswith((".js", ".mjs", ".cjs")) or m.size > 60_000_000:
                    continue
                b = tf.extractfile(m).read()
                if b"//#region" not in b:
                    continue
                st += scan_text(b.decode("utf8", "replace"), name)
        return {"name": name, "version": ver, **st}


def main():
    root, out = sys.argv[1:3]
    minb = int(sys.argv[sys.argv.index("--min-bytes") + 1]) if "--min-bytes" in sys.argv else 150_000
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    want = set()
    for d in sorted(os.listdir(root)):
        p = os.path.join(root, d, "closure.json")
        if not os.path.exists(p):
            continue
        c = json.load(open(p))
        for name, v in (c.get("packages") or {}).items():
            if v.get("bytes", 0) >= minb:
                want |= {(name, x) for x in str(v["version"]).split(",")}  # a closure lists every bundled copy
    done = set()
    if os.path.exists(out):
        done = {(r["name"], r["version"]) for r in map(json.loads, open(out)) if "error" not in r
                and not r.get("foreignRegionBytes")}
    todo = sorted(want - done)
    print(f"{len(want)} package versions, {len(todo)} to scan", flush=True)
    with open(out, "a") as fh, ThreadPoolExecutor(jobs) as ex:
        for r in ex.map(one, todo):
            fh.write(json.dumps(r) + "\n")
            fh.flush()


if __name__ == "__main__":
    main()
