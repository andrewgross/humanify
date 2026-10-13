// Canonical serializer for minified JS: a ladder of normalisation levels.
//
//   L1  AST preorder, identifiers alpha-renamed by first occurrence (globals and property names kept),
//       literals by their RAW text (so 'a' vs "a", !0 vs true, 1e3 vs 1000 differ).
//   L2  L1 + literal-level normalisation: literals by value, !0/!1 -> true/false, void 0 -> undefined,
//       1/0 -> Infinity, template w/o expressions -> string, var/let/const ignored, "use strict" dropped,
//       === -> ==, !== -> !=, `new Error(..)` (builtin ctors) -> call, object keys/members by name,
//       method/shorthand flags ignored.
//   L3  L2 + structural normalisation: arrow -> function (expr body -> return), statement lists flattened
//       (comma sequences split, `a&&b;` / `a||b;` / `a?b:c;` -> if, `if((x,y))` split, return/throw sequences
//       split, blocks flattened, multi-declarator var split), `if(a)if(b)` merged, `if(!a)X else Y` swapped,
//       `return a?b:c` -> if/return, else-after-return lifted, literal/typeof operand put on the right
//       (yoda), `return undefined` -> `return`.
//   SK  "content skeleton": the ordered list of things a minifier never renames — string/number/regex
//       literals, property names, global names. BAG = the sorted multiset of SK.
//
// Exported: canonUnit(node, src) -> per-level token arrays + function sub-ranges.

import { createHash } from 'node:crypto';

export const GLOBALS = new Set(`Object Array Function String Number Boolean Symbol BigInt Math JSON Date RegExp Error
TypeError RangeError SyntaxError ReferenceError EvalError URIError AggregateError Promise Proxy Reflect Map Set WeakMap
WeakSet WeakRef FinalizationRegistry ArrayBuffer SharedArrayBuffer DataView Int8Array Uint8Array Uint8ClampedArray
Int16Array Uint16Array Int32Array Uint32Array Float32Array Float64Array BigInt64Array BigUint64Array Atomics Intl
globalThis undefined NaN Infinity isNaN isFinite parseInt parseFloat encodeURIComponent decodeURIComponent encodeURI
decodeURI escape unescape eval arguments require module exports process Buffer console setTimeout clearTimeout
setInterval clearInterval setImmediate clearImmediate queueMicrotask structuredClone TextEncoder TextDecoder URL
URLSearchParams AbortController AbortSignal fetch Headers Request Response FormData Blob File ReadableStream
WritableStream TransformStream Event EventTarget performance crypto navigator window document self global
__dirname __filename atob btoa WebAssembly Bun Deno`.split(/\s+/));

const NEW_TO_CALL = new Set(`Error TypeError RangeError SyntaxError ReferenceError EvalError URIError AggregateError
Array Object Function RegExp`.split(/\s+/));

const FN_TYPES = new Set(['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression']);
const CLASS_TYPES = new Set(['ClassDeclaration', 'ClassExpression']);
const SKIP_KEYS = new Set(['type', 'start', 'end', 'loc', 'range', 'raw', 'sourceType', 'shorthand', 'method', '_o']);
const COMMUTE = new Set(['==', '!=', '===', '!==', '&', '|', '^']);
const FLIP = { '<': '>', '>': '<', '<=': '>=', '>=': '<=' };

let OFF = new Set();
const on = (x) => !OFF.has(x);
export const TRANSFORMS = ['bool','void0','inf','template','lit-value','decl-kind','use-strict','eqeq','new-call','key-norm','arrow','seq-split','logical-stmt','cond-stmt','var-split','if-seq','if-not-swap','if-merge','else-lift','ret-seq','ret-undef','ret-ternary','yoda','cond-not-swap','sorted-keys','class-expr','catch-param','while-for','if-neq-swap','for-init-lift','trailing-exit','seq0-callee','nullish','early-return','not-eq','fn-anon','private-alpha','jump-merge','early-continue','unused-param','return-void'];
const lit = (value, o) => ({ type: 'Literal', value, start: o.start, end: o.end, _o: o });
const ident = (name, o) => ({ type: 'Identifier', name, start: o.start, end: o.end, _o: o });

