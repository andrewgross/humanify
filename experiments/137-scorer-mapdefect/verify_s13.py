"""exp137: scorer 14 with rule `mapdefect` OFF must reproduce scorer 13's cards. Re-scores the given labels with
truth_audit.RULES14 off into the working tree; then `git diff --stat` / `git diff -U0` on their results must show
the schema string and nothing else (checked for the labels named in the README).

usage: <venv python> -I verify_s13.py --corpus C <label>..."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

if __name__ == "__main__":
    argv = lib096.corpus_flag(sys.argv)
    truth_audit.RULES14["mapdefect"] = False
    for label in argv[1:]:
        sys.argv = ["score.py", label] + (["--corpus", lib096.CORPUS] if lib096.CORPUS != "dev" else [])
        score.main()
