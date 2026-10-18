"""exp129: scorer 12 with both its changes switched OFF (the copies-v2 layer, origins12's `patched`) must reproduce the previous scorer's committed summary
(and, on a dev corpus, every card) for a label, byte for byte up to the schema string and the copies sha.

usage: <venv python> -I verify_copies1.py <label> <previous scorer version string> [--corpus C]
Run BEFORE the label is re-scored (its committed cards must still be the previous scorer's). Writes into a scratch
directory; compares parsed JSON (the commit hook re-spells JSON)."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def main():
    argv = lib096.corpus_flag(sys.argv)
    label, previous = argv[1], argv[2]
    committed = os.path.join(lib096.HERE, "results", label)
    truth_audit.COPIES2["on"] = False
    import origins as O
    O.RULES12["patched"] = False
    score.SCORER_VERSION = previous
    tmp = tempfile.mkdtemp(prefix="exp129-verify-")
    score.HERE = tmp
    sys.argv = [sys.argv[0], label] + (["--corpus", lib096.CORPUS] if lib096.CORPUS != "dev" else [])
    score.main()
    out = os.path.join(tmp, "results", label)
    diffs = []
    for root, _, files in os.walk(out):
        for f in files:
            a = os.path.join(root, f)
            b = os.path.join(committed, os.path.relpath(a, out))
            if f == "timing.json":
                continue
            if not os.path.exists(b) or json.load(open(a)) != json.load(open(b)):
                diffs.append(os.path.relpath(a, out))
    print(f"{label}: {'IDENTICAL' if not diffs else 'DIFFERS: ' + ', '.join(sorted(diffs)[:10])}")
    sys.exit(1 if diffs else 0)


if __name__ == "__main__":
    main()
