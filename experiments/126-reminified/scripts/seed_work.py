"""exp126: seed a work dir for a full re-fingerprint of k2-all-r5 from the cached builds, each version's install
taken from the SAME work dir as its builds (exp124's lesson: a build whose map names dependency files only its own
install had loses those owners when the seeded install comes from another work dir).

usage: python3 -I seed_work.py <versions list> <work dir> [--jobs N]
  <versions list>: install dir names (<name with __>@<version>), e.g. `ls /work/exp124-data/full/work`
Per version: every FINISHED build (a `.recipe` stamp) of each slice/tag is taken from the first source holding it
(seed-work.sh's order) and hard-linked; node_modules is SYMLINKED from the source that provides the most of those
builds (ties: source order), its package.json / package-lock.json hard-linked from the same source. Prints counts:
versions seeded, versions whose builds come from more than one source (their minority builds keep a foreign install,
as in exp124), versions with no install.
"""
import collections
import os
import shutil
import sys
from concurrent.futures import ThreadPoolExecutor

SOURCES = ["/work/exp113-data/work", "/work/exp109-data/work", "/work/exp101-data/work",
           "/work/exp095-index-v2-data/selABC/work", "/work/exp105-data/work", "/work/exp124-data/full/work"]


def builds_of(src, d):
    out = {}
    root = os.path.join(src, d, "build")
    if not os.path.isdir(root):
        return out
    for sl in os.listdir(root):
        p = os.path.join(root, sl)
        if not os.path.isdir(p):
            continue
        for tag in os.listdir(p):
            b = os.path.join(p, tag)
            if os.path.isfile(os.path.join(b, ".recipe")):
                out[f"{sl}/{tag}"] = b
    return out


def one(args):
    d, work = args
    dst = os.path.join(work, d)
    if os.path.exists(os.path.join(dst, ".installed")):
        return "skip"
    installed = [s for s in SOURCES if os.path.isfile(os.path.join(s, d, ".installed"))]
    if not installed:
        return "noinstall"
    chosen = {}
    for s in SOURCES:
        for rel, b in builds_of(s, d).items():
            chosen.setdefault(rel, (s, b))
    votes = collections.Counter(s for s, _ in chosen.values() if s in installed)
    home = max(installed, key=lambda s: (votes[s], -SOURCES.index(s)))
    os.makedirs(os.path.join(dst, "build"), exist_ok=True)
    for f in ("package.json", "package-lock.json"):
        p = os.path.join(home, d, f)
        if os.path.exists(p):
            os.link(p, os.path.join(dst, f))
    nm = os.path.join(home, d, "node_modules")
    if os.path.exists(nm):
        os.symlink(nm, os.path.join(dst, "node_modules"))
    for rel, (s, b) in sorted(chosen.items()):
        t = os.path.join(dst, "build", rel)
        os.makedirs(os.path.dirname(t), exist_ok=True)
        shutil.copytree(b, t, copy_function=os.link)
    open(os.path.join(dst, ".installed"), "w").close()
    return "mixed" if len({s for s, _ in chosen.values()}) > 1 else "seeded"


def main():
    a = sys.argv[1:]
    jobs = int(a[a.index("--jobs") + 1]) if "--jobs" in a else 16
    lst, work = a[0], a[1]
    os.makedirs(work, exist_ok=True)
    names = [l.strip() for l in open(lst) if l.strip()]
    with ThreadPoolExecutor(jobs) as ex:
        res = collections.Counter(ex.map(one, [(d, work) for d in names], chunksize=64))
    print(dict(res))


if __name__ == "__main__":
    main()
