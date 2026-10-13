"""Fetch registry snapshots (humanify-index's own packument format: name, fetchedAt, time, versions) for a list of
packages, serially and only when missing (lane "indexv2"). The builder's --dry-run fetches the same way but stops at
the first package npm does not know; this skips it and says so.

usage: python3 -I packuments.py <registry dir> <packages.txt> [--workers 4]
"""
import json, os, subprocess, sys, time, datetime
from concurrent.futures import ThreadPoolExecutor


def main():
    reg, names = sys.argv[1], [l.strip() for l in open(sys.argv[2]) if l.strip()]
    workers = int(sys.argv[sys.argv.index("--workers") + 1]) if "--workers" in sys.argv else 4
    os.makedirs(reg, exist_ok=True)
    todo = [n for n in names if not os.path.exists(os.path.join(reg, n.replace("/", "__") + ".json"))]
    with ThreadPoolExecutor(workers) as ex:
        got = list(ex.map(lambda n: one(reg, n), todo))
    print("fetched", got.count(True), "missing", got.count(False))


def one(reg, n):
        path = os.path.join(reg, n.replace("/", "__") + ".json")
        for attempt in range(3):
            r = subprocess.run(["npm", "view", n, "time", "versions", "--json"], capture_output=True, text=True)
            if r.returncode == 0 or "E404" in r.stderr:
                break
            time.sleep(5 * (attempt + 1))  # back off on a registry error (429 / 5xx)
        if r.returncode != 0:
            print("no packument", n, r.stderr.strip().splitlines()[-1:] if r.stderr else "", flush=True)
            return False
        v = json.loads(r.stdout)
        vs = v.get("versions")
        doc = {"name": n, "fetchedAt": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z",
               "time": {k: t for k, t in (v.get("time") or {}).items() if isinstance(t, str)},
               "versions": [vs] if isinstance(vs, str) else (vs or [])}
        open(path + ".tmp", "w").write(json.dumps(doc, indent=2) + "\n")
        os.replace(path + ".tmp", path)
        return True


if __name__ == "__main__":
    main()