// ---------- L2/L3 literal-level helpers (pure; used by the serializer) ----------
function isVoid0(n) {
  return n && n.type === 'UnaryExpression' && n.operator === 'void' && n.argument.type === 'Literal' && n.argument.value === 0;
}
function boolOf(n) {
  if (n && n.type === 'UnaryExpression' && n.operator === '!' && n.argument.type === 'Literal' && typeof n.argument.value === 'number') {
    return n.argument.value === 0;
  }
  return null;
}
function rank(n) {
  if (n && n.type === 'UnaryExpression' && n.operator === 'typeof') return 1;
  return isLiteralish(n) ? 2 : 0;
}
function isLiteralish(n) {
  if (!n) return false;
  if (n.type === 'Literal') return true;
  if (n.type === 'UnaryExpression' && n.operator === 'void') return true;
  if (n.type === 'UnaryExpression' && n.operator === '-' && n.argument.type === 'Literal') return true;
  if (boolOf(n) !== null) return true;
  if (n.type === 'Identifier' && (n.name === 'undefined' || n.name === 'Infinity' || n.name === 'NaN')) return true;
  if (n.type === 'TemplateLiteral' && n.expressions.length === 0) return true;
  return false;
}

// ---------- L3 structural rewrite (returns a new tree; original nodes reachable via _o) ----------
function stmtList(list, out, ctx) {
  for (const s of list) pushStmt(s, out);
  if (on('jump-merge')) out = jumpMerge(out);
  if (ctx === 'fn' || ctx === 'loop') return tailFix(out, ctx);
  return out;
}
function jumpKey(s) {
  if (s.type !== 'IfStatement' || s.alternate || s.consequent.body.length !== 1) return null;
  const j = s.consequent.body[0];
  if (j.type === 'ContinueStatement' && !j.label) return 'C';
  if (j.type === 'BreakStatement' && !j.label) return 'B';
  if (j.type !== 'ReturnStatement') return null;
  const a = j.argument;
  if (!a) return 'R';
  const b = boolOf(a);
  if (b !== null) return 'R' + b;
  if (a.type === 'Literal') return 'R' + JSON.stringify(a.value);
  return null;
}
// if(a)return!1;if(b)return!1  ->  if(a||b)return!1
function jumpMerge(out) {
  const res = [];
  for (const s of out) {
    const prev = res[res.length - 1];
    const k = jumpKey(s);
    if (k && prev && jumpKey(prev) === k) {
      res[res.length - 1] = { ...prev, test: { type: 'LogicalExpression', operator: '||', left: prev.test, right: s.test, start: prev.start, end: s.end } };
    } else res.push(s);
  }
  return res;
}
function negate(t) {
  if (t.type === 'UnaryExpression' && t.operator === '!') return t.argument;
  return rw({ type: 'UnaryExpression', operator: '!', prefix: true, argument: t, start: t.start, end: t.end });
}
// function-body / loop-body list rewrites that need to see what follows
function tailFix(out, ctx) {
  if (on('trailing-exit') && out.length) {
    const last = out[out.length - 1];
    if (ctx === 'fn' && last.type === 'ReturnStatement' && !last.argument) out.pop();
    else if (ctx === 'loop' && last.type === 'ContinueStatement' && !last.label) out.pop();
  }
  if ((ctx === 'fn' && on('early-return')) || (ctx === 'loop' && on('early-continue'))) {
    const jt = ctx === 'fn' ? 'ReturnStatement' : 'ContinueStatement';
    for (let i = 0; i < out.length - 1; i++) {
      const s = out[i];
      if (s.type === 'IfStatement' && !s.alternate && s.consequent.body.length === 1 && s.consequent.body[0].type === jt && !s.consequent.body[0].argument && !s.consequent.body[0].label) {
        const rest = tailFix(out.slice(i + 1), ctx);
        const ifs = { type: 'IfStatement', test: negate(s.test), consequent: { type: 'BlockStatement', body: rest, start: s.start, end: s.end }, alternate: null, start: s.start, end: s.end, _o: s._o };
        return out.slice(0, i).concat([ifs]);
      }
    }
  }
  return out;
}
function asStmt(e, o) {
  return { type: 'ExpressionStatement', expression: e, start: o.start, end: o.end, _o: o };
}
function pushStmt(s, out) {
  if (!s) return;
  switch (s.type) {
    case 'EmptyStatement':
      return;
    case 'BlockStatement':
      stmtList(s.body, out);
      return;
    case 'ExpressionStatement': {
      if (s.directive === 'use strict' && on('use-strict')) return;
      const e = s.expression;
      if (e.type === 'SequenceExpression' && on('seq-split')) {
        for (const x of e.expressions) pushStmt(asStmt(x, x), out);
        return;
      }
      if (on('logical-stmt') && e.type === 'LogicalExpression' && (e.operator === '&&' || e.operator === '||')) {
        const test = e.operator === '&&' ? e.left : { type: 'UnaryExpression', operator: '!', prefix: true, argument: e.left, start: e.start, end: e.end };
        pushStmt({ type: 'IfStatement', test, consequent: asStmt(e.right, e.right), alternate: null, start: s.start, end: s.end, _o: s }, out);
        return;
      }
      if (on('cond-stmt') && e.type === 'ConditionalExpression') {
        pushStmt({ type: 'IfStatement', test: e.test, consequent: asStmt(e.consequent, e.consequent), alternate: asStmt(e.alternate, e.alternate), start: s.start, end: s.end, _o: s }, out);
        return;
      }
      out.push(rwFields(s));
      return;
    }
    case 'VariableDeclaration': {
      if (on('class-expr') && s.declarations.length === 1 && s.declarations[0].init && s.declarations[0].init.type === 'ClassExpression' && s.declarations[0].id.type === 'Identifier') {
        const c = s.declarations[0].init;
        out.push(rwFields({ type: 'ClassDeclaration', id: s.declarations[0].id, superClass: c.superClass, body: c.body, start: c.start, end: c.end, _o: c }));
        return;
      }
      if (s.declarations.length > 1 && on('var-split')) {
        for (const d of s.declarations) out.push(rwFields({ type: 'VariableDeclaration', kind: s.kind, declarations: [d], start: d.start, end: d.end, _o: s }));
        return;
      }
      out.push(rwFields(s));
      return;
    }
    case 'IfStatement': {
      let test = s.test;
      if (test.type === 'SequenceExpression' && on('if-seq')) {
        const xs = test.expressions;
        for (let i = 0; i < xs.length - 1; i++) pushStmt(asStmt(xs[i], xs[i]), out);
        test = xs[xs.length - 1];
      }
      let cons = s.consequent;
      let alt = s.alternate;
      // if(!a) X else Y  ->  if(a) Y else X
      if (on('if-not-swap') && alt && test.type === 'UnaryExpression' && test.operator === '!') {
        test = test.argument;
        [cons, alt] = [alt, cons];
      }
      if (on('if-neq-swap') && alt && test.type === 'BinaryExpression' && (test.operator === '!=' || test.operator === '!==')) {
        test = { ...test, operator: test.operator === '!=' ? '==' : '===' };
        [cons, alt] = [alt, cons];
      }
      // if(a) if(b) X  ->  if(a&&b) X
      const consList = stmtList([cons], []);
      if (on('if-merge') && !alt && consList.length === 1 && consList[0].type === 'IfStatement' && !consList[0].alternate) {
        const inner = consList[0];
        out.push({ type: 'IfStatement', test: rw({ type: 'LogicalExpression', operator: '&&', left: test, right: inner.test, start: s.start, end: s.end }), consequent: inner.consequent, alternate: null, start: s.start, end: s.end, _o: s });
        return;
      }
      const cBlock = { type: 'BlockStatement', body: consList, start: cons.start, end: cons.end };
      const exits = consList.length && ['ReturnStatement', 'ThrowStatement', 'ContinueStatement', 'BreakStatement'].includes(consList[consList.length - 1].type);
      if (on('else-lift') && alt && exits) {
        // else after return is redundant: if(a){..return} else {B}  ->  if(a){..return} B
        out.push({ type: 'IfStatement', test: rw(test), consequent: cBlock, alternate: null, start: s.start, end: s.end, _o: s });
        pushStmt(alt, out);
        return;
      }
      const aBlock = alt ? { type: 'BlockStatement', body: stmtList([alt], []), start: alt.start, end: alt.end } : null;
      out.push({ type: 'IfStatement', test: rw(test), consequent: cBlock, alternate: aBlock, start: s.start, end: s.end, _o: s });
      return;
    }
    case 'WhileStatement':
      if (on('while-for')) {
        out.push(rw({ type: 'ForStatement', init: null, test: s.test, update: null, body: s.body, start: s.start, end: s.end, _o: s }));
        return;
      }
      out.push(rw(s));
      return;
    case 'ForStatement':
      if (on('for-init-lift') && s.init) {
        if (s.init.type === 'VariableDeclaration') pushStmt(s.init, out);
        else pushStmt(asStmt(s.init, s.init), out);
        out.push(rw({ ...s, init: null, _o: s }));
        return;
      }
      out.push(rw(s));
      return;
    case 'ReturnStatement':
    case 'ThrowStatement': {
      let a = s.argument;
      if (on('ret-seq') && a && a.type === 'SequenceExpression') {
        const xs = a.expressions;
        for (let i = 0; i < xs.length - 1; i++) pushStmt(asStmt(xs[i], xs[i]), out);
        a = xs[xs.length - 1];
      }
      if (on('return-void') && s.type === 'ReturnStatement' && a && a.type === 'UnaryExpression' && a.operator === 'void' && !isVoid0(a)) {
        pushStmt(asStmt(a.argument, a.argument), out);
        a = null;
      }
      if (on('ret-undef') && s.type === 'ReturnStatement' && a && (isVoid0(a) || (a.type === 'Identifier' && a.name === 'undefined'))) a = null;
      if (on('ret-ternary') && s.type === 'ReturnStatement' && a && a.type === 'ConditionalExpression') {
        // return a?b:c  ->  if(a){return b} return c
        pushStmt({ type: 'IfStatement', test: a.test, consequent: { type: 'ReturnStatement', argument: a.consequent, start: a.start, end: a.end }, alternate: null, start: s.start, end: s.end, _o: s }, out);
        pushStmt({ type: 'ReturnStatement', argument: a.alternate, start: a.start, end: a.end }, out);
        return;
      }
      out.push({ type: s.type, argument: a ? rw(a) : null, start: s.start, end: s.end, _o: s });
      return;
    }
    default:
      out.push(rw(s));
  }
}
function blockify(s) {
  return { type: 'BlockStatement', body: stmtList([s], []), start: s.start, end: s.end };
}
function rwFields(n) {
  const o = {};
  for (const k of Object.keys(n)) o[k] = SKIP_KEYS.has(k) ? n[k] : rw(n[k]);
  o._o = n._o ?? n;
  return o;
}
function identNames(node) {
  const set = new Set();
  (function walk(x) {
    if (!x || typeof x !== 'object') return;
    if (Array.isArray(x)) {
      for (const y of x) walk(y);
      return;
    }
    if (x.type === 'Identifier') set.add(x.name);
    for (const k in x) if (k !== '_o' && k !== 'start' && k !== 'end') walk(x[k]);
  })(node);
  return set;
}
// rewrite one node (expression or non-list statement)
function rw(n) {
  if (!n || typeof n !== 'object') return n;
  if (Array.isArray(n)) return n.map(rw);
  const t = n.type;
  if (t === 'ArrowFunctionExpression' && on('arrow')) {
    const body = n.expression
      ? { type: 'BlockStatement', body: stmtList([{ type: 'ReturnStatement', argument: n.body, start: n.body.start, end: n.body.end }], []), start: n.body.start, end: n.body.end }
      : { type: 'BlockStatement', body: stmtList(n.body.body, [], 'fn'), start: n.body.start, end: n.body.end };
    return { type: 'FunctionExpression', id: null, params: rw(n.params), body, async: n.async, generator: false, start: n.start, end: n.end, _o: n };
  }
  if (t === 'BlockStatement' || t === 'StaticBlock') {
    return { type: t, body: stmtList(n.body, []), start: n.start, end: n.end, _o: n };
  }
  if (t === 'Program') return { type: t, body: stmtList(n.body, []), start: n.start, end: n.end, _o: n };
  if (t === 'SwitchCase') return { type: t, test: rw(n.test), consequent: stmtList(n.consequent, []), start: n.start, end: n.end, _o: n };
  if (['ForStatement', 'ForInStatement', 'ForOfStatement', 'WhileStatement', 'DoWhileStatement', 'LabeledStatement', 'WithStatement'].includes(t)) {
    const o = {};
    const isLoop = t !== 'LabeledStatement' && t !== 'WithStatement';
    for (const k of Object.keys(n)) o[k] = k === 'body' ? (isLoop ? { type: 'BlockStatement', body: stmtList([n.body], [], 'loop'), start: n.body.start, end: n.body.end } : blockify(n.body)) : SKIP_KEYS.has(k) ? n[k] : rw(n[k]);
    o._o = n._o ?? n;
    return o;
  }
  if ((t === 'FunctionDeclaration' || t === 'FunctionExpression') && n.body && n.body.type === 'BlockStatement') {
    let params = n.params;
    if (on('unused-param') && params.length) {
      const used = identNames(n.body);
      let k = params.length;
      while (k > 0 && params[k - 1].type === 'Identifier' && !used.has(params[k - 1].name)) k--;
      if (k !== params.length) params = params.slice(0, k);
    }
    const o = rwFields({ ...n, params, body: null });
    o.body = { type: 'BlockStatement', body: stmtList(n.body.body, [], 'fn'), start: n.body.start, end: n.body.end };
    return o;
  }
  if (t === 'CatchClause' && on('catch-param')) {
    return { type: t, param: null, body: rw(n.body), start: n.start, end: n.end, _o: n };
  }
  if (on('seq0-callee') && (t === 'CallExpression' || t === 'TaggedTemplateExpression')) {
    const k = t === 'CallExpression' ? 'callee' : 'tag';
    const c = n[k];
    if (c.type === 'SequenceExpression' && c.expressions.length === 2 && c.expressions[0].type === 'Literal' && c.expressions[0].value === 0) {
      return rwFields({ ...n, [k]: c.expressions[1], _o: n._o ?? n });
    }
  }
  if (on('nullish') && t === 'ConditionalExpression' && n.test.type === 'BinaryExpression' && n.test.operator === '==' && n.test.left.type === 'Identifier' && n.test.right.type === 'Literal' && n.test.right.value === null && n.alternate.type === 'Identifier' && n.alternate.name === n.test.left.name) {
    return rw({ type: 'LogicalExpression', operator: '??', left: n.alternate, right: n.consequent, start: n.start, end: n.end, _o: n._o ?? n });
  }
  if (on('not-eq') && t === 'UnaryExpression' && n.operator === '!' && n.argument.type === 'BinaryExpression' && ['==', '!=', '===', '!=='].includes(n.argument.operator)) {
    const flip = { '==': '!=', '!=': '==', '===': '!==', '!==': '===' }[n.argument.operator];
    return rw({ ...n.argument, operator: flip, _o: n._o ?? n });
  }
  if (t === 'BinaryExpression') {
    let { left, right, operator } = n;
    if (on('yoda') && rank(left) > rank(right)) {
      if (COMMUTE.has(operator)) [left, right] = [right, left];
      else if (FLIP[operator]) {
        [left, right] = [right, left];
        operator = FLIP[operator];
      }
    }
    return { type: t, operator, left: rw(left), right: rw(right), start: n.start, end: n.end, _o: n };
  }
  if (on('cond-not-swap') && t === 'ConditionalExpression' && n.test.type === 'UnaryExpression' && n.test.operator === '!') {
    return { type: t, test: rw(n.test.argument), consequent: rw(n.alternate), alternate: rw(n.consequent), start: n.start, end: n.end, _o: n };
  }
  if (t === 'IfStatement' || t === 'ExpressionStatement' || t === 'VariableDeclaration' || t === 'ReturnStatement' || t === 'ThrowStatement') {
    // a statement reached outside a statement list (e.g. a for-body handled by blockify) — go through the list path
    const out = stmtList([n], []);
    return out.length === 1 ? out[0] : { type: 'BlockStatement', body: out, start: n.start, end: n.end };
  }
  if (t === 'TryStatement') {
    return { type: t, block: rw(n.block), handler: rw(n.handler), finalizer: rw(n.finalizer), start: n.start, end: n.end, _o: n };
  }
  const o = {};
  for (const k of Object.keys(n)) o[k] = SKIP_KEYS.has(k) ? n[k] : rw(n[k]);
  o._o = n;
  return o;
}

