"""Run density build groups through a fixed set of CPU slots (each slot: one humanify-index process, 4 jobs, pinned
to 4 cores), so the lab never exceeds its core budget. A slot may start busy with an already-running PID.
usage: python3 -I queue.py --slot 44-47[:pid] --slot ... -- <group.txt>...
"""
import os, subprocess, sys, time

S = os.path.join(os.path.dirname(os.path.abspath(__file__)), "build-group.sh")
a = sys.argv[1:]
cut = a.index("--")
slots = [a[i + 1] for i in range(0, cut, 2) if a[i] == "--slot"]
queue = list(a[cut + 1:])
busy = {}
for s in slots:
    cpus, _, pid = s.partition(":")
    busy[cpus] = int(pid) if pid else None


def alive(x):
    if x is None:
        return False
    if isinstance(x, subprocess.Popen):
        return x.poll() is None
    try:
        os.kill(x, 0)
        return True
    except OSError:
        return False


while queue or any(alive(x) for x in busy.values()):
    for cpus, x in list(busy.items()):
        if not alive(x) and queue:
            g = queue.pop(0)
            print(time.strftime("%H:%M:%S"), "start", g, "on", cpus, flush=True)
            busy[cpus] = subprocess.Popen([S, g, cpus])
    time.sleep(15)
print("queue done", flush=True)
