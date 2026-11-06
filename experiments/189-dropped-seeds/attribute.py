"""exp189: attribute every byte of exp188's ">= 50% cover, unclaimed" bucket to the stage of the run that dropped it.

usage: <venv python> -I attribute.py <trail label> --corpus bun-dev [--json out.json]

The population is exp188's census (`census.unclaimed_units`: scored units the run left unclaimed whose structural
route WITHOUT a cover floor reaches >= 50% cover), read off the label's own reports. Each unit is joined, by span,
to the run's seed trail (`seedTrail`, written by the instrumented binary with `--unit-prints`), and charged to ONE
stage, the last that touched it in pipeline order:

  after the fixpoint     vetoed | settledOut | external | claimedOther | ownPackage | unclaimed
  presence rules         contextPresence | copyPresence | shadowTies | factoryTieFence | factoryWholeSeed
  lookup guards          evidenceConflict | helperFamilies | helperPrints | unsupported (position window)
  no trail row at all    noPick (the run's lookup made no pick: an emulation/run difference, read apart)

Bytes are the whole unit (what a fixed stage could claim), judged two ways: by the census's best package (exp188's
verdict, comparable with its table) and by the package the trail row carries (what the run itself would have
claimed). A PRIVATE corpus prints totals only."""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
S096 = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, S096)
sys.path.insert(0, os.path.join(HERE, "../188-partial-claims"))
import census  # noqa: E402  (unclaimed_units, verdict_of: exp188's census, one copy)
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

FIXPOINT = ("vetoed", "settledOut", "external", "claimedOther", "ownPackage", "unclaimed", "claimed")
PRESENCE = ("contextPresence", "copyPresence", "shadowTies", "factoryTieFence", "factoryWholeSeed")
LOOKUP = ("evidenceConflict", "helperFamilies", "helperPrints", "unsupported")
MOVED = ("copyPresenceMoved", "shadowTiesMoved")
ORDER = LOOKUP + MOVED + PRESENCE + FIXPOINT


def charge(rows):
    """The one stage a unit's trail rows charge (the last in pipeline order), and that row's package."""
    if not rows:
        return "noPick", None
    # a seed another seed on the unit beat is charged the winner's fate when the winner has a trail row (an index
    # seed); with none, the winner was a declared seed: the presence fence's, which a presence rule's row explains
    lost = [r for r in rows if r["stage"] == "settledOut"]
    rows = [r for r in rows if r["stage"] != "settledOut"] or lost
    best = max(rows, key=lambda r: ORDER.index(r["stage"]))
    return best["stage"], best["package"]


def main():
    argv = lib096.corpus_flag(sys.argv)
    label = argv[1]
    out_json = argv[argv.index("--json") + 1] if "--json" in argv else None
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    private = lib096.CORPUS in lib096.PRIVATE
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    indexes = {}
    # set -> stage -> verdict -> bytes (census verdict); same keyed by the trail package's verdict
    by_census = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    by_trail = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    packages = collections.defaultdict(collections.Counter)  # stage -> census package -> right bytes
    stage_rows = collections.Counter()  # every trail row by stage, whole corpus (does each stage fire?)
    examples = collections.defaultdict(list)
    for entry in lib096.load_manifest()["entries"]:
        doc = json.load(open(os.path.join(run_dir, "reports", entry["id"] + ".json")))
        report = doc["libraryDiscovery"]
        if report is None or entry["truth"]["method"] != "sourcemap":
            continue
        slice_ = doc["toolchain"]["slice"]
        if slice_ not in indexes:
            indexes[slice_] = score.load_index(pipeline["indexDir"], slice_, pipeline["indexId"])
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        trail = collections.defaultdict(list)
        for r in report.get("seedTrail", []):
            trail[(r["start"], r["end"])].append(r)
            stage_rows[r["stage"]] += 1
        labels = score.label_units(truth, score.run_units(report), origins, audit)
        for u, route, frac, v, _, _ in census.unclaimed_units(report, truth, indexes[slice_], origins, audit):
            if frac < 0.5:
                continue
            stage, pkg = charge(trail.get((u["s"], u["e"]), []))
            by_census[entry["set"]][stage][v] += u["b"]
            tv = census.verdict_of(score.judge_origin(pkg, labels[u["i"]], truth["appPackage"])[0]) if pkg else v
            by_trail[entry["set"]][stage][tv] += u["b"]
            if v == "right":
                packages[stage][route[0]] += u["b"]
            if not private and len(examples[stage]) < 5 and u["b"] > 20000:
                examples[stage].append([entry["id"], u["s"], u["e"], u["b"], route[0], pkg, v])
        if not private:
            print(f"  {entry['id']}", file=sys.stderr, flush=True)
    out = {"stageRows": dict(stage_rows), "sets": {}}
    for set_ in sorted(by_census):
        out["sets"][set_] = {
            "byCensusVerdict": {s: dict(c) for s, c in by_census[set_].items()},
            "byTrailVerdict": {s: dict(c) for s, c in by_trail[set_].items()},
        }
    if not private:
        out["topPackages"] = {s: c.most_common(8) for s, c in packages.items()}
        out["examples"] = examples
    text = json.dumps(out, indent=1, sort_keys=True)
    print(text)
    if out_json:
        open(out_json, "w").write(text + "\n")


if __name__ == "__main__":
    main()
