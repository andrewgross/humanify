"""exp128: scorer 11's statement scope against scorer 10's whole card. For each label, the scorer-11 summary's
sets[*].scopes.statement against the scorer-10 summary committed at <ref> (default origin/main). They differ only
through rule `notinstalled` (its relabelled sources sit in esbuild sets); with --rules-off the label is re-scored in
a scratch dir with `notinstalled` OFF and must equal scorer 10 exactly. Exit 1 on any difference.

usage: python3 -I verify11.py [--ref REF] <label>...|--all
       <venv python> -I verify11.py --rules-off <scratch dir> --corpus C <label>   (the exact-identity check)"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../.."))
RESULTS = "experiments/096-library-matching-score/results"
KEYS = ("libraryBytes", "namedBytes", "appClaimedBytes", "wrongFamilyBytes", "namedAsOriginBytes", "copyBytes",
        "precision", "recall", "f1", "f05", "precisionShipperOnly", "recallShipperOnly", "f05ShipperOnly",
        "missShares")


def old_summary(ref, label):
    p = subprocess.run(["git", "show", f"{ref}:{RESULTS}/{label}/summary.json"], cwd=REPO, capture_output=True,
                       text=True)
    return json.loads(p.stdout) if p.returncode == 0 else None


def rules_off(argv):
    """Re-score one label with `notinstalled` off into a scratch dir; compare its statement scope with scorer 10."""
    sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
    import lib096
    argv = lib096.corpus_flag(argv)
    scratch, label = argv[argv.index("--rules-off") + 1], argv[-1]
    import score
    import truth_audit
    truth_audit.RULES11["notinstalled"] = False
    score.HERE = scratch
    sys.argv = ["score.py", label] + (["--corpus", lib096.CORPUS] if lib096.CORPUS != "dev" else [])
    score.main()
    new = json.load(open(os.path.join(scratch, "results", label, "summary.json")))
    old = old_summary("origin/main", label)
    bad = 0
    for s, row in old["sets"].items():
        st = new["sets"][s]["scopes"]["statement"]
        diff = [k for k in KEYS if k in row and row[k] != st[k]]
        bad += bool(diff)
        print(f"{'DIFF' if diff else 'SAME'} {label} {s} {diff}")
    sys.exit(1 if bad else 0)


def main():
    argv = sys.argv[1:]
    if "--rules-off" in argv:
        return rules_off(argv)
    ref = "origin/main"
    if "--ref" in argv:
        i = argv.index("--ref")
        ref = argv[i + 1]
        del argv[i:i + 2]
    labels = sorted(os.listdir(os.path.join(REPO, RESULTS))) if argv == ["--all"] else argv
    bad = checked = 0
    for label in labels:
        path = os.path.join(REPO, RESULTS, label, "summary.json")
        if not os.path.exists(path):
            continue
        new = json.load(open(path))
        if "/11" not in new["schema"]:
            print(f"{label}: not re-scored by scorer 11 yet, skipped")
            continue
        old = old_summary(ref, label)
        if old is None:
            print(f"{label}: not at {ref} (new label), skipped")
            continue
        for s, row in old["sets"].items():
            st = new["sets"][s]["scopes"]["statement"]
            diff = [k for k in KEYS if k in row and row[k] != st[k]]
            checked += 1
            if diff:
                bad += 1
                print(f"DIFF {label} {s}: {', '.join(f'{k} {row[k]} -> {st[k]}' for k in diff)}")
    print(f"{checked} label-sets checked, {bad} differ")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
