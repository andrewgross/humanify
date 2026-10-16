"""exp118: public npm registry metadata for a list of package names (cached; re-runs fetch only what is missing).

  abbr  https://registry.npmjs.org/<name> with the install-v1 Accept header: every version's dependencies,
        optionalDependencies, peerDependencies (no devDependencies, no maintainers). Kept slim: per version the
        three dependency maps, plus dist-tags.
  ver   https://registry.npmjs.org/<name>/<version>: one version's full manifest. Kept slim: dependencies,
        devDependencies, peerDependencies, optionalDependencies, bundleDependencies, maintainers, _npmUser,
        repository.

usage: python3 -I fetch_npm.py <cache dir> abbr <names.txt> [jobs]
       python3 -I fetch_npm.py <cache dir> ver <name@version lines file> [jobs]
"""
import concurrent.futures
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

REG = "https://registry.npmjs.org/"
ABBR = "application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8"


def get(url, accept="application/json"):
    for i in range(6):
        try:
            req = urllib.request.Request(url, headers={"Accept": accept, "User-Agent": "humanify-exp118"})
            with urllib.request.urlopen(req, timeout=120) as r:
                return 200, json.load(r)
        except urllib.error.HTTPError as e:
            if e.code in (404, 405):
                return e.code, None
            time.sleep(3 * (i + 1))
        except Exception:  # noqa: BLE001
            time.sleep(3 * (i + 1))
    return 0, None


def enc(name):
    return name.replace("/", "%2F") if name.startswith("@") else urllib.parse.quote(name, safe="")


DEPS = ("dependencies", "optionalDependencies", "peerDependencies")


def slim_abbr(doc):
    return {"name": doc.get("name"), "dist-tags": doc.get("dist-tags", {}),
            "versions": {v: {k: m[k] for k in DEPS if m.get(k)} for v, m in (doc.get("versions") or {}).items()}}


def slim_ver(m):
    keys = DEPS + ("devDependencies", "bundleDependencies", "bundledDependencies", "maintainers", "_npmUser",
                   "repository", "name", "version")
    return {k: m[k] for k in keys if m.get(k)}


def one(cache, kind, item):
    fn = os.path.join(cache, kind, item.replace("/", "__") + ".json")
    if os.path.exists(fn):
        return 0
    if kind == "abbr":
        st, doc = get(REG + enc(item), ABBR)
        body = slim_abbr(doc) if doc else None
    else:
        name, _, ver = item.rpartition("@")
        st, doc = get(REG + enc(name) + "/" + urllib.parse.quote(ver, safe=""))
        body = slim_ver(doc) if doc else None
    with open(fn + ".tmp", "w") as f:
        json.dump({"status": st, "body": body}, f)
    os.replace(fn + ".tmp", fn)
    return 1


def main():
    cache, kind, lst = sys.argv[1], sys.argv[2], sys.argv[3]
    jobs = int(sys.argv[4]) if len(sys.argv) > 4 else 16
    os.makedirs(os.path.join(cache, kind), exist_ok=True)
    items = [x.strip() for x in open(lst) if x.strip()]
    done = 0
    with concurrent.futures.ThreadPoolExecutor(jobs) as ex:
        for n, r in enumerate(ex.map(lambda it: one(cache, kind, it), items)):
            done += r
            if n % 1000 == 0:
                print(kind, n, len(items), "fetched", done, flush=True)
    print(kind, "done", len(items), "fetched", done, flush=True)


if __name__ == "__main__":
    main()
