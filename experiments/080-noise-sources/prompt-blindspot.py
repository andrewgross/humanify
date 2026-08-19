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
# ONLY the function prompt makes the promise being checked: one code block that
# should contain every identifier it asks about (buildBatchRenamePrompt).
# The MODULE-LEVEL prompt (buildModuleLevelRenameBody) emits one fenced profile
# PER identifier, so checking all of its identifiers against the last fence is
# meaningless — that mistake put this number at 0.9% when the truth is far
# lower. Anchor on the header each builder writes.
FN_PROMPT = re.compile(r'^Analyze this function and suggest descriptive names')
MODULE_PROMPT = re.compile(r'^Analyze these top-level module identifiers')

prompts = 0
asked = 0
missing = 0
missing_names = collections.Counter()
by_blocklen = collections.Counter()

code, in_code, cur = [], False, None
kind = None
skipped_module = 0
with open(path, encoding="utf8", errors="ignore") as fh:
    for line in fh:
        line = line.rstrip("\n")
        if FN_PROMPT.match(line):
            kind = "fn"
            continue
        if MODULE_PROMPT.match(line):
            kind = "module"
            continue
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
        if m and kind == "module":
            skipped_module += 1
            cur = None
            continue
        if m and cur is not None:
            prompts += 1
            names = [n.strip() for n in m.group(1).split(",") if n.strip()]
            blocklen = cur.count("\n") + 1
            windowed = "omitted] " in cur or "omitted]" in cur
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
                    if windowed:
                        by_blocklen["WINDOWED (anchor dropped)"] += 1
                    elif blocklen >= 500:
                        by_blocklen["500 (at cap)"] += 1
                    else:
                        by_blocklen["<500, not windowed"] += 1
            cur = None

print(f"FUNCTION prompts checked       {prompts:>8,}")
print(f"module-level prompts skipped   {skipped_module:>8,}  (one fence per identifier — different contract)")
print(f"identifiers asked about        {asked:>8,}")
print(f"NOT PRESENT in the shown code  {missing:>8,}  ({100*missing/max(1,asked):.1f}%)")
print(f"\nof the missing, by block size: {dict(by_blocklen)}")
print("\nmost frequently asked-but-unseen:")
for n, c in missing_names.most_common(12):
    print(f"  {n:<28} {c}")
