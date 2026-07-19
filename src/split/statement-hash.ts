/**
 * Rename-invariant structural hash of wrapper-body statements — the
 * content-identity key for the split's hash-keyed file inheritance.
 *
 * Motivation (walk measurement, 2026-07-18): when upstream reorders the
 * bundle (2.1.85->86 shuffled 35% of statement order), name votes miss for
 * any statement whose declared names flipped (LLM rename noise) and the
 * neighbor-following residue rule scatters byte-identical code into other
 * files. Content identity is order-free and name-free, so it survives both.
 *
 * The hash covers node types, tree shape, literals, operators and
 * declaration kinds; every identifier NAME is masked. Humanify renames
 * bindings AND export-object member names, so property identifiers are
 * masked too — two statements hash equal iff they are the same code modulo
 * renaming. Consequence: short generic statements (`foo();`) collide across
 * unrelated code; the inheritance tier compensates with an equal-count
 * unanimity rule (stable-split.ts).
 */

import { createHash } from "node:crypto";
import * as t from "@babel/types";

/** Bump when the serialization below changes shape. A prior ledger hashed
 * under a different version is ignored by the inheritance tier (it just
 * stays off for that one hop), never misread. */
export const STATEMENT_HASH_VERSION = 1;

/** The value-bearing part of a node — everything that distinguishes two
 * structurally-identical trees. Identifier names deliberately absent. */
function nodeContent(node: t.Node): string {
  switch (node.type) {
    case "StringLiteral":
    case "DirectiveLiteral":
    case "BigIntLiteral":
      return node.value;
    case "NumericLiteral":
    case "BooleanLiteral":
      return String(node.value);
    case "RegExpLiteral":
      return `${node.pattern}/${node.flags}`;
    case "TemplateElement":
      return node.value.raw;
    case "VariableDeclaration":
      return node.kind;
    case "BinaryExpression":
    case "LogicalExpression":
    case "AssignmentExpression":
    case "UnaryExpression":
      return node.operator;
    case "UpdateExpression":
      return `${node.operator}${node.prefix ? "pre" : "post"}`;
    case "MemberExpression":
    case "OptionalMemberExpression":
    case "ObjectProperty":
    case "ObjectMethod":
    case "ClassMethod":
    case "ClassProperty":
      return node.computed ? "computed" : "";
    default:
      return "";
  }
}

type StackItem = t.Node | "close" | "hole";

function isNode(value: unknown): value is t.Node {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { type?: unknown }).type === "string"
  );
}

/** Children in visitor-key order, reversed for the LIFO stack so they pop
 * left-to-right. Array holes (`[1, , 2]`) get an explicit marker so they
 * cannot alias the hole-free spelling. */
function pushChildren(stack: StackItem[], child: unknown): void {
  if (Array.isArray(child)) {
    for (let i = child.length - 1; i >= 0; i--) {
      if (child[i] === null) stack.push("hole");
      else pushChildren(stack, child[i]);
    }
    return;
  }
  if (isNode(child)) stack.push(child);
}

/** Hash one statement. Iterative (explicit stack) so the multi-thousand-line
 * function statements real bundles contain cannot overflow the JS stack. */
export function statementHash(stmt: t.Statement): string {
  const hash = createHash("sha256");
  const stack: StackItem[] = [stmt];
  while (stack.length > 0) {
    const item = stack.pop() as StackItem;
    if (item === "close") {
      hash.update(")");
      continue;
    }
    if (item === "hole") {
      hash.update("_");
      continue;
    }
    hash.update(`(${item.type}\x00${nodeContent(item)}\x00`);
    stack.push("close");
    const keys = t.VISITOR_KEYS[item.type] ?? [];
    for (let k = keys.length - 1; k >= 0; k--) {
      pushChildren(
        stack,
        (item as unknown as Record<string, unknown>)[keys[k]]
      );
    }
  }
  return hash.digest("hex").slice(0, 16);
}
