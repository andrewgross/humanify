"""exp148: the declared dependencies (dependencies + peerDependencies + optionalDependencies + bundleDependencies,
union over every version published on or before the as-of) of each named package, from registry.npmjs.org.
Writes <dir>/<name with / -> __>.json = {name, deps: [...]}; existing files kept.

usage: python3 -I fetch_deps.py <names.txt> <dir> [threads]"""
import concurrent.futures
import datetime
import json
import os
import sys
import urllib.parse
import urllib.request

AS_OF = datetime.datetime(2026, 10, 15, 23, 59, 59, 999000, tzinfo=datetime.timezone.utc)
KEYS = ("dependencies", "peerDependencies", "optionalDependencies")


def fetch(name, d):
    path = os.path.join(d, name.replace("/", "__") + ".json")
    if os.path.exists(path):
        return "kept"
    url = "https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@")
    try:
        with urllib.request.urlopen(url, timeout=120) as r:
            doc = json.load(r)
    except Exception as e:  # noqa: BLE001 - network: reported
        return "error " + str(e)[:60]
    times = doc.get("time", {})
    deps = set()
    for v, m in (doc.get("versions") or {}).items():
        t = times.get(v)
        if not t or datetime.datetime.fromisoformat(t.replace("Z", "+00:00")) > AS_OF:
            continue
        for k in KEYS:
            deps |= set((m.get(k) or {}).keys())
        b = m.get("bundleDependencies") or m.get("bundledDependencies") or []
        deps |= set(b if isinstance(b, list) else [])
    json.dump({"name": name, "deps": sorted(deps)}, open(path, "w"))
    return "fetched"


def main():
    names = sorted({l.strip() for l in open(sys.argv[1]) if l.strip()})
    d = sys.argv[2]
    with concurrent.futures.ThreadPoolExecutor(int(sys.argv[3]) if len(sys.argv) > 3 else 8) as ex:
        res = list(ex.map(lambda n: fetch(n, d), names))
    print({k: sum(1 for r in res if r.split()[0] == k) for k in {r.split()[0] for r in res}})


if __name__ == "__main__":
    main()