// ---------- serializer ----------
// Emits tokens into `toks`. Identifier tokens are emitted as {id:name} objects so a sub-range can be alpha-renamed
// independently. Function/class nodes record their token range in `fnRanges` keyed by original start.
function serialize(root, level, toks, fnRanges, sk) {
  const L2 = level >= 2;
  function litTok(n) {
    if (!L2 || !on('lit-value')) return 'L' + (n.raw ?? JSON.stringify(n.value));
    if (n.regex) return 'R/' + n.regex.pattern + '/' + n.regex.flags;
    if (n.bigint !== undefined) return 'N' + n.bigint + 'n';
    if (typeof n.value === 'string') return 'S' + JSON.stringify(n.value);
    if (typeof n.value === 'number') return 'N' + String(n.value);
    return 'K' + String(n.value); // true/false/null
  }
  function skLit(n) {
    if (!sk) return;
    if (n.regex) sk.push('R/' + n.regex.pattern + '/' + n.regex.flags);
    else if (typeof n.value === 'string') sk.push('S' + n.value);
    else if (typeof n.value === 'number') sk.push('N' + n.value);
  }
  function keyTok(k, computed) {
    if (!computed && k.type === 'Identifier') return 'P' + k.name;
    if (L2 && on('key-norm') && k.type === 'Literal' && (typeof k.value === 'string' || typeof k.value === 'number')) return 'P' + String(k.value);
    return null;
  }
  function v(n) {
    if (n === null || n === undefined) {
      toks.push('_');
      return;
    }
    const t = n.type;
    // literal-level normalisation
    if (L2) {
      const b = on('bool') ? boolOf(n) : null;
      if (b !== null) {
        toks.push('K' + b);
        return;
      }
      if (on('void0') && isVoid0(n)) {
        toks.push('Gundefined');
        return;
      }
      if (on('inf') && t === 'BinaryExpression' && n.operator === '/' && n.left.type === 'Literal' && n.left.value === 1 && n.right.type === 'Literal' && n.right.value === 0) {
        toks.push('GInfinity');
        return;
      }
      if (on('template') && t === 'TemplateLiteral' && n.expressions.length === 0) {
        const s = n.quasis[0].value.cooked ?? n.quasis[0].value.raw;
        toks.push('S' + JSON.stringify(s));
        if (sk) sk.push('S' + s);
        return;
      }
      if (on('use-strict') && t === 'ExpressionStatement' && n.directive === 'use strict') return;
    }
    if (t === 'Identifier') {
      if (GLOBALS.has(n.name)) {
        toks.push('G' + n.name);
        if (sk) sk.push('G' + n.name);
      } else toks.push({ id: n.name });
      return;
    }
    if (t === 'PrivateIdentifier') {
      if (on('private-alpha')) toks.push({ id: '#' + n.name });
      else toks.push('#' + n.name);
      return;
    }
    if (t === 'Literal') {
      toks.push(litTok(n));
      skLit(n);
      return;
    }
    if (t === 'TemplateElement') {
      toks.push('T' + JSON.stringify(n.value.cooked ?? n.value.raw));
      if (sk) sk.push('S' + (n.value.cooked ?? n.value.raw));
      return;
    }
    const isFn = FN_TYPES.has(t) || CLASS_TYPES.has(t);
    const from = toks.length;
    const skFrom = sk ? sk.length : 0;
    let ty = t;
    if (L2 && on('new-call') && t === 'NewExpression' && n.callee.type === 'Identifier' && NEW_TO_CALL.has(n.callee.name)) ty = 'CallExpression';
    if (on('fn-anon') && level >= 3) {
      if (t === 'FunctionDeclaration' || t === 'FunctionExpression') ty = 'Function';
      else if (CLASS_TYPES.has(t)) ty = 'Class';
    }
    toks.push('(' + ty);
    if (t === 'MemberExpression') {
      v(n.object);
      let kt = keyTok(n.property, n.computed);
      if (kt && L2 && n.computed && !/^[A-Za-z_$][\w$]*$/.test(kt.slice(1))) kt = null;
      if (kt && (!n.computed || (L2 && on('key-norm')))) {
        toks.push(kt);
        if (sk) sk.push(kt);
      } else {
        toks.push('[');
        v(n.property);
      }
      if (n.optional) toks.push('?.');
      toks.push(')');
      return;
    }
    if (t === 'Property' || t === 'PropertyDefinition' || t === 'MethodDefinition') {
      const kt = keyTok(n.key, n.computed);
      if (kt) {
        toks.push(kt);
        if (sk) sk.push(kt);
      } else {
        toks.push('[');
        v(n.key);
      }
      if (n.kind && n.kind !== 'init') toks.push('k' + n.kind);
      if (n.static) toks.push('static');
      v(n.value);
      toks.push(')');
      return;
    }
    const keys = on('sorted-keys') ? Object.keys(n).sort() : Object.keys(n);
    for (const k of keys) {
      if (SKIP_KEYS.has(k)) continue;
      if (k === 'id' && level >= 3 && on('fn-anon') && (FN_TYPES.has(t) || CLASS_TYPES.has(t))) continue;
      if (L2 && on('decl-kind') && k === 'kind' && t === 'VariableDeclaration') continue;
      if (L2 && on('eqeq') && t === 'BinaryExpression' && k === 'operator') {
        toks.push(n.operator === '===' ? '==' : n.operator === '!==' ? '!=' : n.operator);
        continue;
      }
      if (t === 'ArrowFunctionExpression' && k === 'expression') continue;
      const val = n[k];
      if (Array.isArray(val)) {
        toks.push('[');
        for (const x of val) v(x);
        toks.push(']');
      } else if (val && typeof val === 'object' && val.type) v(val);
      else if (val === null) toks.push('_');
      else if (typeof val === 'boolean') {
        if (val) toks.push(k);
      } else if (typeof val === 'string') toks.push(val);
    }
    toks.push(')');
    if (isFn && fnRanges) {
      const o = n._o ?? n;
      fnRanges.push({ start: o.start, end: o.end, from, to: toks.length, skd: sk ? new Set(sk.slice(skFrom)).size : 0 });
    }
  }
  v(root);
}

