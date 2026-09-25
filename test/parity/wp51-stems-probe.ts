// WP5.1 stems probe: the TS truth table for the split's file-name shaping
// (src/split/stable-split.ts BAD_STEM / hasMintedNumber / LEADING_STOPWORD
// / toKebabCase / acceptProposedName, and fossil-assign.ts stemOf).
//
// The unexported helpers are evaluated FROM THEIR OWN SOURCE TEXT (the
// regex literals and function bodies are cut out of the .ts files and
// compiled by the JS engine), so the vectors are the TS's semantics, not a
// transcription of them. Output: test/parity/wp51-stems.json.
//
//   npx tsx test/parity/wp51-stems-probe.ts > test/parity/wp51-stems.json
import fs from "node:fs";
import {
  acceptProposedName,
  toKebabCase
} from "../../src/split/stable-split.js";

const stableSrc = fs.readFileSync("src/split/stable-split.ts", "utf-8");
const fossilSrc = fs.readFileSync("src/split/fossil-assign.ts", "utf-8");

function regexLiteral(src: string, name: string): RegExp {
  const m = src.match(new RegExp(`const ${name} =\\s*\\n?\\s*(/.*/[a-z]*);`));
  if (!m) throw new Error(`regex ${name} not found`);
  return new Function(`return ${m[1]};`)() as RegExp;
}

const BAD_STEM = regexLiteral(stableSrc, "BAD_STEM");
const LEADING_STOPWORD = regexLiteral(stableSrc, "LEADING_STOPWORD");
const known = stableSrc.match(/KNOWN_NUMBER_TOKENS = new Set\(\[([^\]]*)\]\)/);
if (!known) throw new Error("KNOWN_NUMBER_TOKENS not found");
const KNOWN = new Set([...known[1].matchAll(/"(\d+)"/g)].map((m) => m[1]));
function hasMintedNumber(name: string): boolean {
  const runs = name.match(/\d+/g);
  if (!runs) return false;
  return runs.some((run) => run.length >= 2 && !KNOWN.has(run));
}
const stemBody = fossilSrc.match(
  /function stemOf\(name: string\): string \{([\s\S]*?)\n\}/
);
if (!stemBody) throw new Error("stemOf not found");
const stemOf = new Function("name", stemBody[1]) as (n: string) => string;

const names = [
  "noop",
  "noOp",
  "no_op",
  "no-ops",
  "noopFunction36",
  "noxop",
  "nop",
  "doNothing",
  "doNothing24",
  "doNothingX-",
  "silentNoop",
  "silent_noops2",
  "emptyFunction",
  "emptyFunctions3",
  "emptyCallback12",
  "emptyHandlers",
  "emptyOperationX",
  "idleOperation",
  "idle-operation7",
  "initializeModule",
  "initializeModule12",
  "placeholder",
  "placeholderValue",
  "_",
  "__",
  "_12",
  "__a",
  "reactLib48",
  "reactLib",
  "userVal",
  "userVal12",
  "val",
  "Val2",
  "xVal",
  "x-Val",
  "appInitializer17",
  "app254Initializer",
  "float64Error",
  "sha256Hasher",
  "base64Encode",
  "v8",
  "a1b22",
  "andTaskPipeline",
  "theTaskRunner",
  "inputHandler",
  "themeEngine",
  "andrewConfig",
  "a",
  "an",
  "aB",
  "anB",
  "anxiety",
  "or",
  "orange",
  "or2",
  "and",
  "butX",
  "nor",
  "isReverseDirection",
  "ABCDef",
  "XMLHttpRequest",
  "getHTTPResponseCode",
  "fooBar",
  "FooBar",
  "foo_bar",
  "foo-bar",
  "foo--bar",
  "__proto__",
  "$x",
  "_$private",
  "a1B",
  "aBC",
  "aBcD",
  "HTML5Parser",
  "é",
  "caféLatte",
  "ÉcoleNormale",
  "x_y_z",
  "-lead",
  "trail-",
  "utils",
  "Utils",
  "src",
  "index",
  "sharedState",
  "react-lib-48",
  "retry-scheduler",
  "Retry_Scheduler",
  "a-",
  "ab",
  "a",
  "1abc",
  "$ab",
  "ab$",
  "a".repeat(40),
  "a".repeat(41),
  "helpers",
  "diff-view",
  "token-bucket-",
  "x__",
  "__x__",
  "m$n",
  "mÜller",
  "straße",
  "İstanbul",
  "ǅemal",
  "ONE",
  "oneTWOThree",
  "one2Three",
  "fetch-API-data",
  "userID",
  "IDs",
  "a_1",
  "noop-handler",
  "emptycallbacks",
  "EMPTYFUNCTION",
  "NOOP",
  "PLACEHOLDER",
  "ReactLib7",
  "INITIALIZEMODULE9",
  "idle_operation",
  "silent-noop",
  "ſilentNoop",
  "Key"
];

const rows = names.map((name) => ({
  name,
  kebab: toKebabCase(name),
  accept: acceptProposedName(name),
  badStem: BAD_STEM.test(name),
  minted: hasMintedNumber(name),
  stopword: LEADING_STOPWORD.test(name),
  stemOf: stemOf(name)
}));
process.stdout.write(`${JSON.stringify(rows, null, 1)}\n`);
