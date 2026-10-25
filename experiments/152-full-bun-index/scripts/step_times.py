"""exp152: write an index's build.json sidecar (NOT in the manifest: the index id is the manifest's sha256, and
build-machine times there would give identical indexes different ids). It holds the wall time per step, tool
versions, the as-of, versions built / reused / failed by reason, and the pin list's sha256.

Per-step split of the shard phase (installs, bundles and fingerprinting overlap across 7 processes x 4 jobs):
  install     = the npm calls' wall time (timed.sh lines `npm`, by install dir; a retry counts)
  bundle      = the Bun build calls' wall time (lines `bun` run in an install dir)
  fingerprint = per version, its span (install dir's package.json written -> its last shard written) minus both:
                fingerprinting, the edge and literal tables, entries, map-comment stripping and shard writes.
Each is summed over versions as task-seconds; the shard phase's wall time is split by those shares.

usage: python3 -I step_times.py <data-dir> <name> <index dir> <pins.txt> <humanify-index> <bun> <out build.json>"""
import collections
import datetime
import glob
import hashlib
import json
import os
import re
import subprocess
import sys

SLICES = ("bun-bun-cjs-min", "bun-node-cjs-min", "literals")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def tool(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=60).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return None


def shard_notes(path):
    """The shard's `notes` object: the last key of the file, so only its tail is read."""
    with open(path, "rb") as f:
        f.seek(max(0, os.path.getsize(path) - 65536))
        tail = f.read().decode("utf-8", "replace")
    i = tail.rfind('"notes":')
    try:
        return json.JSONDecoder().raw_decode(tail[i + 8:])[0] if i >= 0 else {}
    except ValueError:
        return {}


def failure_reason(msg):
    m = re.search(r"npm error code (\w+)", msg)
    if m:
        return f"install {m[1]}"
    if "timed out" in msg.lower():
        return "timeout"
    return msg.split(":")[0][:60] if msg else "unknown"


def per_version_times(data, work):
    calls = collections.defaultdict(lambda: [0.0, 0.0, 0, 0])  # cwd -> install s, bundle s, npm calls, bun calls
    for line in open(f"{data}/logs/timing.log", errors="replace"):
        f = line.split(" ", 4)
        if len(f) < 5:
            continue
        cwd = f[4].rstrip("\n")
        if not cwd.startswith(work + "/"):
            continue  # `bun --version` from the chunk process
        key = os.path.relpath(cwd, work).split("/")[0]
        d = float(f[2]) - float(f[1])
        c = calls[key]
        if f[0] == "npm":
            c[0] += d
            c[2] += 1
        else:
            c[1] += d
            c[3] += 1
    return calls


