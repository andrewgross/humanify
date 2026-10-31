# ref-scratch-0f338ffa-r2 — 2.1.215->2.1.216 (default mode)

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
| carryOverReconstructed                 | 1621 factories inherited by (structural hash, ordinal) at the same path (the run log's "carry-over" count is the check where a log exists)                                                                                  |
| eraVendorStatements                    | 0                                                                                                                                                                                                                           |
| factoriesWithContestedClaim            | 0                                                                                                                                                                                                                           |
| indexLabelledFreshFactories            | 0                                                                                                                                                                                                                           |
| restorationApplicationSlots            | 0                                                                                                                                                                                                                           |
| restorationApplicationSlotsInFactories | 0                                                                                                                                                                                                                           |
| eraTrailFileMapping                    | 681/707 trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); 200412 fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/ |
| eraRestorationSiteAApplied             | 0                                                                                                                                                                                                                           |
| eraRestorationFrozen                   | null                                                                                                                                                                                                                        |
| mintedCensusTotal                      | 94                                                                                                                                                                                                                          |

## 1. Where code goes (the fresh tree, .humanify/ excluded)

| area    | files | lines   | bytes      | share of lines |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 901,536 | 48,056,210 | 100.0%         |
| src/    | 4,851 | 869,254 | 41,995,540 | 96.4%          |
| vendor/ | 1,648 | 27,286  | 5,762,969  | 3.0%           |
| other   | 4     | 4,996   | 297,701    | 0.6%           |

### vendor/ by the mechanism that placed each file (exclusive)

| mechanism                                               | files | lines  | bytes     | share of vendor bytes |
| ------------------------------------------------------- | ----- | ------ | --------- | --------------------- |
| TOTAL                                                   | 1,648 | 27,286 | 5,762,969 | 100%                  |
| inherited from prior by hash (factory carry-over)       | 1,621 | 14,420 | 5,274,922 | 91.5%                 |
| index claim moved by package                            | 0     | 0      | 0         | 0.0%                  |
| unclaimed-but-clean, moved on index owner+file (exp157) | 0     | 0      | 0         | 0.0%                  |
| whole-module factory, named this run (model/declared)   | 26    | 873    | 132,925   | 2.3%                  |
| root lib\_\* fallback                                   | 0     | 0      | 0         | 0.0%                  |
| era (app) statements filed into vendor by placement     | 0     | 0      | 0         | 0.0%                  |
| the vendor record (\_bun-modules.json)                  | 1     | 11,993 | 355,122   | 6.2%                  |
| other                                                   | 0     | 0      | 0         | 0.0%                  |

### finer detail (mechanism :: label)

| mechanism :: detail                                                                 | files | lines  | bytes     |
| ----------------------------------------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over) :: carried; origin label llm      | 1,321 | 12,817 | 4,424,194 |
| inherited from prior by hash (factory carry-over) :: carried; origin label fallback | 281   | 1,504  | 531,053   |
| the vendor record (\_bun-modules.json) :: humanify's own metadata, not library code | 1     | 11,993 | 355,122   |
| inherited from prior by hash (factory carry-over) :: carried; origin label url      | 18    | 97     | 318,645   |
| whole-module factory, named this run (model/declared) :: llm, decided this run      | 17    | 847    | 122,662   |
| whole-module factory, named this run (model/declared) :: url, decided this run      | 9     | 26     | 10,263    |
| inherited from prior by hash (factory carry-over) :: carried; origin label banner   | 1     | 2      | 1,030     |

### per package (top 25 by bytes, then the long tail)

