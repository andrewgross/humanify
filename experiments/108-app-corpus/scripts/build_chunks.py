"""exp108: build an index's missing shards in many SMALL processes, then merge with --merge-only.

Why: a `humanify-index build` process's RSS grows ~40 MB per package version it builds (measured on main 116090f2:
~33 GB per part after ~800 versions, 6 parts -> one part OOM-killed with 137). Short-lived processes cap it.
Run under the campaign lock:  flock /work/index-build.lock python3 -I build_chunks.py <args>

usage: build_chunks.py <data-dir> <name> <cpus> <procs> <humanify-index> <packages.txt> <pins.txt> <chunk-versions>
                       [--as-of YYYY-MM-DD] [--build-pins <pins.txt>] -- <recipe flags>
  --as-of       the index date (default 2026-10-11, every index before exp142)
  --build-pins  build shards only for these pins (a DELTA on a store that holds the rest: exp142); the merge still
                reads <packages.txt> + <pins.txt>
Writes <data-dir>/ix/<name> (refuses an existing one); logs in <data-dir>/logs/chunks-<name>/, a per-chunk
peak-RSS table in <data-dir>/logs/chunks-<name>.tsv, timings appended to <data-dir>/logs/merge-<name>.log.
"""
import os
import subprocess
import sys
import time

ASOF = "2026-10-11"


def main():
    a = sys.argv[1:]
    sep = a.index("--")
    data, name, cpus, procs, ix, pkgs, pins, chunk = a[:8]
    opts = dict(zip(a[8:sep:2], a[9:sep:2]))
    if sep < 8 or set(opts) - {"--as-of", "--build-pins"} or len(a[8:sep]) % 2:
        raise SystemExit(__doc__)
    as_of = opts.get("--as-of", ASOF)
    recipe = a[sep + 1:]
    procs, chunk = int(procs), int(chunk)
    out = f"{data}/ix/{name}"
    if os.path.exists(out):
        raise SystemExit(f"REFUSED: {out} exists (never overwrite an index)")
    common = ["--as-of", as_of, "--registry", f"{data}/registry", "--store", f"{data}/store", "--work", f"{data}/work",
              "--policy", "year-all+minor", *recipe]
    ld = f"{data}/logs/chunks-{name}"
    os.makedirs(ld, exist_ok=True)
    os.makedirs(f"{data}/ix", exist_ok=True)
    roots = [l.strip() for l in open(pkgs) if l.strip()]
    pinned = [l.strip() for l in open(opts.get("--build-pins", pins)) if l.strip()]
    n = max(1, (len(pinned) + chunk - 1) // chunk)
    chunks = []
    for i in range(n):  # round-robin: one package's versions spread over chunks, so no chunk is all one big family
        cp, cr = f"{ld}/{i:04d}.pins", f"{ld}/{i:04d}.roots"
        open(cp, "w").write("\n".join(pinned[i::n]) + "\n")
        open(cr, "w").write("\n".join(roots[i::n]) + "\n")
        chunks.append((i, cr, cp))
    t0 = time.time()
    running, tsv = {}, open(f"{data}/logs/chunks-{name}.tsv", "w")
    tsv.write("chunk\texit\tseconds\tpeak_rss_gb\tbuilt\n")
    todo = list(chunks)
    while todo or running:
        while todo and len(running) < procs:
            i, cr, cp = todo.pop(0)
            log = open(f"{ld}/{i:04d}.log", "w")
            p = subprocess.Popen(["taskset", "-c", cpus, ix, "build", "--packages", cr, "--pinned", cp, *common,
                                  "--jobs", "4", "--offline", "--out", f"{ld}/{i:04d}.ix"], stdout=log, stderr=log)
            running[p.pid] = (i, time.time(), p)
        pid, status, ru = os.wait4(-1, 0)
        if pid not in running:
            continue
        i, st, _ = running.pop(pid)
        built = sum(1 for l in open(f"{ld}/{i:04d}.log", errors="replace") if l.startswith("building "))
        tsv.write(f"{i}\t{os.waitstatus_to_exitcode(status)}\t{time.time() - st:.0f}\t{ru.ru_maxrss / 1e6:.2f}\t{built}\n")
        tsv.flush()
        subprocess.run(["rm", "-rf", f"{ld}/{i:04d}.ix"])
    shard_s = time.time() - t0
    with open(f"{data}/logs/merge-{name}.log", "w") as log:
        t1 = time.time()
        p = subprocess.Popen(["taskset", "-c", cpus, ix, "build", "--packages", pkgs, "--pinned", pins, *common,
                              "--jobs", "4", "--offline", "--merge-only", "--out", out], stdout=log, stderr=log)
        _, status, ru = os.wait4(p.pid, 0)
        log.write(f"shard seconds {shard_s:.0f}\nmerge seconds {time.time() - t1:.0f}\n"
                  f"merge peak rss gb {ru.ru_maxrss / 1e6:.2f}\nmerge exit {os.waitstatus_to_exitcode(status)}\n")
    print(open(f"{data}/logs/merge-{name}.log").read()[-400:])
    print("BUILD DONE", name, flush=True)


if __name__ == "__main__":
    main()
