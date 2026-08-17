## 2.1.86 — fossil-structured tree preview

| | fossil layout (preview) | current layout |
|---|---|---|
| app files | **3273** (+1 bootstrap for 4 eager stmts) | 1528 |
| folders | 280 | 103 |
| statements/file | min 1 · p25 2 · median 4 · p75 7 · p95 17 · max 220 | (ledger-equivalent: 13.1 avg if same statements) |
| lines/file | min 3 · p25 13 · median 49 · p75 143 · p95 517 · max 6996 | min 7 · p25 62 · median 130 · p75 333 · p95 1201 · max 3035 |
| files/folder | min 3 · p25 3 · median 4 · p75 8 · p95 21 · max 1085 | min 6 · p25 8 · median 15 · p75 21 · p95 25 · max 26 |
| name sources | 3092 module-match carry · 127 fresh-named · 54 ambiguous twins (fresh-named, flagged) | n/a |

folder-signal census (modules placed by each signal, ladder order): barrel 483 · anchor 288 · dominant-importer 1311 · co-importer 604 · **flat residue 587**

entry-distance layering (import depth from roots): cyclic:45 · L0:150 · L1:503 · L2:924 · L3:568 · L4:412 · L5:356 · L6:145 · L7:65 · L8:45 · L9:30 · L10:12 · L11:8 · L12:3 · L13:2 · L14:1 · L15:1 · L16:3

largest folders (files · dominant signal · naming prior):

- `src/` — 1085 · flat · mixed (top 2%)
- `src/extract-pull-request-number-from-string/render-transcript-toggle/` — 96 · dominant-importer · → carry `src/extract-pull-request-number-from-string/` (79%)
- `src/get-skills/` — 95 · dominant-importer · → carry `src/get-skills/` (93%)
- `src/batch-delete-eval-job-endpoint-plugin-shared/` — 79 · co-importer · → carry `src/batch-delete-eval-job-endpoint-plugin-shared/` (97%)
- `src/extract-pull-request-number-from-string/` — 43 · dominant-importer · → carry `src/extract-pull-request-number-from-string/` (63%)
- `src/locale-function-map/` — 41 · barrel · → carry `src/locale-function-map/` (70%)
- `src/initialize-app109/` — 39 · barrel · → carry `src/initialize-app109/` (71%)
- `src/create-child-element/` — 39 · dominant-importer · → carry `src/create-child-element/` (72%)
- `src/credential-provider-chain/` — 35 · dominant-importer · → carry `src/credential-provider-chain/` (97%)
- `src/extract-pull-request-number-from-string/build-render-command/` — 30 · dominant-importer · → carry `src/extract-pull-request-number-from-string/` (78%)
- `src/display-message-component/` — 27 · dominant-importer · mixed (top 59%)
- `src/cognito-identity-endpoint-plugin-shared/` — 25 · co-importer · → carry `src/cognito-identity-endpoint-plugin-shared/` (85%)
- `src/clone-deep-recursive/` — 24 · co-importer · → carry `src/clone-deep-recursive/` (71%)
- `src/apply-onboarding-completion/` — 24 · co-importer · mixed (top 35%)
- `src/is-valid-keyword/` — 21 · dominant-importer · → carry `src/is-valid-keyword/` (74%)
- `src/initialize-app135/` — 21 · barrel · → carry `src/initialize-app135/` (100%)
- `src/anthropic-module-exports/` — 19 · dominant-importer · → carry `src/anthropic-module-exports/` (84%)
- `src/is-session-expired-error/` — 19 · dominant-importer · → carry `src/is-session-expired-error/` (73%)
- `src/add-to-set-with-limit/` — 19 · co-importer · mixed (top 45%)
- `src/initialize-all-paginators/` — 18 · barrel · → carry `src/initialize-all-paginators/` (100%)

largest files (lines):

- `src/csharp-claude-api-doc.js` — 6996 lines, 56 statements
- `src/handle-command/get-resolved-promise.js` — 3839 lines, 83 statements
- `src/zod-schema/parse-context.js` — 3622 lines, 25 statements
- `src/extract-pull-request-number-from-string/render-transcript-toggle.js` — 3199 lines, 21 statements
- `src/get-teammate-message-formatter.js` — 3179 lines, 131 statements
- `src/is-valid-message-type.js` — 3001 lines, 126 statements
- `src/add-to-set-with-limit/add-to-set-with-limit.js` — 2995 lines, 27 statements
- `src/get-session-end-hooks-timeout/get-session-end-hooks-timeout.js` — 2868 lines, 67 statements