| package                         | files | lines  | bytes     | share of bytes | dominant mechanism (by bytes)                     |
| ------------------------------- | ----- | ------ | --------- | -------------- | ------------------------------------------------- |
| (vendor root)                   | 742   | 18,956 | 2,489,940 | 43.2%          | inherited from prior by hash (factory carry-over) |
| protobufjs                      | 28    | 167    | 288,711   | 5.0%           | inherited from prior by hash (factory carry-over) |
| highlight                       | 3     | 8      | 205,995   | 3.6%           | inherited from prior by hash (factory carry-over) |
| tslib                           | 15    | 82     | 165,952   | 2.9%           | inherited from prior by hash (factory carry-over) |
| react                           | 6     | 519    | 162,911   | 2.8%           | inherited from prior by hash (factory carry-over) |
| prismjs                         | 24    | 52     | 137,635   | 2.4%           | inherited from prior by hash (factory carry-over) |
| design-sync                     | 3     | 2,033  | 123,843   | 2.1%           | inherited from prior by hash (factory carry-over) |
| w3                              | 5     | 25     | 122,209   | 2.1%           | inherited from prior by hash (factory carry-over) |
| google-auth-library             | 31    | 171    | 97,975    | 1.7%           | inherited from prior by hash (factory carry-over) |
| forge-pki                       | 6     | 59     | 93,426    | 1.6%           | inherited from prior by hash (factory carry-over) |
| dom-serializer                  | 26    | 140    | 74,532    | 1.3%           | inherited from prior by hash (factory carry-over) |
| sharp                           | 10    | 70     | 66,800    | 1.2%           | inherited from prior by hash (factory carry-over) |
| aws-sdk-client-middleware       | 14    | 70     | 64,669    | 1.1%           | inherited from prior by hash (factory carry-over) |
| semver                          | 23    | 162    | 58,174    | 1.0%           | inherited from prior by hash (factory carry-over) |
| @aws-sdk/smithy-client          | 5     | 30     | 52,628    | 0.9%           | inherited from prior by hash (factory carry-over) |
| javascript                      | 14    | 28     | 50,122    | 0.9%           | inherited from prior by hash (factory carry-over) |
| esbuild                         | 2     | 868    | 48,812    | 0.8%           | inherited from prior by hash (factory carry-over) |
| css                             | 5     | 10     | 45,682    | 0.8%           | inherited from prior by hash (factory carry-over) |
| openid-client                   | 6     | 67     | 45,634    | 0.8%           | inherited from prior by hash (factory carry-over) |
| forge-tls                       | 2     | 16     | 41,722    | 0.7%           | inherited from prior by hash (factory carry-over) |
| @opentelemetry/core             | 30    | 124    | 38,668    | 0.7%           | inherited from prior by hash (factory carry-over) |
| ts-morph                        | 2     | 701    | 38,489    | 0.7%           | inherited from prior by hash (factory carry-over) |
| xmldom                          | 4     | 21     | 37,005    | 0.6%           | inherited from prior by hash (factory carry-over) |
| color-convert                   | 3     | 9      | 35,689    | 0.6%           | inherited from prior by hash (factory carry-over) |
| @opentelemetry/sdk-metrics-base | 13    | 76     | 33,003    | 0.6%           | inherited from prior by hash (factory carry-over) |
| long tail (161 packages)        | 626   | 2,822  | 1,142,743 | 19.8%          | —                                                 |

### vendor-root loose files

| mechanism                                             | files | lines  | bytes     |
| ----------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over)     | 724   | 6,116  | 2,012,156 |
| the vendor record (\_bun-modules.json)                | 1     | 11,993 | 355,122   |
| whole-module factory, named this run (model/declared) | 17    | 847    | 122,662   |

## 2. How identifiers are named

### vendor/ factory bodies (each binding zipped against the raw bundle's factory)

| name source                                                         | bindings | share |
| ------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                               | 53,218   | 100%  |
| left as bundled (minifier's name)                                   | 34,748   | 65.3% |
| carried from prior: file inherited, still the prior's minifier name | 18,001   | 33.8% |
| factory not paired to the raw bundle                                | 469      | 0.9%  |

### vendor/ era (app) statements (strategy trail, by terminal step)

| name source | bindings | share |
| ----------- | -------- | ----- |
| TOTAL       | 0        | 100%  |

### src/ (strategy trail, by terminal step) — the contrast

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 202,352  | 100%  |
| carried from prior by exact match                       | 186,608  | 92.2% |
| carried from prior by close match                       | 8,773    | 4.3%  |
| model-named                                             | 5,029    | 2.5%  |
| left as bundled (no final name, or kept the minifier's) | 1,233    | 0.6%  |
| post-split reconcile (finish rule)                      | 707      | 0.3%  |
| other (class-id-floor)                                  | 2        | 0.0%  |

### app bindings whose file could not be mapped

| name source                                                         | bindings | share |
| ------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                               | 2        | 100%  |
| left as bundled (no final name, or kept the minifier's) [generated] | 1        | 50.0% |
| post-split reconcile (finish rule) [generated]                      | 1        | 50.0% |

## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)

| bucket                                  | lines | of which mint-bearing |
| --------------------------------------- | ----- | --------------------- |
| TOTAL churnLines                        | 1,879 | 0                     |
| manifest                                | 201   | 0                     |
| name re-rolls (nameOnly+freeReroll)     | 0     | 0                     |
| moved / relocated (movedPath+relocated) | 1,538 | 0                     |
| real: changed in place or paired        | 20    | 0                     |
| real: truly added                       | 120   | 0                     |
| real: truly removed                     | 0     | 0                     |
| other (unparsed)                        | 0     | 0                     |
| = noise (vendorLn)                      | 1,739 |                       |
| = real (vendorReal)                     | 140   |                       |

### file-path stability (vendor files)

| outcome                                                 | files | lines charged |
| ------------------------------------------------------- | ----- | ------------- |
| kept path, byte-identical                               | 1,621 | 0             |
| new (content absent from the prior)                     | 23    | 120           |
| moved, realChange                                       | 3     | 20            |
| moved and changed a little (relocation pair, path draw) | 3     | 1,538         |

### identifier-name stability for identical code (same content signature)

| file kind: outcome      | bindings |
| ----------------------- | -------- |
| factory file: same name | 57,805   |
