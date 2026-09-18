# 091 — BRIEF (hypothesis): stop the model reusing names the prior file already owns

> Written 2026-09-18 as the next-experiment candidate. NOT yet approved for
> build — task 0 first. Origin: the 2026-08-21 noise dig's verified case —
> the model renamed prior `frameRows` to `sessionLabel` while the prior
> file's OWN `sessionLabel` binding lived three functions away. That
> collision entangles two identities in the diff and poisons the restore
> machinery both ways (occurrence-outside-diff + consumer-to-name-live
> refusals both trace to this class, ~280 git lines on the busy hop).

## Why this is NOT exp089 again

exp089 added SUGGESTIONS to prompts ("previously named X") — ignorable, and
GLM/gpt-oss already infer as much from context; refuted, +20% decorated
prompts moved nothing. This lever extends the AVOID-LIST — which is
mechanically ENFORCED: a suggestion that hits the avoid set fails
validation and drives the existing retry, exactly like duplicate names do
today. The model cannot ignore it. Different mechanism, different fate
possible.

## The design sketch

For close-match (and full-LLM-with-prior-context) prompts, add to
`usedNames` the prior FILE's binding names that (a) were NOT transferred or
hinted to any binding in this function, and (b) are still live in the prior
file outside this function. The model then cannot re-mint `sessionLabel`
for an unrelated loop variable, because `sessionLabel` is spoken for.

## The risk the design must answer (task 0's job)

An avoid-list can block a CORRECT answer: if the model wants to give a
binding its rightful prior name and our correspondence machinery failed to
mark it as such, the avoid-list turns a would-be stability win into a
forced rename. Task 0 must count both directions on real logs:

1. Collisions the list would prevent: fresh LLM answers equal to a
   prior-file name owned by a DIFFERENT binding (the frameRows class).
2. Correct reuses the list would block: fresh LLM answers equal to the
   SAME binding's prior name where no transfer/hint existed (the machinery
   missed it but the model landed it anyway).

If (2) is not clearly smaller than (1), the lever dies here. Both counts
come from existing -vv walk logs + rename-pair censuses — no code, no walk.

## Gates if built

Red/green units on the avoid-set assembly; `npm run check`; cold walk with
`novel`/`realLines` exact, the occ-outside-diff and to-name-live refusal
masses down, retry volume NOT meaningfully up (an over-broad avoid-list
shows up as retries), calm hop inside the band. gpt-oss era (post-swap-back
baselines: /work/exp088-walk, band 35/32).