function alphaHash(toks, from, to) {
  const map = new Map();
  const h = createHash('md5');
  let buf = '';
  for (let i = from; i < to; i++) {
    const t = toks[i];
    let s;
    if (typeof t === 'string') s = t;
    else {
      let k = map.get(t.id);
      if (k === undefined) {
        k = map.size;
        map.set(t.id, k);
      }
      s = '$' + k;
    }
    buf += s + '\u0001';
    if (buf.length > 65536) {
      h.update(buf);
      buf = '';
    }
  }
  h.update(buf);
  return h.digest('hex').slice(0, 16);
}

function maskHash(toks, from, to) {
  const h = createHash('md5');
  let buf = '';
  for (let i = from; i < to; i++) {
    const t = toks[i];
    buf += (typeof t === 'string' ? t : '$') + '\u0001';
    if (buf.length > 65536) {
      h.update(buf);
      buf = '';
    }
  }
  h.update(buf);
  return h.digest('hex').slice(0, 16);
}

export function hashStr(s) {
  return createHash('md5').update(s).digest('hex').slice(0, 16);
}

// One unit (a top-level statement, or a declarator) -> fingerprints at each level and of each function inside.
// minFnTokens: functions smaller than this (L3 tokens) are not reported.
export function canonUnit(node, src, minFnTokens = 12) {
  const res = { h0: hashStr(src.slice(node.start, node.end)) };
  const fnByStart = new Map();
  for (const level of [1, 2, 3]) {
    const toks = [];
    const fr = [];
    const sk = level === 3 ? [] : null;
    const root = level === 3 ? rewriteUnit(node) : node;
    serialize(root, level, toks, fr, sk);
    res['h' + level] = alphaHash(toks, 0, toks.length);
    if (level === 3) {
      res.h4 = maskHash(toks, 0, toks.length);
      res.n = toks.length;
      res.sk = hashStr(sk.join('\u0001'));
      res.bag = hashStr([...sk].sort().join('\u0001'));
      res.skLen = sk.length;
      res.skd = new Set(sk).size;
    }
    for (const r of fr) {
      if (r.to - r.from < 4) continue;
      let f = fnByStart.get(r.start);
      if (!f) {
        f = { s: r.start, e: r.end };
        fnByStart.set(r.start, f);
      }
      f['h' + level] = alphaHash(toks, r.from, r.to);
      if (level === 3) {
        f.skd = r.skd;
        f.n = r.to - r.from;
        f.h4 = maskHash(toks, r.from, r.to);
      }
    }
  }
  res.fns = [...fnByStart.values()].filter((f) => f.n >= minFnTokens && f.h3);
  return res;
}

