"""exp101 step 1: how the HELD-OUT apps' "a — not indexed" / "b — version missing" miss bytes split, in AGGREGATE.

usage: <exp096 venv python> -I miss_buckets.py <label> <out.json> [<other label>...]
Re-runs exp096's score_map on the label's held-out reports with decompose.classify wrapped, so every missed byte
keeps its (class, package, version). It prints and writes ONLY aggregates: byte totals per bucket and package
COUNTS — never a held-out package name or version (the held-out identities must not become a selection list).

Buckets for "a" (package absent from the index):
  corpus       the package is in the dependency closure of some TRAINING app of the exp095 selection corpus
               (the 271 apps that are not held out) — a bigger budget on the same corpus could reach it
  npm rank     its position in npm-high-impact's download list (the selection lane's npm-top.json, 15,916 names)
For "b" (package present, version absent): the missing version's publish date vs the index's as-of date and the
year-all+minor policy (inside the last 12 months / latest patch of an older minor / an older patch / after as-of).
"""
import collections
import datetime
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import decompose  # noqa: E402

SEL = "/work/exp095-library-fingerprints/selection"
REGISTRIES = ["/work/exp095-index-v2-data/registry", f"{SEL}/registry", "/work/exp101-data/registry"]
ASOF = datetime.datetime(2026, 10, 11, tzinfo=datetime.timezone.utc)


def training_closure():
    held = set(json.load(open(f"{SEL}/selection.json"))["heldout"])
    pk = collections.Counter()
    napps = 0
    for slug in sorted(os.listdir(f"{SEL}/apps")):
        p = f"{SEL}/apps/{slug}/closure.json"
        if not os.path.exists(p):
            continue
        c = json.load(open(p))
        if c["id"] in held:
            continue
        napps += 1
        for name in c.get("packages", {}):
            pk[name] += 1
    return pk, napps


def packument(name):
    fn = name.replace("/", "__") + ".json"
    for r in REGISTRIES:
        p = os.path.join(r, fn)
        if os.path.exists(p):
            try:
                return json.load(open(p))
            except ValueError:
                return None
    return None


def parse_t(s):
    return datetime.datetime.fromisoformat(s.replace("Z", "+00:00"))


def version_bucket(name, version):
    doc = packument(name)
    if not doc or "time" not in doc or version not in doc["time"]:
        return "no-packument"
    t = parse_t(doc["time"][version])
    if t > ASOF:
        return "after-as-of"
    if (ASOF - t).days <= 365:
        return "within-12-months"
    parts = version.split("-")[0].split(".")
    if "-" in version:
        return "prerelease"
    minor = ".".join(parts[:2])
    same = [v for v in doc["time"] if v not in ("created", "modified") and "-" not in v
            and ".".join(v.split(".")[:2]) == minor and parse_t(doc["time"][v]) <= ASOF]

    def key(v):
        try:
            return tuple(int(x) for x in v.split("."))
        except ValueError:
            return (0,)
    return "latest-patch-of-older-minor" if same and max(same, key=key) == version else "older-patch"


def rank_bucket(r):
    if r is None:
        return "unranked"
    for cut in (500, 1000, 2000, 5000, 10000, 16000):
        if r < cut:
            return f"<{cut}"
    return "unranked"


def run(label):
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    om = lib096.load_origins_manifest()
    cm = score.truth_audit.load_copies_manifest()
    captured = []
    orig = decompose.classify

    def wrapped(ix, u, t, claim, vetoed, struct_claims, rule):
        c = orig(ix, u, t, claim, vetoed, struct_claims, rule)
        captured.append((c, t["pkg"], t.get("version"), u["b"]))
        return c
    decompose.classify = wrapped
    ixs = {}
    lib = 0
    per_app_a = []
    for entry in lib096.load_manifest()["entries"]:
        if entry["set"] != "heldout":
            continue
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        rep = doc["libraryDiscovery"]
        if rep is None:
            continue
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipeline["indexDir"], sl, pipeline["indexId"])
        n0 = len(captured)
        truth = lib096.load_truth(entry)
        body = score.score_map(entry, rep, truth, ixs[sl], pipeline["rule"],
                               lib096.load_origins(entry, om), score.truth_audit.Audit(entry, truth, cm))
        lib += body["totals"]["libraryBytes"]
        per_app_a.append(len({p for c, p, _, _ in captured[n0:] if c == "a"}))
    decompose.classify = orig
    return captured, lib, ixs


