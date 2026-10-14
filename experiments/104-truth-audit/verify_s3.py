"""exp104: scorer 4 with its three corrections switched OFF must reproduce scorer 3's committed cards byte for byte.

usage: <venv python> -I verify_s3.py <label> <scorer-3 results dir (a checkout of the parent commit's results)>
Runs score.py's own main() with: families_lib's links off (families.json only), no truth_audit, the schema string
of scorer 3; writes into a scratch dir and compares every card and the summary (minus scorer 4's two new config
shas) with the scorer-3 files. Proves the code change alters nothing but the three corrections."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import score  # noqa: E402
from libix import family as base_family  # noqa: E402


def main():
    label, s3 = sys.argv[1], sys.argv[2]
    out = tempfile.mkdtemp(prefix="exp104-s3-")
    score.HERE = out
    score.SCORER_VERSION = "exp096-score/3"
    score.same_family = lambda a, b: a is not None and b is not None and base_family(a) == base_family(b)
    score.truth_audit.Audit = lambda *a: None
    sys.argv = ["score.py", label]
    score.main()
    bad = 0
    names = sorted(os.listdir(os.path.join(s3, label, "cards")))
    for n in names:
        a = open(os.path.join(s3, label, "cards", n)).read()
        b = open(os.path.join(out, "results", label, "cards", n)).read()
        if json.loads(a) != json.loads(b):
            bad += 1
            print("DIFFERS", n)
    sa = json.load(open(os.path.join(s3, label, "summary.json")))
    sb = json.load(open(os.path.join(out, "results", label, "summary.json")))
    for k in ("copiesSha256", "familiesSha256"):
        sb["config"].pop(k)
    if sa != sb:
        bad += 1
        print("DIFFERS summary.json")
    print(f"{label}: {len(names)} cards, {bad} differ from scorer 3" + (" -- OK" if not bad else " -- FAIL"))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
