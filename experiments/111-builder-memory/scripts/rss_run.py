"""Run one humanify-index build and sample its RSS against the number of versions started.
usage: rss_run.py <bin> <label> <cpus>   (data under /work/exp111-data; NEW store/out per label)"""
import os, subprocess, sys, time
b, label, cpus = sys.argv[1:4]
D = "/work/exp111-data"; T = "/work/exp095-library-fingerprints/lab/tools"
open(f"{D}/empty.txt", "w").close()
for p in (f"{D}/store-{label}", f"{D}/ix-{label}"):
    if os.path.exists(p): raise SystemExit(f"REFUSED: {p} exists")
log = open(f"{D}/logs/{label}.log", "w")
cmd = ["taskset", "-c", cpus, b, "build", "--packages", f"{D}/empty.txt", "--pinned", os.environ.get("PINS", f"{D}/pins.txt"),
       "--as-of", "2026-10-11", "--registry", "/work/exp108-data/idx/registry", "--store", f"{D}/store-{label}",
       "--work", f"{D}/work", "--policy", "year-all+minor", "--jobs", os.environ.get("JOBS", "4"), "--offline", "--out", f"{D}/ix-{label}",
       "--recipe", "bun:target=bun,format=cjs,minify", "--recipe", "bun:target=node,format=cjs,minify",
       "--recipe", "esbuild:target=node,format=cjs,minify",
       "--bun", f"{T}/bun-1.4.0/node_modules/.bin/bun", "--esbuild", f"{T}/node_modules/esbuild/bin/esbuild"]
p = subprocess.Popen(cmd, stdout=log, stderr=log)
out = open(f"{D}/logs/{label}.rss.tsv", "w"); out.write("t\tbuilding\trss_mb\n")
t0 = time.time()
while p.poll() is None:
    try:
        rss = [l for l in open(f"/proc/{p.pid}/status") if l.startswith("VmRSS")]
        kb = int(rss[0].split()[1]) if rss else 0
    except OSError:
        kb = 0
    n = sum(1 for l in open(f"{D}/logs/{label}.log", errors="replace") if l.startswith("building "))
    out.write(f"{time.time()-t0:.0f}\t{n}\t{kb//1024}\n"); out.flush()
    time.sleep(2)
_, st, ru = os.wait4(p.pid, 0) if False else (0, p.returncode, None)
out.write(f"# exit {p.returncode} seconds {time.time()-t0:.0f}\n")
print("DONE", label, p.returncode)
