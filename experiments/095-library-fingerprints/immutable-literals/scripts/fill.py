"""Claim propagation along a statement sequence (Step B), shared by the lab and Claude Code evaluations.
own:      a statement's own literals claim a package (recognise.claim)
fill:     unbounded: an unclaimed statement takes P when the nearest claims on both sides are P
cfill:    'consistent' fill: as fill, but the gap is at most G statements and EVERY gap statement is consistent with P
          (no literals, or >= CONS of its literal weight is found in P)."""
from recognise import claim_q

def pscore(q, p):
    for name, s in q['top']:
        if name == p: return s
    return None

def consistent(q, p, cons):
    if q['tot'] <= 0: return True
    s = pscore(q, p)
    return s is not None and s >= cons * q['tot']

def propagate(qs, T, C, M, G=8, cons=0.9):
    own = [claim_q(q, T, C, M) for q in qs]
    n = len(qs)
    fill = list(own); cfill = list(own)
    claimed = [i for i, c in enumerate(own) if c]
    for a, b in zip(claimed, claimed[1:]):
        if b - a <= 1 or own[a] != own[b]: continue
        p = own[a]
        for i in range(a + 1, b): fill[i] = p
        if b - a - 1 <= G and all(consistent(qs[i], p, cons) for i in range(a + 1, b)):
            for i in range(a + 1, b): cfill[i] = p
    return own, fill, cfill
