"""exp107: scorer 6 with its three rules switched OFF must reproduce scorer 5's committed cards (scorer 6 = 5 +
exp107's decoder rules).

usage: <venv python> -I verify_s5.py <label> [--corpus dev|bundlers|apps]
Runs score.py's own main() with truth_audit.RULES6 all off and scorer 5's schema string into a scratch dir, and
compares every card and the summary with the committed scorer-5 files of experiments/096-library-matching-score/
results/<label> (a dev-side label: the test split's cards are never committed nor read)."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, SCORE)
import score  # noqa: E402


def main():
    argv = score.lib096.corpus_flag(sys.argv)
    label = argv[1]
    s4 = os.path.join(SCORE, "results", label)
    if json.load(open(os.path.join(s4, "summary.json")))["schema"] != "exp096-score/5":
        raise SystemExit(f"REFUSED: {label}'s committed cards are not scorer 5's")
    out = tempfile.mkdtemp(prefix="exp107-s5-")
    score.HERE = out
    score.SCORER_VERSION = "exp096-score/5"
    for k in score.truth_audit.RULES6:
        score.truth_audit.RULES6[k] = False
    sys.argv = ["score.py", label, "--corpus", score.lib096.CORPUS]
    import contextlib
    with contextlib.redirect_stdout(open(os.devnull, "w")):
        score.main()
    bad = 0
    names = sorted(os.listdir(os.path.join(s4, "cards")))
    for n in names:
        if json.load(open(os.path.join(s4, "cards", n))) != json.load(open(os.path.join(out, "results", label,
                                                                                         "cards", n))):
            bad += 1
            print("DIFFERS", n)
    if json.load(open(os.path.join(s4, "summary.json"))) != json.load(open(os.path.join(out, "results", label,
                                                                                        "summary.json"))):
        bad += 1
        print("DIFFERS summary.json")
    print(f"{label}: {len(names)} cards, {bad} differ from scorer 5" + (" -- OK" if not bad else " -- FAIL"))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
