#!/usr/bin/env bash
# exp131: slim the build caches in place. Run under: flock /work/index-build.lock strip-full.sh <humanify-index> <out-dir> <work>...
# Refuses to start while any builder or bundler process runs (orphans under the deleted /work/exp109-data/work
# are ignored). df before/after, the slim report as JSON, progress in <out-dir>/slim.log.
set -euo pipefail
IX=$1; OUT=$2; shift 2
mkdir -p "$OUT"
python3 -I - "$@" <<'PY'
import os, sys
# this script's own ancestors (flock, bash) name the builder in their arguments: skip them
me, pid = set(), os.getpid()
while pid > 1:
    me.add(pid)
    pid = int(open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[1])
busy = []
for pid in filter(str.isdigit, os.listdir("/proc")):
    if int(pid) in me:
        continue
    try:
        argv = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
    except OSError:
        continue
    argv = [a.decode(errors="replace") for a in argv if a]
    if not argv or any("exp109-data/work" in a for a in argv):
        continue
    prog = os.path.basename(argv[0])
    script = os.path.basename(argv[1]) if len(argv) > 1 else ""
    if prog.startswith("humanify-index") or prog in ("esbuild", "bun") or \
            (prog == "node" and script in ("webpack-build.cjs", "rollup-build.mjs")):
        busy.append(f"{pid} {' '.join(argv)[:160]}")
if busy:
    print("REFUSED: builder or bundler processes are running:\n" + "\n".join(busy))
    sys.exit(2)
print("no builder or bundler process is running (exp109 orphans ignored)")
PY
df -B1 /work | tail -1 | tee "$OUT/df-before.txt"
date -u +"start %FT%TZ"
args=(); for w in "$@"; do args+=(--work "$w"); done
taskset -c 48-63 "$IX" slim-cache "${args[@]}" --journal "$OUT/journal" > "$OUT/slim.json" 2> "$OUT/slim.log"
cat "$OUT/slim.json"
date -u +"end %FT%TZ"
df -B1 /work | tail -1 | tee "$OUT/df-after.txt"
echo STRIP DONE
