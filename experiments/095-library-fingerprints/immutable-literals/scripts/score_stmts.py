"""Score every distinct top-level statement of the minified lab builds (data/stmt-lits.jsonl) -> results/scores-stmts.jsonl"""
import os, sys, json
sys.path.insert(0, "/work/exp095-library-fingerprints/immutable-literals/scripts")
from score import run, HERE, DATA
V = os.environ.get('VARIANT', '')  # weight variant suffix ('' = plain IDF, '-capvocab' = word-like literals capped)
q = []
for l in open(DATA + "stmt-lits.jsonl"):
    r = json.loads(l)
    q.append(({"sha": r["sha"]}, r["lits"], None))
run(q, HERE + "results/scores-stmts"+V+".jsonl")
