"""exp118: delete the work dirs (npm install + slice builds) of every FINISHED build chunk, while build_chunks.py runs.

Why: a new version's work dir holds ~41 MB (node_modules ~23, slice builds ~18); 33.8k versions would need ~1.4 TB
and /work had 716 GB free. A chunk process writes every shard of its versions to the store before it exits, each
version belongs to exactly one chunk (round-robin pins), and the merge (`--merge-only` / the merge step) reads only
the store, so a finished chunk's work dirs are never read again. Runs until build.out says BUILD DONE.

usage: python3 -I prune_work.py <data dir> <index name> <build.out>
"""
import os
import shutil
import sys
import time


def main():
    data, name, out = sys.argv[1:4]
    ld = os.path.join(data, "logs", f"chunks-{name}")
    tsv = os.path.join(data, "logs", f"chunks-{name}.tsv")
    work = os.path.join(data, "work")
    pruned = set()
    freed = 0
    while True:
        done = os.path.exists(out) and "BUILD DONE" in open(out, errors="replace").read()
        finished = set()
        if os.path.exists(tsv):
            for line in open(tsv).read().splitlines()[1:]:
                f = line.split("\t")
                if len(f) >= 2 and f[0].isdigit():
                    finished.add(int(f[0]))
        for i in sorted(finished - pruned):
            for spec in open(os.path.join(ld, f"{i:04d}.pins")).read().split():
                n, _, v = spec.rpartition("@")
                d = os.path.join(work, f"{n.replace('/', '__')}@{v}")
                if os.path.isdir(d):
                    shutil.rmtree(d, ignore_errors=True)
                    freed += 1
            pruned.add(i)
            print(f"pruned chunk {i} (dirs removed so far {freed})", flush=True)
        if done:
            print("build done; pruner exits", flush=True)
            return
        time.sleep(60)


if __name__ == "__main__":
    main()
