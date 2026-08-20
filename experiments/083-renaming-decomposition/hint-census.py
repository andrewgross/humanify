#!/usr/bin/env python3
"""For each word-choice (old->new) rename pair, find the prompt(s) that
produced `new` in this run's log and check whether `old` appeared in the
prompt text (i.e. the pipeline KNEW the prior name and the model deviated).

Approximation stated upfront: newName -> prompt matching is by answer value,
so a name minted by several prompts can mis-attribute; pairs whose old name
is a short common word ('data') can appear in a prompt by coincidence.
Treat shares as approximate; samples printed for eyeballing.
"""
import json, re, sys, collections

log_path, pairs_path = sys.argv[1], sys.argv[2]
content = open(log_path, encoding="utf8", errors="ignore").read()
blocks = content.split("=" * 80)
by_answer = collections.defaultdict(list)   # newName -> [(prompt, renames)]
for b in blocks:
    if "suggestAllNames - SUCCESS" not in b: continue
    if "--- USER PROMPT ---" not in b or "--- PARSED ---" not in b: continue
    prompt = b.split("--- USER PROMPT ---", 1)[1].split("---", 1)[0]
    parsed_txt = b.split("--- PARSED ---", 1)[1].strip()
    try:
        obj = json.loads(parsed_txt[: parsed_txt.rindex("}") + 1])
    except Exception:
        continue
    renames = obj.get("renames", obj)
    for k, v in renames.items():
        if isinstance(v, str):
            by_answer[v].append(prompt)

pairs = json.load(open(pairs_path))
KWDROP = re.compile(r'^src|[0-9]+$')

def norm(name):
    n = re.sub(r'^src', '', name); n = re.sub(r'\d+$', '', n)
    return (n[0].lower() + n[1:]) if n else n

hinted = nohint = notllm = 0
hinted_pairs, nohint_pairs, notllm_pairs = [], [], []
for e in pairs:
    o, n, cnt = e["old"], e["new"], e["n"]
    if norm(o) == norm(n):
        continue  # alias-form flips: not a model decision
    prompts = by_answer.get(n)
    if not prompts:
        notllm += cnt
        if len(notllm_pairs) < 10: notllm_pairs.append(f"{cnt:>3} {o} -> {n}")
        continue
    if any(re.search(r'\b' + re.escape(o) + r'\b', p) for p in prompts):
        hinted += cnt
        if len(hinted_pairs) < 10: hinted_pairs.append(f"{cnt:>3} {o} -> {n}")
    else:
        nohint += cnt
        if len(nohint_pairs) < 10: nohint_pairs.append(f"{cnt:>3} {o} -> {n}")

tot = hinted + nohint + notllm
print(f"non-alias rename mass: {tot} line-pairs")
print(f"  LLM answer, prior name WAS in the prompt (deviated):   {hinted} ({100*hinted/tot:.0f}%)")
print(f"  LLM answer, prior name NOT in the prompt (never knew): {nohint} ({100*nohint/tot:.0f}%)")
print(f"  new name not found among LLM answers (cascade/derived): {notllm} ({100*notllm/tot:.0f}%)")
print("\nhinted-deviated samples:");  [print("   ", s) for s in hinted_pairs]
print("\nno-hint samples:");          [print("   ", s) for s in nohint_pairs]
print("\nnot-an-LLM-answer samples:");[print("   ", s) for s in notllm_pairs]
