"""exp112 scorer 7: the packages a published package says it BUNDLED, read from its own tarball's third-party
license collection: `licenses/<pkg>/LICENSE*` style directories (tsdown / rollup-plugin-license / vite's
`LICENSE.md` sections `## <pkg>`). Cached per (package, version) for the `chunk` rule (origins7.py).

usage: python3 -I bundled.py <owners.json> [--jobs N]     owners.json: [[package, version|null], ...]
writes BUNDLED/<package>@<version>.json = {"bundled": [...]} ({"error": ...} when npm pack fails)."""
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
from concurrent.futures import ThreadPoolExecutor

BUNDLED = "/work/exp112-data/bundled"
LIC_DIR = re.compile(r"^package/(?:.*/)?(?:licenses|third[_-]party[_-]licenses|LICENSES)/((?:@[^/]+/)?[^/]+)/[^/]+$",
                     re.I)
HEADING = re.compile(r"^#{2,3} ((?:@[\w.-]+/)?[\w.-]+)\s*$", re.M)
LIC_MD = re.compile(r"^package/(?:LICENSE|THIRD[_-]PARTY[_-]LICENSES?|LICENSES)(?:\.md)?$", re.I)


def path(pkg, ver):
    return os.path.join(BUNDLED, pkg.replace("/", "__") + "@" + str(ver or "latest") + ".json")


def one(job):
    pkg, ver = job
    p = path(pkg, ver)
    if os.path.exists(p):
        return
    with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR")) as td:
        r = subprocess.run(["npm", "pack", f"{pkg}@{ver or 'latest'}", "--pack-destination", td, "--prefer-offline",
                            "--silent", "--ignore-scripts"], capture_output=True, text=True, timeout=300)
        tgz = [f for f in os.listdir(td) if f.endswith(".tgz")]
        if r.returncode or not tgz:
            json.dump({"error": (r.stderr or "npm pack failed")[-200:]}, open(p, "w"))
            return
        out = set()
        with tarfile.open(os.path.join(td, tgz[0])) as tf:
            for m in tf:
                mm = LIC_DIR.match(m.name)
                if mm:
                    out.add(mm.group(1))
                elif m.isfile() and LIC_MD.match(m.name) and m.size < 5_000_000:
                    out |= set(HEADING.findall(tf.extractfile(m).read().decode("utf8", "replace")))
        json.dump({"bundled": sorted(out)}, open(p, "w"))


def main():
    os.makedirs(BUNDLED, exist_ok=True)
    owners = json.load(open(sys.argv[1]))
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 8
    with ThreadPoolExecutor(jobs) as ex:
        list(ex.map(one, [tuple(o) for o in owners]))
    print(f"{len(owners)} owners -> {BUNDLED}")


if __name__ == "__main__":
    main()
