"""exp132 lab: score.py with scorer 13's rule switched (`--misattr13 on|off`), for like-for-like comparisons while
the layer is built. Same cards as score.py with that switch.

usage: <venv python> -I score_rules.py <label> --corpus C --misattr13 on|off"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import score  # noqa: E402
import truth_audit  # noqa: E402


if __name__ == "__main__":
    i = sys.argv.index("--misattr13")
    truth_audit.RULES13["misattributed"] = sys.argv[i + 1] == "on"
    del sys.argv[i:i + 2]
    score.main()
