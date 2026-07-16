/**
 * The single definition of a Bun CJS factory declarator's SHAPE and of
 * what makes a name fit to be a vendor FILENAME.
 *
 * Three consumers previously each carried their own copy and drifted:
 * bun-module-classification (rename layer) matched the shape without a
 * param check, cluster-assign (split layer) matched single-declarator
 * statements only — so comma-joined factories vendored on one path and
 * landed in src/ on the other — and the Bun unpack adapter wrote raw
 * minified factory vars as filenames (vendor/H.js). Shape and floor live
 * HERE so a fix in one consumer can never miss the others.
 */

import { createHash } from "node:crypto";
import * as t from "@babel/types";

export interface FactoryCall {
  binding: string;
  callee: string;
  /** Params of the wrapped function: CJS factories take (exports, module),
   * ESM inits take none — callers decide the policy. */
  paramCount: number;
}

/** `X = CALLEE(fn, ...)` declarator shape (Bun CJS factory / ESM init). */
export function factoryCallOf(decl: t.VariableDeclarator): FactoryCall | null {
  if (!t.isIdentifier(decl.id)) return null;
  const init = decl.init;
  if (!init || !t.isCallExpression(init)) return null;
  if (!t.isIdentifier(init.callee)) return null;
  const arg0 = init.arguments[0];
  if (
    !arg0 ||
    (!t.isArrowFunctionExpression(arg0) && !t.isFunctionExpression(arg0))
  ) {
    return null;
  }
  return {
    binding: decl.id.name,
    callee: init.callee.name,
    paramCount: arg0.params.length
  };
}

/** A binding fit to become a vendor filename: at least 3 identifier
 * chars. Anything shorter is minified residue (H, qA) that must never
 * name a file. */
export function isVendorWorthyBinding(name: string): boolean {
  return /^[A-Za-z_$][A-Za-z0-9_$]{2,}$/.test(name);
}

/** Drop a trailing ".js" from a package/file stem so appending the real
 * extension can never yield highlight.js.js. */
export function stripJsExtension(name: string): string {
  return name.replace(/\.js$/i, "");
}

/** Vendor file stem from an UNTRUSTED candidate (a binding or raw factory
 * var): trailing ".js" stripped, and minified residue floored to
 * lib_<sha256(bodyText)[:8]> — the same fallback family the naming
 * cascade uses. */
export function vendorStemFor(candidate: string, bodyText: string): string {
  const stem = stripJsExtension(candidate);
  if (isVendorWorthyBinding(stem)) return stem;
  const hash = createHash("sha256").update(bodyText).digest("hex").slice(0, 8);
  return `lib_${hash}`;
}
