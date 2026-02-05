# Adding New Test Fixtures

This guide explains how to add new packages to the E2E validation harness.

## Choosing a Good Test Package

Ideal packages are:
- **Small** (under 1000 LOC) - faster iteration
- **Pure library** - no external dependencies to mock
- **Multiple releases** - need at least two versions to compare
- **Clear function boundaries** - named exports, not just one big IIFE
- **TypeScript or JavaScript** - must be parseable by Babel

Good candidates:
- `mitt` (~100 LOC) - tiny event emitter
- `nanoid` (~200 LOC) - unique ID generator
- `zustand` (~500 LOC) - state management
- `preact` (~3k LOC) - React alternative

## Creating the Fixture

### 1. Create the fixture directory

```bash
mkdir -p test/e2e/fixtures/<package-name>
```

### 2. Create fixture.config.json

```json
{
  "package": "<package-name>",
  "repo": "https://github.com/<owner>/<repo>",
  "sourceStrategy": {
    "type": "git-tag",
    "tagPattern": "v{version}"
  },
  "entryPoints": ["src/index.ts"],
  "buildCommand": "npx tsc src/index.ts --outDir build --module esnext --target es2020 --moduleResolution node",
  "versionPairs": [
    {
      "v1": "1.0.0",
      "v2": "1.0.1",
      "description": "Patch release"
    }
  ]
}
```

### 3. Create .gitignore

```bash
echo -e ".tmp-clone/\nsource/\nbuild/\nminified/" > test/e2e/fixtures/<package-name>/.gitignore
```

## Configuration Reference

### package
Name of the package (used in CLI commands and output).

### repo
Git URL for cloning. HTTPS URLs recommended.

### sourceStrategy

How to find each version in the git repo.

**Git tags (most common):**
```json
{
  "type": "git-tag",
  "tagPattern": "v{version}"
}
```
The `{version}` placeholder is replaced with the version number. Common patterns:
- `v{version}` → `v1.0.0`
- `{version}` → `1.0.0`
- `release-{version}` → `release-1.0.0`

**Git commits (for packages without clean tags):**
```json
{
  "type": "git-commit",
  "commits": {
    "1.0.0": "abc123...",
    "1.0.1": "def456..."
  }
}
```

### entryPoints

Array of file paths (relative to repo root) to extract and process.

```json
"entryPoints": ["src/index.ts"]
```

For multi-file packages:
```json
"entryPoints": ["src/core.ts", "src/utils.ts"]
```

### buildCommand (optional)

Shell command to compile TypeScript or bundle the code. Run from the build directory.

**TypeScript compilation:**
```json
"buildCommand": "npx tsc src/index.ts --outDir build --module esnext --target es2020 --moduleResolution node"
```

**No build needed (pure JS):**
Omit the `buildCommand` field entirely.

### versionPairs

Array of version pairs to compare. Each pair generates a separate validation run.

```json
"versionPairs": [
  {
    "v1": "3.0.0",
    "v2": "3.0.1",
    "description": "Patch release - bug fixes only"
  },
  {
    "v1": "3.0.0",
    "v2": "4.0.0",
    "description": "Major release - breaking changes"
  }
]
```

## Testing Your Fixture

```bash
# Set up the fixture
npm run e2e -- setup <package-name>

# Verify it lists correctly
npm run e2e -- list

# Run validation
npm run e2e -- validate <package-name> --verbose

# If it passes, create the baseline snapshot
npm run e2e -- validate <package-name> --update-snapshot
```

## Troubleshooting

### "Entry point not found"

The file path in `entryPoints` doesn't exist in the repo at that version. Check:
- Is the path correct? (case-sensitive)
- Did the file exist at that version tag?
- Is the file in a subdirectory?

### "Failed to parse"

Babel couldn't parse the source file. Check:
- Is it valid TypeScript/JavaScript?
- Does it need additional Babel plugins?
- Is it using syntax too new for our parser?

### "No functions found in ground truth"

The AST extraction found no named functions. Check:
- Does the code use named function declarations/expressions?
- Are functions exported, or all anonymous?
- Is the entry point the right file?

### Build command fails

The TypeScript/bundler command failed. Check:
- Is the command correct for this package?
- Are there missing dependencies?
- Run the command manually in the build dir to debug.

## Example: Adding nanoid

```bash
# Create directory
mkdir -p test/e2e/fixtures/nanoid

# Create config
cat > test/e2e/fixtures/nanoid/fixture.config.json << 'EOF'
{
  "package": "nanoid",
  "repo": "https://github.com/ai/nanoid",
  "sourceStrategy": {
    "type": "git-tag",
    "tagPattern": "{version}"
  },
  "entryPoints": ["index.js"],
  "versionPairs": [
    {
      "v1": "4.0.0",
      "v2": "4.0.1",
      "description": "Patch release"
    }
  ]
}
EOF

# Create gitignore
echo -e ".tmp-clone/\nsource/\nbuild/\nminified/" > test/e2e/fixtures/nanoid/.gitignore

# Set up and test
npm run e2e -- setup nanoid
npm run e2e -- validate nanoid --verbose
```
