"""The scorer's ONE package-family judgement (scorer 4). Two packages are one family when families.json says so
(the matcher's table: listed families, same npm scope, scoped forks; libix.family), OR when the links exp104
derived from npm registry metadata connect them (families-derived.json: same repository, a renamed-to
deprecation, a GitHub fork republished under a scope; a chain of links counts).

The judgement is PAIRWISE on purpose: folding the derived links into the scope families transitively would chain
unrelated packages through a scope (@xtuc/long ~ long and @xtuc/ieee754 ~ ieee754 would make long ~ ieee754;
@openpgp/tweetnacl ~ tweetnacl and @openpgp/jsdoc ~ jsdoc would make tweetnacl ~ jsdoc). The miss classifier
(decompose.py) keeps the matcher's families.json: it explains why the MATCHER missed."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../095-library-fingerprints/selection/scripts"))
from libix import family as base_family  # noqa: E402

# ONE file for the scorer and the matcher (exp106): the Rust family judgement reads the same links
# (crates/humanify-core/src/libindex/families.rs), restricted to an index's own packages.
DERIVED = os.path.join(HERE, "../../crates/humanify-core/src/libindex/families-derived.json")


def derived_sha():
    """The derived links' identity: sha256 of the canonical spelling (the commit hook may re-spell the file)."""
    import lib096
    with open(DERIVED) as fh:
        return lib096.sha256_bytes(lib096.dump(json.load(fh)).encode())


class Families:
    def __init__(self, path=DERIVED):
        self.parent = {}
        with open(path) as fh:
            for x in json.load(fh)["links"]:
                self._union(x["a"], x["b"])

    def _find(self, a):
        p = self.parent
        root = a
        while p.get(root, root) != root:
            root = p[root]
        while p.get(a, a) != root:
            p[a], a = root, p[a]
        return root

    def _union(self, a, b):
        ra, rb = self._find(a), self._find(b)
        if ra != rb:
            lo, hi = sorted((ra, rb))
            self.parent[hi] = lo

    def same(self, a, b):
        if a is None or b is None:
            return False
        return base_family(a) == base_family(b) or self._find(a) == self._find(b)

    def linked(self, a, b):
        """True when ONLY the derived links make a and b one family (the relabel ledger's `family` rule)."""
        return base_family(a) != base_family(b) and self._find(a) == self._find(b)
