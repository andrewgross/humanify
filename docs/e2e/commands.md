# E2E CLI Commands Reference

All commands are run via `npm run e2e -- <command>`.

## list

List all available test fixtures and their status.

```bash
npm run e2e -- list
```

Output:
```
Available fixtures:
  mitt (ready)
    3.0.0 → 3.0.1 (Patch release)
```

Status meanings:
- `ready` - Fixture is set up and ready for validation
- `needs setup` - Run `setup` command first

---

## setup

Download and prepare a test fixture. This clones the package repository, checks out the specified version tags, and compiles TypeScript to JavaScript.

```bash
npm run e2e -- setup <fixture>
```

**Example:**
```bash
npm run e2e -- setup mitt
```

**What it does:**
1. Clones the package repo to `.tmp-clone/`
2. For each version in `versionPairs`:
   - Checks out the git tag
   - Copies source files to `source/v{version}/`
   - Runs build command (if configured) to `build/v{version}/`

---

## validate

Run fingerprint validation on a fixture. Minifies both versions, computes ground truth, runs fingerprinting, and compares results.

```bash
npm run e2e -- validate <fixture> [v1] [v2] [options]
```

**Arguments:**
- `<fixture>` - Name of the fixture (e.g., `mitt`)
- `[v1] [v2]` - Optional: specific version pair to test. If omitted, tests all configured pairs.

**Options:**
- `--verbose` - Show detailed failure hints
- `--update-snapshot` - Save results as the new CI baseline
- `--ci` - Compare against saved snapshot, fail on any drift

**Examples:**
```bash
# Test all version pairs
npm run e2e -- validate mitt

# Test specific versions
npm run e2e -- validate mitt 3.0.0 3.0.1

# Verbose output with failure hints
npm run e2e -- validate mitt --verbose

# Update the CI baseline snapshot
npm run e2e -- validate mitt --update-snapshot

# CI mode - compare against snapshot
npm run e2e -- validate mitt --ci
```

**Output (interactive mode):**
```
┌───────────────────────────────────────────────────────┐
│  E2E Validation: mitt 3.0.0 → 3.0.1 (terser-default)  │
└───────────────────────────────────────────────────────┘

Ground Truth:
  6 functions in v1, 6 functions in v2
  4 unchanged, 0 modified, 0 added, 0 removed

Fingerprint Matching:
  Unchanged: 4/4 matched (100%)
  Modified:  N/A
  Added:     N/A

Overall: PASS (100%)
```

**Output (CI mode):**
```
E2E: mitt 3.0.0->3.0.1 (terser-default)
  Unchanged: 4/4
  Modified: 0/0
  Added: 0/0
  Overall: PASS (100%)

Snapshot comparison: MATCH
```

---

## debug

Investigate a specific function's fingerprinting results. Shows ground truth data, body hashes, and any failure artifacts.

```bash
npm run e2e -- debug <fixture> <v1> <v2> --function <name>
```

**Arguments:**
- `<fixture>` - Name of the fixture
- `<v1> <v2>` - Version pair
- `--function <name>` - Name of the function to investigate

**Example:**
```bash
npm run e2e -- debug mitt 3.0.0 3.0.1 --function emit
```

**Output:**
```
Function: emit
Change type: unchanged
Source file: index.js

V1 Function:
  Location: lines 56-73
  Arity: 2
  Body hash: 8f918e8b009ccb1e

V2 Function:
  Location: lines 56-73
  Arity: 2
  Body hash: 8f918e8b009ccb1e

No failures recorded for this function.

Fingerprint data stored in:
  V1: test/e2e/output/mitt/.../debug/v1-fingerprints.json
  V2: test/e2e/output/mitt/.../debug/v2-fingerprints.json
```

**Prerequisites:**
- Run `validate` first to generate debug artifacts

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0    | All validations passed |
| 1    | One or more failures, or snapshot drift in CI mode |

---

## Environment Variables

None currently. All configuration is in `fixture.config.json` files.
