# exp257 — names and imports that stay put across a release

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), measured on
real-LLM walks of two hops (2.1.285 -> 286 -> 287), one walk per
commit. Branch `exp257-work`. Follows exp256, which built the measure.

## Totals

Changed lines outside `.humanify/`, real-LLM walk, cold first version.
"Start" is the exp256 result (walk 4). "End" is the last walk of this
experiment (walk 13).

| hop | walk | all lines | real (ceiling) | noise | wiring | unknown |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 2.1.285 -> 286 | start | 45,251 | 39,364 | 2,956 | 1,299 | 1,632 |
| 2.1.285 -> 286 | end | 43,185 | 38,695 | 2,084 | 1,201 | 1,205 |
| 2.1.286 -> 287 | start | 51,973 | 44,720 | 3,710 | 1,643 | 1,900 |
| 2.1.286 -> 287 | end | 49,876 | 43,599 | 2,851 | 1,612 | 1,814 |

- Every version of every walk boots, the prompt check included, and
  every run exits 0.
- Peak memory is unchanged: about 19.5 GB cold and about 31 GB with a
  previous tree.
- "Real" is a ceiling: every changed line inside a changed input
  function counts as real.

## How far two walks of one pipeline differ

Walks 4 and 5 ran the same pipeline. Their noise counts were 2,956 and
2,443 on the first hop and 3,710 and 3,846 on the second. A difference
of up to about 500 noise lines between two single walks is therefore
inside the model's run-to-run variation. Each step below was also
checked by its cause: the lines it was built to remove were looked up
by name in the labelled diff of the next walk.

## Every walk

| walk | what the commit added | 285 -> 286 all / noise | 286 -> 287 all / noise |
| --- | --- | ---: | ---: |
| 4 | exp256 result | 45,251 / 2,956 | 51,973 / 3,710 |
| 5 | nothing (repeat) | 44,839 / 2,443 | 52,322 / 3,846 |
| 6 | export key follows a renamed binding | 44,715 / 2,519 | 52,187 / 3,690 |
| 7 | import alias rule, library import names | 44,539 / 2,401 | 51,295 / 3,240 |
| 8 | Node built-in imports at top level | 44,166 / 2,255 | 51,571 / 3,396 |
| 9 | wrapper takes the wrapped function's name | 43,870 / 2,127 | 50,950 / 3,046 |
| 10 | exact previous import names, alias restore | 43,879 / 2,132 | 50,843 / 2,974 |
| 11 | content-first close matching | 43,334 / 2,114 | 51,386 / 3,352 |
| 12 | new local makes way for an import name, class names from methods | 43,109 / 2,009 | 50,184 / 2,883 |
| 13 | loop variables, catch parameters and destructured parameters of an edited function | 43,185 / 2,084 | 49,876 / 2,851 |

## What was built

1. **An export key follows its binding.** A late step gives a
   top-level binding its previous name inside its own file. The name
   the file exports it under, and every other file's use of that name,
   now change with it. One init function used to cost about 240 caller
   lines on one hop.
2. **An import alias gives way only to a local that can see the
   module's reads.** A new local variable of the same name elsewhere in
   the file no longer pushes the alias to a longer name.
3. **A file's library imports keep the names its previous copy used**
   (`pathModule327` stays `pathModule327`), paired by package and
   position.
4. **A lazy chunk's Node built-in imports are plain top-level
   declarations.** They used to sit inside the chunk's startup function
   and moved whenever Bun regrouped chunks.
5. **A wrapper the old callers now call takes the wrapped function's
   name.** When upstream puts a new function in front of an old one and
   the matched callers all call the new one, the new one takes the
   name and the old one is named again. The rule declines when any
   matched caller still calls the old function.
6. **Import aliases are restored after the late rename step.** The
   alias is picked before that step gives locals their previous names
   back; a second pass then gives the alias its previous name back
   where it is free. A new, unexported top-level variable holding that
   name is renamed out of the way.
7. **An edited function pairs with the earlier function whose content
   it shares.** Close matching ranked candidates by shape counts only,
   so React components of one shape could swap names and files. It now
   ranks by shared strings, property names and outside references
   first. On the first hop, same-name function pairs that share under
   20% of their strings fell from 10 to 3 (2,703 lines to 389) in a
   stub A/B run.
8. **A changed class takes the name of the class its methods match.**
   Each unchanged method casts one vote for its class's previous name.
   Two agreeing votes name the class; one does not. A changed class
   used to depend on the model repeating the name, and a new name also
   moved the class to another file or another place in its file.

9. **An edited function keeps its loop variables, catch parameters
   and destructured parameters.** The variable in the head of a loop
   or `try` that has an edit inside it was not carried over, and
   neither were names inside a destructured parameter. Both now carry
   when the head, or the parameter's shape, is unchanged. On the
   second hop the two largest cases (75 and 53 lines) are gone, and
   changed lines that differ from the previous version only by a name
   fell from 2,042 to 1,844. The first hop had no large case of this
   kind (2,716 to 2,692).

## Decisions

- **Sibling init calls stay in Bun's order** (label
  `imports-reordered`, about 1,270 and 1,900 lines, more than half of
  the noise left). Reordering them changes which module runs first.
- **A function upstream changed is still named by the model**, with
  the previous code as context. Only the cases above, where other
  unchanged code names it, are settled without the model.

## Noise left, by label (walk 13)

| label | 285 -> 286 | 286 -> 287 |
| --- | ---: | ---: |
| imports-reordered | 1,266 | 1,901 |
| linker-renamed | 270 | 446 |
| moved-in-file | 149 | 203 |
| renamed | 166 | 151 |
| reshaped | 197 | 100 |
| moved-file | 32 | 23 |
| linker-moved | 2 | 21 |
| require-renamed | 2 | 6 |

## Open

- `linker-renamed`: names in linker-written text change when Bun
  regroups chunks.
- Part of `renamed` is real change the measure cannot see: callers
  that switched to a new function while the old one still has users.
- Library import names are chosen before the late rename step, the
  same ordering weakness the alias restore fixes for aliases.
- Assignments inside a startup function move file when Bun moves a
  module's init (about 10 lines).
- Private class members are numbered by position (`#map3`), so one
  inserted above renumbers the rest (about 7 lines).
- About 7 to 8% of `src/` (roughly 125,000 lines) is library code the
  index did not recognise, mostly ESM packages Bun hoisted to top
  level with no module wrapper.
- The first, cold version takes about 50 minutes.
