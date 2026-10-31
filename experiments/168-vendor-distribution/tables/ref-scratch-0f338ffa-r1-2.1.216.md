# ref-scratch-0f338ffa-r1 — 2.1.215->2.1.216 (default mode)

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
| eraTrailFileMapping                    | 678/692 trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); 200415 fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/ |
| eraRestorationSiteAApplied             | 0                                                                                                                                                                                                                           |
| eraRestorationFrozen                   | null                                                                                                                                                                                                                        |
| mintedCensusTotal                      | 90                                                                                                                                                                                                                          |

## 1. Where code goes (the fresh tree, .humanify/ excluded)

| area    | files | lines   | bytes      | share of lines |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 901,536 | 48,231,945 | 100.0%         |
| src/    | 4,851 | 869,254 | 42,171,697 | 96.4%          |
| vendor/ | 1,648 | 27,286  | 5,763,132  | 3.0%           |
| other   | 4     | 4,996   | 297,116    | 0.6%           |

### vendor/ by the mechanism that placed each file (exclusive)

| mechanism                                               | files | lines  | bytes     | share of vendor bytes |
| ------------------------------------------------------- | ----- | ------ | --------- | --------------------- |
| TOTAL                                                   | 1,648 | 27,286 | 5,763,132 | 100%                  |
| inherited from prior by hash (factory carry-over)       | 1,620 | 14,417 | 5,274,306 | 91.5%                 |
| index claim moved by package                            | 0     | 0      | 0         | 0.0%                  |
| unclaimed-but-clean, moved on index owner+file (exp157) | 0     | 0      | 0         | 0.0%                  |
| whole-module factory, named this run (model/declared)   | 27    | 876    | 133,853   | 2.3%                  |
| root lib\_\* fallback                                   | 0     | 0      | 0         | 0.0%                  |
| era (app) statements filed into vendor by placement     | 0     | 0      | 0         | 0.0%                  |
| the vendor record (\_bun-modules.json)                  | 1     | 11,993 | 354,973   | 6.2%                  |
| other                                                   | 0     | 0      | 0         | 0.0%                  |

### finer detail (mechanism :: label)

| mechanism :: detail                                                                 | files | lines  | bytes     |
| ----------------------------------------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over) :: carried; origin label llm      | 1,286 | 13,081 | 4,418,859 |
| inherited from prior by hash (factory carry-over) :: carried; origin label fallback | 315   | 1,237  | 535,808   |
| the vendor record (\_bun-modules.json) :: humanify's own metadata, not library code | 1     | 11,993 | 354,973   |
| inherited from prior by hash (factory carry-over) :: carried; origin label url      | 18    | 97     | 318,609   |
| whole-module factory, named this run (model/declared) :: llm, decided this run      | 18    | 850    | 123,546   |
| whole-module factory, named this run (model/declared) :: url, decided this run      | 9     | 26     | 10,307    |
| inherited from prior by hash (factory carry-over) :: carried; origin label banner   | 1     | 2      | 1,030     |

### per package (top 25 by bytes, then the long tail)

