"""exp133: exp118's prune_work.py plus a disk guard. Deletes the work dirs (npm install + slice builds) of every
FINISHED build chunk while build_chunks.py runs, and PAUSES the build (SIGSTOP to every `humanify-index build`
child of the build_chunks.py pid) while /work's free space is under a floor, resuming (SIGCONT) once it is 50 GB
over it. A finished chunk has written every shard to the store, each version is in exactly one chunk, and the merge
reads only the store, so a pruned dir is never read again by this build. (Measured: the builds are already slim —
main's builder writes no sourcesContent since exp131 — so a version's ~40 MB is its node_modules.) Runs until
build.out says BUILD DONE.

usage: python3 -I prune_work_guard.py <data dir> <index name> <build.out> <build_chunks pid> <pause below GB>
"""
import os
import shutil
import signal
import sys
import time


def free_gb(path):
    st = os.statvfs(path)
    return st.f_bavail * st.f_frsize / 1e9


def finished_chunks(tsv):
    out = []
    if os.path.exists(tsv):
        for line in open(tsv).read().splitlines()[1:]:
            f = line.split("\t")
            if len(f) >= 2 and f[0].isdigit():
                out.append(int(f[0]))
    return out


def prune_chunk(ld, work, i):
    n_rm = 0
    for spec in open(os.path.join(ld, f"{i:04d}.pins")).read().split():
        n, _, v = spec.rpartition("@")
        d = os.path.join(work, f"{n.replace('/', '__')}@{v}")
        if os.path.isdir(d):
            shutil.rmtree(d, ignore_errors=True)
            n_rm += 1
    return n_rm


def builders(parent):
    """pids of the build_chunks.py process's direct children (taskset execs humanify-index in place)."""
    out = []
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            stat = open(f"/proc/{p}/stat").read()
        except OSError:
            continue
        ppid = int(stat.rsplit(")", 1)[1].split()[1])
        if ppid == parent:
            out.append(int(p))
    return out


def signal_all(parent, sig):
    for pid in builders(parent):
        try:
            os.kill(pid, sig)
        except OSError:
            pass


def main():
    data, name, out, parent, floor = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), float(sys.argv[5])
    ld = os.path.join(data, "logs", f"chunks-{name}")
    tsv = os.path.join(data, "logs", f"chunks-{name}.tsv")
    work = os.path.join(data, "work")
    pruned, paused = set(), False
    while True:
        done = os.path.exists(out) and "BUILD DONE" in open(out, errors="replace").read()
        for i in finished_chunks(tsv):
            if i not in pruned:
                n_rm = prune_chunk(ld, work, i)
                pruned.add(i)
                print(f"pruned chunk {i} ({n_rm} dirs), free {free_gb(work):.0f} GB", flush=True)
        free = free_gb(work)
        if not paused and free < floor:
            signal_all(parent, signal.SIGSTOP)
            paused = True
            print(f"PAUSED build: free {free:.0f} GB < {floor:.0f}", flush=True)
        elif paused and free > floor + 50:
            signal_all(parent, signal.SIGCONT)
            paused = False
            print(f"RESUMED build: free {free:.0f} GB", flush=True)
        if done:
            print(f"build done; pruner exits; {len(pruned)} chunks pruned", flush=True)
            return
        time.sleep(30)


if __name__ == "__main__":
    main()
