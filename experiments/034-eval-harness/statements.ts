/**
 * Shared statement extraction for the eval harness: top-level statements of a
 * humanified single-file output, each with its rename-invariant statementHash
 * and source text. Falls back to Program body when the output is not
 * wrapper-shaped.
 */
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { statementHash } from "../../src/split/statement-hash.js";

export interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

export function statementsOf(code: string): Stmt[] {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  const body =
    wrapper && t.isBlockStatement(wrapper.functionPath.node.body)
      ? wrapper.functionPath.node.body.body
      : ast.program.body;
  return body.map((stmt) => {
    const text =
      stmt.start != null && stmt.end != null
        ? code.slice(stmt.start, stmt.end)
        : "";
    return {
      hash: statementHash(stmt),
      text,
      lines: text.length ? text.split("\n").length : 0
    };
  });
}
