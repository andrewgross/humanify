#!/usr/bin/env python3
"""Lever-1 sizing: of the LLM questions asked on release N and N+1, how many
are byte-identical, and how often did the model answer them differently?

Parses the -vv logs' suggestAllNames SUCCESS blocks: USER PROMPT -> PARSED
renames. Key = exact user-prompt text.

What this filter WRONGLY includes / misses (stated before quoting numbers):
- retries/fallback rounds re-ask with tweaked prompts -> count as distinct
  questions (correct for our purpose: a deterministic model still sees them).
- blocks whose PARSED lacks a renames dict are skipped and counted.
"""
import sys, json, re, collections

def parse(path):
    qa = {}
    dup_diff = 0
    skipped = 0
    with open(path, encoding="utf8", errors="ignore") as fh:
        content = fh.read()
    blocks = content.split("=" * 80)
    for b in blocks:
        if "suggestAllNames - SUCCESS" not in b: continue
        if "--- USER PROMPT ---" not in b or "--- PARSED ---" not in b: continue
        up = b.split("--- USER PROMPT ---", 1)[1]
        prompt = up.split("---", 1)[0].strip()
        parsed_txt = b.split("--- PARSED ---", 1)[1].strip()
        # PARSED is a pretty-printed JSON object; take to the last closing brace
        try:
            obj = json.loads(parsed_txt[: parsed_txt.rindex("}") + 1])
        except Exception:
            skipped += 1
            continue
        renames = obj.get("renames", obj)
        renames = {k: v for k, v in renames.items()
                   if not k.startswith("_") and k not in ("finishReason", "usage")}
        if prompt in qa and qa[prompt] != renames:
            dup_diff += 1  # same question asked twice in one run, diff answer
        qa[prompt] = renames
    return qa, dup_diff, skipped

a_path, b_path = sys.argv[1], sys.argv[2]
A, adup, askip = parse(a_path)
B, bdup, bskip = parse(b_path)
print(f"A: {len(A)} distinct questions (skipped {askip}, within-run re-ask-diff {adup})")
print(f"B: {len(B)} distinct questions (skipped {bskip}, within-run re-ask-diff {bdup})")
shared = set(A) & set(B)
same_ans = sum(1 for q in shared if A[q] == B[q])
print(f"IDENTICAL questions in both: {len(shared)}")
print(f"  answered identically:   {same_ans}")
print(f"  answered DIFFERENTLY:   {len(shared) - same_ans}")
# how many identifiers those disagreements cover
ids_diff = 0
names_diff = 0
for q in shared:
    if A[q] == B[q]: continue
    ka, kb = set(A[q]), set(B[q])
    ids_diff += len(ka | kb)
    names_diff += sum(1 for k in ka & kb if A[q][k] != B[q][k])
print(f"  identifiers under disagreement: {ids_diff}, of which named differently: {names_diff}")
