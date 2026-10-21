"""exp132 LAB runner: run.py with a prebuilt binary (an uncommitted working tree), for iteration only. Labels it
writes are lab labels (buildCommit `lab:<binary sha12>`), never committed results; every committed label is run
through `npm run eval -- library` at a commit.

usage: <venv python> -I lab_run.py <label> --bin <path> [run.py's flags]"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import run  # noqa: E402
from lib096 import sha256_file  # noqa: E402


def main():
    argv = sys.argv[1:]
    i = argv.index("--bin")
    binary = os.path.abspath(argv[i + 1])
    del argv[i:i + 2]
    run.build = lambda commit, log: (binary, "lab:" + sha256_file(binary)[:12])
    sys.argv = [sys.argv[0]] + argv
    run.main()


if __name__ == "__main__":
    main()
