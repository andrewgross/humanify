/**
 * WPB.4 probe: the `--stats-json` SHAPE, read from the TS SOURCE by the
 * TypeScript checker — not from a sample file, which can only show the
 * fields one run happened to set.
 *
 * The type of the `stats` object literal in `writeEvalStats`
 * (src/commands/unified.ts) is expanded recursively: every object type
 * becomes its properties in DECLARATION order (the order JSON.stringify
 * emits for an object literal built in that order — the Rust writer's key
 * order is compared against this list), each with its optionality and
 * value kind. Index signatures (Record<string, number>) become `map`.
 * `undefined` members make a property optional (JSON.stringify drops it);
 * `null` members are recorded as nullable.
 *
 *   npx tsx test/parity/wpb4-stats-schema-probe.ts > test/parity/wpb4-stats-schema.json
 */
import path from "node:path";
import ts from "typescript";

type Schema =
  | { kind: "number" | "string" | "boolean" }
  | { kind: "array"; items: Schema }
  | { kind: "map"; values: Schema }
  | {
      kind: "object";
      props: Array<{
        name: string;
        optional: boolean;
        nullable: boolean;
        type: Schema;
      }>;
    }
  | { kind: "union"; of: string[] };

const root = path.resolve(import.meta.dirname, "../..");
const file = path.join(root, "src/commands/unified.ts");
const config = ts.readConfigFile(
  path.join(root, "tsconfig.json"),
  ts.sys.readFile
);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
const program = ts.createProgram([file], parsed.options);
const checker = program.getTypeChecker();
const source = program.getSourceFile(file);
if (!source) throw new Error("unified.ts not in program");

let statsType: ts.Type | undefined;
function find(node: ts.Node): void {
  if (
    ts.isFunctionDeclaration(node) &&
    node.name?.text === "writeEvalStats" &&
    node.body
  ) {
    for (const st of node.body.statements) {
      if (!ts.isVariableStatement(st)) continue;
      for (const d of st.declarationList.declarations) {
        if (ts.isIdentifier(d.name) && d.name.text === "stats") {
          statsType = checker.getTypeAtLocation(d.name);
        }
      }
    }
  }
  ts.forEachChild(node, find);
}
find(source);
if (!statsType) throw new Error("writeEvalStats' stats literal not found");

function strip(t: ts.Type): {
  core: ts.Type[];
  optional: boolean;
  nullable: boolean;
} {
  const parts = t.isUnion() ? t.types : [t];
  let optional = false;
  let nullable = false;
  const core: ts.Type[] = [];
  for (const p of parts) {
    if (p.flags & ts.TypeFlags.Undefined) optional = true;
    else if (p.flags & ts.TypeFlags.Null) nullable = true;
    else core.push(p);
  }
  return { core, optional, nullable };
}

function describe(t: ts.Type, depth: number): Schema {
  if (depth > 12) throw new Error("schema too deep");
  const { core } = strip(t);
  if (core.length > 1) {
    // A union of literals (e.g. "init" | "assignment") or booleans.
    if (core.every((c) => c.flags & ts.TypeFlags.BooleanLiteral)) {
      return { kind: "boolean" };
    }
    if (core.every((c) => c.isStringLiteral())) {
      return {
        kind: "union",
        of: core.map((c) => (c as ts.StringLiteralType).value).sort()
      };
    }
    throw new Error(`unsupported union ${checker.typeToString(t)}`);
  }
  const c = core[0];
  if (!c) throw new Error(`empty type ${checker.typeToString(t)}`);
  if (c.flags & (ts.TypeFlags.Number | ts.TypeFlags.NumberLiteral)) {
    return { kind: "number" };
  }
  if (c.flags & (ts.TypeFlags.String | ts.TypeFlags.StringLiteral)) {
    return { kind: "string" };
  }
  if (c.flags & (ts.TypeFlags.Boolean | ts.TypeFlags.BooleanLiteral)) {
    return { kind: "boolean" };
  }
  if (checker.isArrayType(c)) {
    const [item] = checker.getTypeArguments(c as ts.TypeReference);
    return { kind: "array", items: describe(item, depth + 1) };
  }
  const index = checker.getIndexInfosOfType(c);
  const props = checker.getPropertiesOfType(c);
  if (index.length > 0 && props.length === 0) {
    return { kind: "map", values: describe(index[0].type, depth + 1) };
  }
  return {
    kind: "object",
    props: props.map((p) => {
      const pt = checker.getTypeOfSymbol(p);
      const s = strip(pt);
      return {
        name: p.name,
        optional: s.optional || (p.flags & ts.SymbolFlags.Optional) !== 0,
        nullable: s.nullable,
        type: describe(pt, depth + 1)
      };
    })
  };
}

process.stdout.write(`${JSON.stringify(describe(statsType, 0), null, 2)}\n`);
