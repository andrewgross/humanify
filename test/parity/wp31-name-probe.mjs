// WP3.1 probe: the name predicates validated rename consumes, run on the
// REAL TS functions (lesson 3 — never port a predicate as its name reads).
//
//   RESERVED_WORDS / GLOBAL_BUILTINS   (src/llm/validation.ts — the latter
//                                       derived from the `globals` package)
//   isValidIdentifier                  (src/llm/validation.ts)
//   isValidRenameTarget                (src/rename/validated-rename.ts)
//   isBunToken / isDecoratedDescriptive / isBelowFloorName
//                                      (src/rename/minted-census.ts)
//   createIsEligible()                 (src/rename/rename-eligibility.ts)
//
// Run: npx tsx test/parity/wp31-name-probe.mjs > test/parity/wp31-names.json
const { GLOBAL_BUILTINS, RESERVED_WORDS, isValidIdentifier } = await import(
  "../../src/llm/validation.js"
);
const { isValidRenameTarget } = await import(
  "../../src/rename/validated-rename.js"
);
const { isBunToken, isDecoratedDescriptive, isBelowFloorName } = await import(
  "../../src/rename/minted-census.js"
);
const { createIsEligible } = await import(
  "../../src/rename/rename-eligibility.js"
);

const NAMES = [
  "",
  "a",
  "_",
  "$",
  "__",
  "___",
  "a1",
  "1a",
  "foo-bar",
  "foo bar",
  "é",
  "naïve",
  "aé",
  "delete",
  "class",
  "arguments",
  "eval",
  "undefined",
  "NaN",
  "Infinity",
  "let",
  "yield",
  "await",
  "async",
  "of",
  "get",
  "set",
  "static",
  "Map",
  "Promise",
  "document",
  "window",
  "Bun",
  "process",
  "require",
  "module",
  "exports",
  "event",
  "status",
  "name",
  "length",
  "__esm",
  "__commonJS",
  "__esModule",
  "__c",
  "__t",
  "__ab",
  "__abc",
  "__abc_d",
  "__ab_c",
  "__a$b",
  "__aBc",
  "__Abc",
  "__a1",
  "__a12",
  "__webpack_require__",
  "_interop_require_default",
  "_ts_generator",
  "_foo",
  "_foo_bar",
  "_foo_Bar",
  "_foo_bar_",
  "_myHelper",
  "fs",
  "os",
  "x",
  "q7",
  "ab",
  "abc",
  "A9_",
  "fsPromises_",
  "initializeApp_",
  "initializeApp__",
  "T7Class",
  "do7Function",
  "sm6Factory",
  "h06Result",
  "j3lResult",
  "sha256Hash",
  "sha256",
  "v8Engine",
  "h1Regex",
  "h1",
  "h1_",
  "b64Flag",
  "MAX_SIZE",
  "A_B",
  "A_",
  "e164Number",
  "k8sClient",
  "x509Cert",
  "iIn",
  "$a",
  "a$",
  "a_",
  "ab_",
  "ab1",
  "Ab1",
  "AB",
  "ID",
  "DB",
  "db",
  "utf8",
  "utf8Decoder",
  "base64Url",
  "it2",
  "it2x",
  "v1Api",
  "v1",
  "x0",
  "x0Pos",
  "fnX",
  "LZ77Compressor",
  "P2PConnection",
  "zO_",
  "Z9",
  "ZZ",
  "z_z",
  "HELLO",
  "Hello",
  "i",
  "j",
  "n",
  "e",
  "t",
  "cb",
  "fn",
  "sep",
  "cwd",
  "createEventEmitter",
  "dirPath",
  "counter",
  "fetchCount"
];

const eligible = createIsEligible();
const eligibleBun = createIsEligible("bun", "bun");
const out = {
  reservedWords: [...RESERVED_WORDS].sort(),
  globalBuiltins: [...GLOBAL_BUILTINS].sort(),
  names: NAMES.map((name) => ({
    name,
    isValidIdentifier: isValidIdentifier(name),
    isValidRenameTarget: isValidRenameTarget(name),
    isBunToken: isBunToken(name),
    isDecoratedDescriptive: isDecoratedDescriptive(name),
    isBelowFloorName: isBelowFloorName(name),
    eligible: eligible(name),
    eligibleBun: eligibleBun(name)
  }))
};
console.log(JSON.stringify(out, null, 1));