def main():
    label, out = sys.argv[1], sys.argv[2]
    captured, lib, ixs = run(label)
    train, napps = training_closure()
    top = json.load(open(f"{SEL}/npm-top.json"))
    rank = {n: i for i, n in enumerate(top)}
    greedy = {n: i for i, n in enumerate(json.load(open(f"{SEL}/selection.json"))["greedy"])}
    a_pkg = collections.Counter()
    b_ver = collections.Counter()
    cls = collections.Counter()
    for c, p, v, b in captured:
        cls[c] += b
        if c == "a":
            a_pkg[p] += b
        elif c == "b":
            b_ver[(p, v)] += b
    A = sum(a_pkg.values())
    res = {"label": label, "libraryBytes": lib, "trainingApps": napps,
           "missBytes": dict(sorted(cls.items())),
           "a": {"bytes": A, "packages": len(a_pkg)}, "b": {"bytes": sum(b_ver.values()), "versions": len(b_ver),
                                                           "packages": len({p for p, _ in b_ver})}}
    by = collections.defaultdict(lambda: [0, 0])
    for p, b in a_pkg.items():
        k = ("in-training-closure(>=2 apps)" if train[p] >= 2 else
             "in-training-closure(1 app)" if train[p] == 1 else "not-in-training-closure")
        by[k][0] += b
        by[k][1] += 1
    res["a"]["byTrainingClosure"] = {k: {"bytes": v[0], "packages": v[1]} for k, v in sorted(by.items())}
    by = collections.defaultdict(lambda: [0, 0])
    for p, b in a_pkg.items():
        k = rank_bucket(rank.get(p))
        by[k][0] += b
        by[k][1] += 1
    res["a"]["byNpmDownloadRank"] = {k: {"bytes": v[0], "packages": v[1]} for k, v in sorted(by.items())}
    by = collections.defaultdict(lambda: [0, 0])
    for p, b in a_pkg.items():
        g = greedy.get(p)
        k = "not-in-greedy-list" if g is None else ("greedy<3000" if g < 3000 else "greedy>=3000")
        by[k][0] += b
        by[k][1] += 1
    res["a"]["byGreedyPosition"] = {k: {"bytes": v[0], "packages": v[1]} for k, v in sorted(by.items())}
    # cross: closure x rank
    by = collections.defaultdict(lambda: [0, 0])
    for p, b in a_pkg.items():
        k = ("closure" if train[p] else "noclosure") + "/" + ("top16k" if p in rank else "unranked")
        by[k][0] += b
        by[k][1] += 1
    res["a"]["closureXrank"] = {k: {"bytes": v[0], "packages": v[1]} for k, v in sorted(by.items())}
    sizes = sorted(a_pkg.values(), reverse=True)
    res["a"]["concentration"] = {f"top{n}": round(sum(sizes[:n]) / A, 4) for n in (1, 5, 10, 25, 50, 100) if A}
    by = collections.defaultdict(lambda: [0, 0])
    for (p, v), b in b_ver.items():
        k = version_bucket(p, v)
        by[k][0] += b
        by[k][1] += 1
    res["b"]["byPublishDate"] = {k: {"bytes": v[0], "versions": v[1]} for k, v in sorted(by.items())}
    open(out, "w").write(json.dumps(res, indent=1, sort_keys=True) + "\n")
    print(json.dumps(res, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
