"""exp112: scorer 7 with its two rules switched OFF must reproduce scorer 6's committed cards (scorer 7 = 6 +
exp112's origin-layer rules), and the scorer-6 reading of a label scorer 7 re-scored.

usage: <venv python> -I verify_s6.py <label> [--corpus dev|bundlers|apps|pop|test] [--show]
Runs score.py's own main() with origins.RULES7 all off and scorer 6's schema string into a scratch dir. A dev-side
label with committed scorer-6 cards: every card and the summary must equal them. --show prints the per-set
P / R / F0.5 / wrong-family bytes of that scorer-6 reading (the `before` column of a re-score; for a test label only
its per-set summary, never a card)."""
import contextlib
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.normpath(os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, SCORE)
import origins  # noqa: E402
import score  # noqa: E402


def main():
    argv = score.lib096.corpus_flag(sys.argv)
    label = argv[1]
    test = score.lib096.CORPUS in score.lib096.PRIVATE
    out = tempfile.mkdtemp(prefix="exp112-s6-")
    score.HERE = out
    score.SCORER_VERSION = "exp096-score/6"
    for k in origins.RULES7:
        origins.RULES7[k] = False
    sys.argv = ["score.py", label, "--corpus", score.lib096.CORPUS]
    with contextlib.redirect_stdout(open(os.devnull, "w")):
        score.main()
    got = json.load(open(os.path.join(out, "results", label, "summary.json")))
    if "--show" in argv:
        for s, r in sorted(got["sets"].items()):
            t = r.get("totals", r)
            print(f"{s:14s} P {t['precision']:.4f} R {t['recall']:.4f} F0.5 {t['f05']:.4f} "
                  f"wrong {t['wrongFamilyBytes']:,} app {t['appClaimedBytes']}")
    committed = os.path.join(SCORE, "results", label)
    if test or not os.path.exists(os.path.join(committed, "cards")):
        return
    s6 = json.load(open(os.path.join(committed, "summary.json")))
    if s6["schema"] != "exp096-score/6":
        print(f"{label}: committed cards are {s6['schema']}, not scorer 6's (not compared)")
        return
    bad = 0
    names = sorted(os.listdir(os.path.join(committed, "cards")))
    for n in names:
        if json.load(open(os.path.join(committed, "cards", n))) != json.load(open(os.path.join(out, "results", label,
                                                                                              "cards", n))):
            bad += 1
            print("DIFFERS", n)
    if s6 != got:
        bad += 1
        print("DIFFERS summary.json")
    print(f"{label}: {len(names)} cards, {bad} differ from scorer 6" + (" -- OK" if not bad else " -- FAIL"))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
