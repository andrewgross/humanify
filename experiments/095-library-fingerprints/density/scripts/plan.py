"""Experiment B step 1: every stable release of the chosen packages, as humanify-index package lists.

usage: python3 -I plan.py <packages.txt> [--groups 5]
  -> $DEN/versions.json {pkg: [{version, published, downloads}]}, $DEN/groups/g<k>.txt (`name@version` pins)

Versions: every version in the registry document that is plain semver x.y.z (no prerelease, no build tag) and has
a publish time. Each is passed to humanify-index as a pin, so every one is indexed (the as-of policy's own picks
are a subset). Per-version downloads (api.npmjs.org/versions/<pkg>/last-week, the registry's only per-version
count) weight the "what bundles in the wild use" averages. Groups are balanced by version count so parallel
humanify-index processes (4 jobs each) finish together; they share one shard store.
"""
import json, os, re, sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../selection/scripts"))
from net import get

DEN = os.environ.get("DEN", "/work/exp095-library-fingerprints/density")
SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")


def main():
    names = [l.strip() for l in open(sys.argv[1]) if l.strip() and not l.startswith("#")]
    G = int(sys.argv[sys.argv.index("--groups") + 1]) if "--groups" in sys.argv else 5
    out = {}
    for n in names:
        st, doc = get("https://registry.npmjs.org/" + n.replace("/", "%2F"))
        if st != 200 or not isinstance(doc, dict):
            print("no packument", n, st)
            continue
        st2, dls = get("https://api.npmjs.org/versions/" + n.replace("/", "%2F") + "/last-week", min_interval=1.0)
        if st2 != 200:
            print("WARNING: no per-version downloads for", n, st2, "(re-run: failures are not cached)")
        dls = (dls or {}).get("downloads", {}) if isinstance(dls, dict) else {}
        vs = []
        for v in doc.get("versions", {}):
            m = SEMVER.match(v)
            t = (doc.get("time") or {}).get(v)
            if m and t:
                vs.append({"version": v, "published": t, "downloads": dls.get(v, 0)})
        vs.sort(key=lambda x: tuple(int(p) for p in x["version"].split(".")))
        out[n] = vs
        print(n, len(vs), flush=True)
    json.dump(out, open(DEN + "/versions.json", "w"), indent=0)
    os.makedirs(DEN + "/groups", exist_ok=True)
    groups = [[] for _ in range(G)]
    load = [0] * G
    for n in sorted(out, key=lambda n: -len(out[n])):
        k = load.index(min(load))
        groups[k] += [f"{n}@{v['version']}" for v in out[n]]
        load[k] += len(out[n])
    for k, g in enumerate(groups):
        open(f"{DEN}/groups/g{k}.txt", "w").write("\n".join(g) + "\n")
    print("versions", sum(len(v) for v in out.values()), "groups", load)


if __name__ == "__main__":
    main()
