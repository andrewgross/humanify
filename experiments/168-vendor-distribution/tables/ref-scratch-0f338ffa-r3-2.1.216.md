# ref-scratch-0f338ffa-r3 — 2.1.215->2.1.216 (default mode)

## Run status

| check                     | value                |
| ------------------------- | -------------------- |
| runStatus                 | exitCode 0, errors 0 |
| selfHopCard               | present              |
| tree 2.1.216              | present              |
| tree 2.1.216-selfhop      | present              |
| tree 2.1.216-selfhop-warm | present              |

## Consistency checks (each counter shown able to fire)

| check                                  | value                                                                                                                                                                                                                       |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| factoryShapePairing                    | 1643/1647 fresh vendor factory files paired to a raw factory by shape (1620/1624 prior); record order would have agreed for 829                                                                                             |
| carryOverReconstructed                 | 1620 factories inherited by (structural hash, ordinal) at the same path (the run log's "carry-over" count is the check where a log exists)                                                                                  |
| eraVendorStatements                    | 0                                                                                                                                                                                                                           |
| factoriesWithContestedClaim            | 0                                                                                                                                                                                                                           |
| indexLabelledFreshFactories            | 0                                                                                                                                                                                                                           |
| restorationApplicationSlots            | 0                                                                                                                                                                                                                           |
| restorationApplicationSlotsInFactories | 0                                                                                                                                                                                                                           |
| eraTrailFileMapping                    | 683/693 trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); 200417 fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/ |
| eraRestorationSiteAApplied             | 0                                                                                                                                                                                                                           |
| eraRestorationFrozen                   | null                                                                                                                                                                                                                        |
| mintedCensusTotal                      | 88                                                                                                                                                                                                                          |

## 1. Where code goes (the fresh tree, .humanify/ excluded)

| area    | files | lines   | bytes      | share of lines |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 901,536 | 47,972,520 | 100.0%         |
| src/    | 4,851 | 869,254 | 41,912,207 | 96.4%          |
| vendor/ | 1,648 | 27,286  | 5,760,541  | 3.0%           |
| other   | 4     | 4,996   | 299,772    | 0.6%           |

### vendor/ by the mechanism that placed each file (exclusive)

| mechanism                                               | files | lines  | bytes     | share of vendor bytes |
| ------------------------------------------------------- | ----- | ------ | --------- | --------------------- |
| TOTAL                                                   | 1,648 | 27,286 | 5,760,541 | 100%                  |
| inherited from prior by hash (factory carry-over)       | 1,620 | 14,417 | 5,273,526 | 91.5%                 |
| index claim moved by package                            | 0     | 0      | 0         | 0.0%                  |
| unclaimed-but-clean, moved on index owner+file (exp157) | 0     | 0      | 0         | 0.0%                  |
| whole-module factory, named this run (model/declared)   | 27    | 876    | 133,690   | 2.3%                  |
| root lib\_\* fallback                                   | 0     | 0      | 0         | 0.0%                  |
| era (app) statements filed into vendor by placement     | 0     | 0      | 0         | 0.0%                  |
| the vendor record (\_bun-modules.json)                  | 1     | 11,993 | 353,325   | 6.1%                  |
| other                                                   | 0     | 0      | 0         | 0.0%                  |

### finer detail (mechanism :: label)

| mechanism :: detail                                                                 | files | lines  | bytes     |
| ----------------------------------------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over) :: carried; origin label llm      | 1,318 | 13,341 | 4,587,777 |
| inherited from prior by hash (factory carry-over) :: carried; origin label fallback | 283   | 977    | 366,163   |
| the vendor record (\_bun-modules.json) :: humanify's own metadata, not library code | 1     | 11,993 | 353,325   |
| inherited from prior by hash (factory carry-over) :: carried; origin label url      | 18    | 97     | 318,553   |
| whole-module factory, named this run (model/declared) :: llm, decided this run      | 18    | 850    | 123,440   |
| whole-module factory, named this run (model/declared) :: url, decided this run      | 9     | 26     | 10,250    |
| inherited from prior by hash (factory carry-over) :: carried; origin label banner   | 1     | 2      | 1,033     |

### per package (top 25 by bytes, then the long tail)

