"""exp106's claims.py on any DEV corpus: `claims_c.py [--corpus dev|pop|apps|bundlers] <claims.py args>`.
Never test/test2 (they are summaries-only judges)."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
if lib096.CORPUS in lib096.PRIVATE:
    raise SystemExit("frozen judge: summaries only")
import claims  # noqa: E402

sys.argv = [sys.argv[0]] + ARGV
claims.main()