function rewriteUnit(node) {
  // a unit is a statement or a declarator; wrap statements in a list so list-level rewrites apply
  if (node.type === 'VariableDeclarator') return rw(node);
  const out = stmtList([node], []);
  return out.length === 1 ? out[0] : { type: 'BlockStatement', body: out, start: node.start, end: node.end };
}

// Content skeleton strings (for literal anchors): string literals of length >= minLen inside a node
export function longStrings(node, minLen = 12) {
  const out = [];
  (function walk(n) {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) {
      for (const x of n) walk(x);
      return;
    }
    if (n.type === 'Literal' && typeof n.value === 'string' && n.value.length >= minLen) out.push(n.value);
    else if (n.type === 'TemplateElement') {
      const s = n.value.cooked ?? n.value.raw;
      if (s && s.length >= minLen) out.push(s);
    }
    for (const k in n) if (k !== 'type' && k !== 'start' && k !== 'end') walk(n[k]);
  })(node);
  return out;
}

// Ablation: L3 hashes of a unit and its functions with some transforms switched off.
export function ablateUnit(node, src, offList, minFnTokens = 12) {
  OFF = new Set(offList);
  try {
    const toks = [];
    const fr = [];
    serialize(rewriteUnit(node), 3, toks, fr, null);
    const fns = [];
    for (const r of fr) if (r.to - r.from >= minFnTokens) fns.push([r.start, r.end, r.to - r.from, alphaHash(toks, r.from, r.to)]);
    return { h: alphaHash(toks, 0, toks.length), n: toks.length, fns, toks };
  } finally {
    OFF = new Set();
  }
}
export function alphaTokens(toks, from = 0, to = toks.length) {
  const map = new Map();
  const out = [];
  for (let i = from; i < to; i++) {
    const t = toks[i];
    if (typeof t === 'string') out.push(t);
    else {
      let k = map.get(t.id);
      if (k === undefined) { k = map.size; map.set(t.id, k); }
      out.push('$' + k);
    }
  }
  return out;
}
