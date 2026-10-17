"""exp128: scorer 10's statement scope IS scorer 9's whole card. For each label, the scorer-10 summary's
sets[*].scopes.statement against the scorer-9 summary committed at <ref> (default origin/main): every byte count and
share must be equal. Exit 1 on any difference.

usage: python3 -I verify10.py [--ref REF] <label>...|--all"""
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


def main():
    argv = sys.argv[1:]
    ref = "origin/main"
    if "--ref" in argv:
        i = argv.index("--ref")
        ref = argv[i + 1]
        del argv[i:i + 2]
    labels = sorted(os.listdir(os.path.join(REPO, RESULTS))) if argv == ["--all"] else argv
    bad = checked = 0
    for label in labels:
        new = json.load(open(os.path.join(REPO, RESULTS, label, "summary.json")))
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
