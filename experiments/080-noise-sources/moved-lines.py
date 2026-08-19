import subprocess, collections
p=subprocess.run(["git","diff","--no-index","--unified=0","2.1.215/src","2.1.216/src"],
                 capture_output=True,text=True)
cur=None; dels=collections.defaultdict(list); adds=collections.defaultdict(list)
for l in p.stdout.split("\n"):
    if l.startswith("--- ") or l.startswith("+++ "):
        if l.startswith("+++ "): cur=l[4:]
        continue
    if l.startswith("-"): dels[l[1:]].append(cur)
    elif l.startswith("+"): adds[l[1:]].append(cur)
same_file=cross_file=0
for text, dfiles in dels.items():
    if text not in adds or not text.strip(): continue
    afiles=adds[text]
    for i in range(min(len(dfiles), len(afiles))):
        if dfiles[i]==afiles[i]: same_file+=1
        else: cross_file+=1
print(f"identical lines appearing as both delete and add:")
print(f"  same file  (moved WITHIN a file)   {same_file*2:>7,}")
print(f"  different file (moved BETWEEN files) {cross_file*2:>5,}")
