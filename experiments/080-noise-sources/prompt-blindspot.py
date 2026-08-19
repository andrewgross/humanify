#!/usr/bin/env python3
"""
080 — how often is the LLM asked to name an identifier that is NOT in the code
it was shown?

  python3 experiments/080-noise-sources/prompt-blindspot.py <run.log>

Found from one bad name Andrew questioned: `cr_2` -> `unusedParameterPlaceholder`
(the value is `resolvedFallbackModel.model ?? fallbackModelCandidate` — neither
unused nor a placeholder). The prompt for it contained exactly 500 lines, the
MAX_CODE_LINES cap, with no elision markers — and `cr_2` appears nowhere in it.

exp015 replaced flat truncation with declaration-anchored windows for exactly
this reason. This checks whether that guarantee actually holds at scale.

A model asked to name a symbol it cannot see will invent something, and that
name then churns against whatever the previous release invented.
"""
import re, sys, collections

path = sys.argv[1] if len(sys.argv) > 1 else None
if not path:
    print(__doc__); sys.exit(1)

FENCE_OPEN = re.compile(r'^```javascript\s*$')
FENCE_CLOSE = re.compile(r'^```\s*$')
ASK = re.compile(r'^Identifiers to rename: (.+)$')

prompts = 0
asked = 0
missing = 0
missing_names = collections.Counter()
by_blocklen = collections.Counter()

code, in_code, cur = [], False, None
with open(path, encoding="utf8", errors="ignore") as fh:
    for line in fh:
        line = line.rstrip("\n")
        if FENCE_OPEN.match(line):
            in_code, code = True, []
            continue
        if in_code and FENCE_CLOSE.match(line):
            in_code = False
            cur = "\n".join(code)
            continue
        if in_code:
            code.append(line)
            continue
        m = ASK.match(line)
        if m and cur is not None:
            prompts += 1
            names = [n.strip() for n in m.group(1).split(",") if n.strip()]
            blocklen = cur.count("\n") + 1
            for n in names:
                asked += 1
                # NOT \b: `$` and `_` are identifier characters in JS but
                # `$` is a NON-word character to the regex engine, so `\b$e\b`
                # silently fails to match a real `$e` in the code. That bug put
                # this number at 1.5% when it is really far lower, with $e/$t/$u
                # topping the "missing" list — they were never missing.
                if not re.search(
                    r'(?<![A-Za-z0-9_$])' + re.escape(n) + r'(?![A-Za-z0-9_$])',
                    cur):
                    missing += 1
                    missing_names[n] += 1
                    by_blocklen["500 (at cap)" if blocklen >= 500 else "<500"] += 1
            cur = None

print(f"prompts with a code block      {prompts:>8,}")
print(f"identifiers asked about        {asked:>8,}")
print(f"NOT PRESENT in the shown code  {missing:>8,}  ({100*missing/max(1,asked):.1f}%)")
print(f"\nof the missing, by block size: {dict(by_blocklen)}")
print("\nmost frequently asked-but-unseen:")
for n, c in missing_names.most_common(12):
    print(f"  {n:<28} {c}")
