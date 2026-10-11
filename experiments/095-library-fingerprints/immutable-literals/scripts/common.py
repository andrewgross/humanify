"""Shared loading for the immutable-literal analysis. Run with tools/venv/bin/python."""
import os, re, json, math, collections
import numpy as np
import scipy.sparse as sp

HERE = "/work/exp095-library-fingerprints/immutable-literals/"
LAB = "/work/exp095-library-fingerprints/lab/"
DATA = HERE + "data/"


class Vocab:
    def __init__(self):
        self.ids = {}
        self.words = []

    def get(self, w):
        i = self.ids.get(w)
        if i is None:
            i = len(self.words)
            self.ids[w] = i
            self.words.append(w)
        return i

    def lookup(self, w):
        return self.ids.get(w)


def load_sources(vocab):
    rows = []
    # VARIANT containing "plus": the index also holds the Claude Code-era package versions (pkgs-cc/)
    for l in open(DATA + ("sources-plus.jsonl" if "plus" in os.environ.get("VARIANT", "") else "sources.jsonl")):
        r = json.loads(l)
        r["ids"] = np.array(sorted({vocab.get(w) for w in r.pop("lits")}), dtype=np.int64)
        rows.append(r)
    return rows


def load_jsonl(path):
    return [json.loads(l) for l in open(path)]


def to_ids(lits, vocab, add=False):
    if add:
        return np.array(sorted({vocab.get(w) for w in lits}), dtype=np.int64)
    s = set()
    for w in lits:
        i = vocab.lookup(w)
        s.add(i if i is not None else -1)  # -1 = literal absent from the index
    return s


class Index:
    """Index over source files. Package-level document frequency drives the weights."""

    def __init__(self, src, vocab, exclude_pkgs=()):
        self.src = [r for r in src if r["pkg"] not in exclude_pkgs]
        self.vocab = vocab
        V = len(vocab.words)
        pkgs = sorted({r["pkg"] for r in self.src})
        self.pkgs = pkgs
        self.pkg_ix = {p: i for i, p in enumerate(pkgs)}
        # sort files by package so per-package maxima are a reduceat
        self.src.sort(key=lambda r: (self.pkg_ix[r["pkg"]], r["ver"], r["file"]))
        self.file_pkg = np.array([self.pkg_ix[r["pkg"]] for r in self.src])
        self.pkg_start = np.searchsorted(self.file_pkg, np.arange(len(pkgs)))
        rows, cols = [], []
        for i, r in enumerate(self.src):
            rows.append(np.full(len(r["ids"]), i))
            cols.append(r["ids"])
        rows = np.concatenate(rows)
        cols = np.concatenate(cols)
        self.F = sp.csr_matrix((np.ones(len(rows), dtype=np.float32), (rows, cols)), shape=(len(self.src), V))
        # package x literal presence
        P = sp.csr_matrix((np.ones(len(rows), dtype=np.float32), (self.file_pkg[rows], cols)), shape=(len(pkgs), V))
        P.data[:] = 1
        P.sum_duplicates()
        P.data[:] = 1
        self.P = P
        self.df_pkg = np.asarray(P.sum(axis=0)).ravel()
        self.df_file = np.asarray((self.F > 0).sum(axis=0)).ravel()
        N = len(pkgs)
        self.N = N
        with np.errstate(divide="ignore"):
            w = np.log2(N / np.maximum(self.df_pkg, 1))
        w[self.df_pkg == 0] = 0  # not in this index
        if "capvocab" in os.environ.get("VARIANT", ""):
            # word-like literals (a short identifier or lowercase word: "role", "user", "content", "tool_use") are domain
            # vocabulary that app code shares with libraries; cap their weight at CAP bits.
            CAP = float(os.environ.get("CAP", "2"))
            wordy = re.compile(r"^[SP]:[A-Za-z_$][A-Za-z0-9_$-]*$")
            for i, word in enumerate(vocab.words):
                if wordy.match(word) and w[i] > CAP:
                    w[i] = CAP
        self.w = w.astype(np.float32)
        self.w_unknown = float(math.log2(N) + 1)  # a literal no indexed package has: rarer than anything we know

    def weights_of(self, idset):
        known = [i for i in idset if i >= 0 and self.df_pkg[i] > 0]
        unknown = len(idset) - len(known)
        return known, unknown

    def unknown_weight(self, lits):
        """total weight of the literals the index does not know"""
        cap = "capvocab" in os.environ.get("VARIANT", "")
        wordy = re.compile(r"^[SP]:[A-Za-z_$][A-Za-z0-9_$-]*$")
        tot = 0.0
        for w in lits:
            i = self.vocab.lookup(w)
            if i is None or self.df_pkg[i] == 0:
                tot += min(self.w_unknown, float(os.environ.get("CAP", "2"))) if cap and wordy.match(w) else self.w_unknown
        return tot

    def query_matrix(self, idsets, litlists):
        rows, cols, tot = [], [], np.zeros(len(idsets), dtype=np.float64)
        for qi, s in enumerate(idsets):
            known, unknown = self.weights_of(s)
            rows.extend([qi] * len(known))
            cols.extend(known)
            tot[qi] = float(self.w[known].sum()) + self.unknown_weight(litlists[qi])
        Q = sp.csr_matrix((self.w[cols], (rows, cols)), shape=(len(idsets), self.F.shape[1]))
        return Q, tot