| package                         | files | lines  | bytes     | share of bytes | dominant mechanism (by bytes)                         |
| ------------------------------- | ----- | ------ | --------- | -------------- | ----------------------------------------------------- |
| (vendor root)                   | 782   | 16,343 | 2,482,549 | 43.1%          | inherited from prior by hash (factory carry-over)     |
| protobufjs                      | 31    | 181    | 294,740   | 5.1%           | inherited from prior by hash (factory carry-over)     |
| highlight                       | 2     | 6      | 186,879   | 3.2%           | inherited from prior by hash (factory carry-over)     |
| react                           | 6     | 519    | 162,899   | 2.8%           | inherited from prior by hash (factory carry-over)     |
| typescript                      | 2     | 15     | 137,304   | 2.4%           | inherited from prior by hash (factory carry-over)     |
| design-sync                     | 3     | 2,033  | 123,843   | 2.1%           | inherited from prior by hash (factory carry-over)     |
| w3                              | 5     | 25     | 122,198   | 2.1%           | inherited from prior by hash (factory carry-over)     |
| google-auth-library             | 32    | 174    | 93,863    | 1.6%           | inherited from prior by hash (factory carry-over)     |
| forge-pki                       | 6     | 59     | 93,461    | 1.6%           | inherited from prior by hash (factory carry-over)     |
| sharp                           | 10    | 65     | 88,160    | 1.5%           | inherited from prior by hash (factory carry-over)     |
| ajv-errors-legacy               | 4     | 175    | 77,963    | 1.4%           | whole-module factory, named this run (model/declared) |
| ajv-keywords                    | 24    | 115    | 70,950    | 1.2%           | inherited from prior by hash (factory carry-over)     |
| storybook-screenshots           | 2     | 1,014  | 62,987    | 1.1%           | inherited from prior by hash (factory carry-over)     |
| node-forge                      | 6     | 31     | 61,228    | 1.1%           | inherited from prior by hash (factory carry-over)     |
| semver                          | 28    | 173    | 58,184    | 1.0%           | inherited from prior by hash (factory carry-over)     |
| aws-sdk-client-middleware       | 13    | 65     | 54,811    | 1.0%           | inherited from prior by hash (factory carry-over)     |
| @opentelemetry/sdk-metrics-base | 22    | 131    | 51,717    | 0.9%           | inherited from prior by hash (factory carry-over)     |
| @aws-sdk/smithy-client          | 4     | 26     | 51,264    | 0.9%           | inherited from prior by hash (factory carry-over)     |
| esbuild                         | 2     | 868    | 48,812    | 0.8%           | inherited from prior by hash (factory carry-over)     |
| openid-client                   | 8     | 81     | 48,417    | 0.8%           | inherited from prior by hash (factory carry-over)     |
| fs-extra                        | 21    | 384    | 45,295    | 0.8%           | inherited from prior by hash (factory carry-over)     |
| ts-morph                        | 4     | 706    | 44,006    | 0.8%           | inherited from prior by hash (factory carry-over)     |
| css                             | 4     | 8      | 42,032    | 0.7%           | inherited from prior by hash (factory carry-over)     |
| forge-tls                       | 2     | 16     | 41,738    | 0.7%           | inherited from prior by hash (factory carry-over)     |
| color-palette                   | 2     | 625    | 37,117    | 0.6%           | whole-module factory, named this run (model/declared) |
| long tail (166 packages)        | 623   | 3,448  | 1,180,715 | 20.5%          | —                                                     |

### vendor-root loose files

| mechanism                                             | files | lines  | bytes     |
| ----------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over)     | 771   | 4,308  | 2,120,345 |
| the vendor record (\_bun-modules.json)                | 1     | 11,993 | 354,973   |
| whole-module factory, named this run (model/declared) | 10    | 42     | 7,231     |

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
| TOTAL                                                   | 202,318  | 100%  |
| carried from prior by exact match                       | 186,606  | 92.2% |
| carried from prior by close match                       | 8,773    | 4.3%  |
| model-named                                             | 5,034    | 2.5%  |
| left as bundled (no final name, or kept the minifier's) | 1,211    | 0.6%  |
| post-split reconcile (finish rule)                      | 692      | 0.3%  |
| other (class-id-floor)                                  | 2        | 0.0%  |

### app bindings whose file could not be mapped

| name source                                                         | bindings | share  |
| ------------------------------------------------------------------- | -------- | ------ |
| TOTAL                                                               | 7        | 100%   |
| left as bundled (no final name, or kept the minifier's) [generated] | 7        | 100.0% |

## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)

| bucket                                  | lines | of which mint-bearing |
| --------------------------------------- | ----- | --------------------- |
| TOTAL churnLines                        | 1,917 | 0                     |
| manifest                                | 203   | 0                     |
| name re-rolls (nameOnly+freeReroll)     | 32    | 0                     |
| moved / relocated (movedPath+relocated) | 1,536 | 0                     |
| real: changed in place or paired        | 26    | 0                     |
| real: truly added                       | 120   | 0                     |
| real: truly removed                     | 0     | 0                     |
| other (unparsed)                        | 0     | 0                     |
| = noise (vendorLn)                      | 1,771 |                       |
| = real (vendorReal)                     | 146   |                       |

### file-path stability (vendor files)

| outcome                                                 | files | lines charged |
| ------------------------------------------------------- | ----- | ------------- |
| kept path, byte-identical                               | 1,612 | 0             |
| new (content absent from the prior)                     | 23    | 120           |
| kept path, nameOnly                                     | 8     | 32            |
| moved, realChange                                       | 3     | 26            |
| moved and changed a little (relocation pair, path draw) | 3     | 1,532         |
| moved, movedPath                                        | 1     | 4             |

### identifier-name stability for identical code (same content signature)

| file kind: outcome           | bindings |
| ---------------------------- | -------- |
| factory file: different name | 150      |
| factory file: same name      | 57,655   |
