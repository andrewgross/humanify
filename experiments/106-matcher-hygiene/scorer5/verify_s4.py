"""exp106: scorer 5 with its two source-table corrections switched OFF must reproduce scorer 4's committed cards.

usage: <venv python> -I verify_s4.py <label> <scorer-4 results dir (a copy of the label's results before re-scoring)>
Runs score.py's own main() with truth_audit.RULES5 all off and scorer 4's schema string, into a scratch dir, and
compares every card and the summary (minus scorer 5's new config sha) with the scorer-4 files."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import score  # noqa: E402


def main():
    label, s4 = sys.argv[1], sys.argv[2]
    out = tempfile.mkdtemp(prefix="exp106-s4-")
    score.HERE = out
    score.SCORER_VERSION = "exp096-score/4"
    for k in score.truth_audit.RULES5:
        score.truth_audit.RULES5[k] = False
    sys.argv = ["score.py", label]
    score.main()
    bad = 0
    names = sorted(os.listdir(os.path.join(s4, label, "cards")))
    for n in names:
        a = json.load(open(os.path.join(s4, label, "cards", n)))
        b = json.load(open(os.path.join(out, "results", label, "cards", n)))
        if a != b:
            bad += 1
            print("DIFFERS", n)
    sa = json.load(open(os.path.join(s4, label, "summary.json")))
    sb = json.load(open(os.path.join(out, "results", label, "summary.json")))
    sb["config"].pop("chainedSha256", None)
    sa["config"].pop("familiesSha256", None)
    sb["config"].pop("familiesSha256", None)
    if sa != sb:
        bad += 1
        print("DIFFERS summary.json")
    print(f"{label}: {len(names)} cards, {bad} differ from scorer 4" + (" -- OK" if not bad else " -- FAIL"))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