def main():
    data, name, ix, pins, binary, bun, out = sys.argv[1:8]
    work, store = f"{data}/work", f"{data}/store"
    pinned = [l.strip() for l in open(pins) if l.strip()]
    calls = per_version_times(data, work)
    steps = collections.Counter()
    built, failed, reused = 0, collections.Counter(), 0
    logs = sorted(glob.glob(f"{data}/logs/chunks-{name}/*.log"))
    building, fail_msgs = set(), {}
    for p in logs:
        for line in open(p, errors="replace"):
            if line.startswith("building "):
                building.add(line.split(" ", 1)[1].strip())
            elif line.startswith("FAILED "):
                spec, msg = line[7:].split(": ", 1)
                fail_msgs[spec] = msg.strip()
    bundles_failed = collections.Counter()
    for spec in pinned:
        n, v = spec.rsplit("@", 1)
        key = f"{n.replace('/', '__')}@{v}"
        shards = [f"{store}/shards/{s}/{key}.json" for s in SLICES]
        have = [s for s in shards if os.path.exists(s)]
        if spec in fail_msgs:
            failed[failure_reason(fail_msgs[spec])] += 1
        elif spec not in building:
            reused += len(have) == len(shards)
            if len(have) < len(shards):
                failed["no shard, never built"] += 1
        else:
            built += 1
            for s in have[:2]:
                b = shard_notes(s).get("builds", {})
                if b and all(x != "ok" for x in b.values()):
                    bundles_failed[os.path.basename(os.path.dirname(s))] += 1
        c = calls.get(key)
        if c is None:
            continue
        steps["install"] += c[0]
        steps["bundle"] += c[1]
        start = f"{work}/{key}/package.json"
        if have and os.path.exists(start):
            span = max(os.path.getmtime(s) for s in have) - os.path.getmtime(start)
            steps["fingerprint"] += max(0.0, span - c[0] - c[1])
    merge = {}
    for line in open(f"{data}/logs/merge-{name}.log", errors="replace"):
        m = re.match(r"^(shard seconds|merge seconds|merge peak rss gb|merge exit) (\S+)", line)
        if m:
            merge[m[1]] = float(m[2])
    fetch = re.search(r"registry fetch start (\S+) end (\S+)", open(f"{data}/logs/registry-fetch.log").read())
    waits = [float(m) for m in re.findall(r"waited (\d+)s", open(f"{data}/logs/chunks-{name}/waves.log").read())]
    shard_wall = merge["shard seconds"]
    total = sum(steps.values()) or 1
    manifest = json.load(open(f"{ix}/manifest.json"))
    tsv = [l.rstrip("\n").split("\t") for l in open(f"{data}/logs/chunks-{name}.tsv")][1:]
    doc = {
        "index": hashlib.sha256(open(f"{ix}/manifest.json", "rb").read()).hexdigest(),
        "format": manifest["format"],
        "asOf": manifest["asOf"],
        "pins": {"file": os.path.basename(pins), "count": len(pinned), "sha256": sha256(pins)},
        "builtAt": datetime.datetime.fromtimestamp(os.path.getmtime(f"{ix}/manifest.json"), datetime.timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
        "wallSeconds": {
            "registryFetch": round(float(fetch[2]) - float(fetch[1]), 1),
            "shardPhase": shard_wall,
            "shardPhaseLockWait": sum(waits),
            "install": round(shard_wall * steps["install"] / total, 1),
            "bundle": round(shard_wall * steps["bundle"] / total, 1),
            "fingerprint": round(shard_wall * steps["fingerprint"] / total, 1),
            "merge": merge.get("merge seconds"),
        },
        "wallSecondsNote": "install/bundle/fingerprint split the shard phase (lock waits included) by task-second "
                           "share; the phases overlap across 7 processes x 4 jobs on 30 cores",
        "taskSeconds": {k: round(v, 1) for k, v in steps.items()},
        "resources": {"cores": "34-63 (30)", "processes": 7, "jobsPerProcess": 4, "chunkVersions": 60,
                      "chunks": len(tsv), "chunkPeakRssGbMax": max(float(r[3]) for r in tsv),
                      "mergePeakRssGb": merge.get("merge peak rss gb"),
                      "chunkExitNonZero": sum(1 for r in tsv if r[1] != "0")},
        "tools": {"humanify-index": {"commit": tool(["git", "-C", os.path.dirname(binary), "rev-parse", "HEAD"]),
                                     "sha256": sha256(binary)},
                  "bun": tool([bun, "--version"]), "npm": tool(["npm", "--version"]),
                  "node": tool(["node", "--version"]), "rustc": tool(["rustc", "--version"])},
        "recipes": [s["recipe"] for s in manifest["slices"]],
        "spongeRegions": "origin",
        "versions": {
            "pinned": len(pinned),
            "built": built,
            "reused": reused,
            "failed": sum(failed.values()),
            "failedByReason": dict(failed.most_common()),
            "builtButEveryBundleFailed": dict(bundles_failed),
            "indexed": len(manifest["packages"]),
            "refusedByReason": dict(collections.Counter(r["reason"] for r in manifest["refused"]).most_common()),
            "ownerVersions": sum(len(v) for v in json.load(open(f"{ix}/versions.json")).values()),
        },
    }
    with open(out, "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")
    print(json.dumps(doc, indent=2))


if __name__ == "__main__":
    main()
