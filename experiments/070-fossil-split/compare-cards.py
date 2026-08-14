import json

a = json.load(open("/tmp/070-card-orig.json"))
b = json.load(open("/tmp/070-card-scrambled.json"))


def pick(c):
    ch = c["churn"]
    keys = ("novel", "realLn", "noise", "noiseLn", "clean", "statements")
    return {k: ch.get(k) for k in keys if k in ch}


pa, pb = pick(a), pick(b)
print("original :", pa)
print("scrambled:", pb)
print("IDENTICAL columns:", sorted(k for k in pa if pa[k] == pb.get(k)))
print("DIFFERING columns:", sorted(k for k in pa if pa[k] != pb.get(k)))
print(
    "relocations (layout-DEPENDENT, should differ): orig",
    a["churn"].get("relocations"),
    "scrambled",
    b["churn"].get("relocations"),
)