| package                   | files | lines  | bytes     | share of bytes | dominant mechanism (by bytes)                         |
| ------------------------- | ----- | ------ | --------- | -------------- | ----------------------------------------------------- |
| (vendor root)             | 650   | 17,288 | 2,148,547 | 37.3%          | inherited from prior by hash (factory carry-over)     |
| protobufjs                | 33    | 189    | 296,827   | 5.2%           | inherited from prior by hash (factory carry-over)     |
| highlight                 | 3     | 8      | 205,995   | 3.6%           | inherited from prior by hash (factory carry-over)     |
| tslib                     | 14    | 76     | 186,273   | 3.2%           | inherited from prior by hash (factory carry-over)     |
| react                     | 7     | 847    | 178,917   | 3.1%           | inherited from prior by hash (factory carry-over)     |
| design-sync               | 3     | 2,033  | 123,843   | 2.1%           | inherited from prior by hash (factory carry-over)     |
| w3                        | 5     | 25     | 122,220   | 2.1%           | inherited from prior by hash (factory carry-over)     |
| google-auth-library       | 36    | 200    | 109,276   | 1.9%           | inherited from prior by hash (factory carry-over)     |
| cobol                     | 2     | 4      | 88,384    | 1.5%           | inherited from prior by hash (factory carry-over)     |
| @grpc/grpc-js             | 21    | 154    | 77,826    | 1.4%           | inherited from prior by hash (factory carry-over)     |
| xmldom                    | 27    | 124    | 76,839    | 1.3%           | inherited from prior by hash (factory carry-over)     |
| ajv-errors                | 2     | 154    | 76,383    | 1.3%           | whole-module factory, named this run (model/declared) |
| ajv                       | 23    | 112    | 67,112    | 1.2%           | inherited from prior by hash (factory carry-over)     |
| sharp                     | 10    | 70     | 66,828    | 1.2%           | inherited from prior by hash (factory carry-over)     |
| prismjs                   | 24    | 49     | 65,588    | 1.1%           | inherited from prior by hash (factory carry-over)     |
| fs-extra                  | 38    | 475    | 57,586    | 1.0%           | inherited from prior by hash (factory carry-over)     |
| grpc-js                   | 18    | 81     | 55,131    | 1.0%           | inherited from prior by hash (factory carry-over)     |
| aws-sdk-client-middleware | 13    | 65     | 54,851    | 1.0%           | inherited from prior by hash (factory carry-over)     |
| python                    | 7     | 15     | 54,805    | 1.0%           | inherited from prior by hash (factory carry-over)     |
| color-convert             | 4     | 316    | 53,061    | 0.9%           | inherited from prior by hash (factory carry-over)     |
| @aws-sdk/smithy-client    | 4     | 26     | 51,339    | 0.9%           | inherited from prior by hash (factory carry-over)     |
| aws-sdk-middleware-logger | 2     | 12     | 50,924    | 0.9%           | inherited from prior by hash (factory carry-over)     |
| esbuild                   | 2     | 868    | 48,812    | 0.8%           | inherited from prior by hash (factory carry-over)     |
| openid-client             | 7     | 77     | 47,653    | 0.8%           | inherited from prior by hash (factory carry-over)     |
| css                       | 4     | 8      | 42,962    | 0.7%           | inherited from prior by hash (factory carry-over)     |
| long tail (166 packages)  | 689   | 4,010  | 1,352,559 | 23.5%          | —                                                     |

### vendor-root loose files

| mechanism                                             | files | lines  | bytes     |
| ----------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over)     | 648   | 4,977  | 1,775,478 |
| the vendor record (\_bun-modules.json)                | 1     | 11,993 | 353,325   |
| whole-module factory, named this run (model/declared) | 1     | 318    | 19,744    |

## 2. How identifiers are named

### vendor/ factory bodies (each binding zipped against the raw bundle's factory)

| name source                                                         | bindings | share |
| ------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                               | 53,218   | 100%  |
| left as bundled (minifier's name)                                   | 34,898   | 65.6% |
| carried from prior: file inherited, still the prior's minifier name | 17,851   | 33.5% |
| factory not paired to the raw bundle                                | 469      | 0.9%  |

### vendor/ era (app) statements (strategy trail, by terminal step)

| name source | bindings | share |
| ----------- | -------- | ----- |
| TOTAL       | 0        | 100%  |

### src/ (strategy trail, by terminal step) — the contrast

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 202,344  | 100%  |
| carried from prior by exact match                       | 186,609  | 92.2% |
| carried from prior by close match                       | 8,772    | 4.3%  |
| model-named                                             | 5,034    | 2.5%  |
| left as bundled (no final name, or kept the minifier's) | 1,234    | 0.6%  |
| post-split reconcile (finish rule)                      | 693      | 0.3%  |
| other (class-id-floor)                                  | 2        | 0.0%  |

### app bindings whose file could not be mapped

| name source                                                         | bindings | share  |
| ------------------------------------------------------------------- | -------- | ------ |
| TOTAL                                                               | 6        | 100%   |
| left as bundled (no final name, or kept the minifier's) [generated] | 6        | 100.0% |

## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)

| bucket                                  | lines | of which mint-bearing |
| --------------------------------------- | ----- | --------------------- |
| TOTAL churnLines                        | 1,917 | 0                     |
| manifest                                | 203   | 0                     |
| name re-rolls (nameOnly+freeReroll)     | 32    | 0                     |
| moved / relocated (movedPath+relocated) | 1,538 | 0                     |
| real: changed in place or paired        | 24    | 0                     |
| real: truly added                       | 120   | 0                     |
| real: truly removed                     | 0     | 0                     |
| other (unparsed)                        | 0     | 0                     |
| = noise (vendorLn)                      | 1,773 |                       |
| = real (vendorReal)                     | 144   |                       |

### file-path stability (vendor files)

| outcome                                                 | files | lines charged |
| ------------------------------------------------------- | ----- | ------------- |
| kept path, byte-identical                               | 1,612 | 0             |
| new (content absent from the prior)                     | 23    | 120           |
| kept path, nameOnly                                     | 8     | 32            |
| moved, realChange                                       | 3     | 24            |
| moved and changed a little (relocation pair, path draw) | 3     | 1,534         |
| moved, movedPath                                        | 1     | 4             |

### identifier-name stability for identical code (same content signature)

| file kind: outcome           | bindings |
| ---------------------------- | -------- |
| factory file: different name | 150      |
| factory file: same name      | 57,655   |
