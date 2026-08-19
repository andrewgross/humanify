import re, subprocess, sys, collections
KW=set("""var let const function return if else for while do switch case break continue new typeof
instanceof in of delete void null true false this async await yield throw try catch finally class
extends super import export from default require module exports""".split())
# Mask ONLY identifiers that are not preceded by a dot and not followed by a colon.
# A property name or object key is semantic: tool_use_id -> is_error is REAL change.
tok = re.compile(r'(\.\s*)?([A-Za-z_$][A-Za-z0-9_$]*)(\s*:)?')
def norm(s):
    def rep(m):
        dot, name, colon = m.group(1), m.group(2), m.group(3)
        if dot or colon or name in KW:
            return m.group(0)
        return (dot or '') + 'X' + (colon or '')
    return tok.sub(rep, s).strip()

old, new = sys.argv[1], sys.argv[2]
p = subprocess.run(["git","diff","--no-index","--unified=0",old,new], capture_output=True, text=True)
ls = p.stdout.split("\n")
minus=[l[1:] for l in ls if l.startswith('-') and not l.startswith('---')]
plus =[l[1:] for l in ls if l.startswith('+') and not l.startswith('+++')]
total = len(minus)+len(plus)

# Drop lines that appear unchanged on both sides — those are not edits at all.
mset, pset = collections.Counter(minus), collections.Counter(plus)
identical = sum(min(c, pset.get(k,0)) for k,c in mset.items())
m2 = list(minus); p2 = list(plus)
for k,c in mset.items():
    n = min(c, pset.get(k,0))
    for _ in range(n):
        m2.remove(k); p2.remove(k)

mn = collections.Counter(norm(l) for l in m2)
pn = collections.Counter(norm(l) for l in p2)
nameonly = sum(min(c, pn.get(k,0)) for k,c in mn.items())

print(f"total changed lines                 {total:>8,}")
print(f"  unchanged text on both sides      {identical*2:>8,}  ({200*identical/total:.1f}%)")
print(f"  NAME-ONLY (local/binding renames) {nameonly*2:>8,}  ({200*nameonly/total:.1f}%)")
print(f"  remaining (real edits)            {total-identical*2-nameonly*2:>8,}")
