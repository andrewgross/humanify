// Canonical serialisation of a JS AST into a token array, plus unit extraction.
//
// A "unit" is a sub-tree we hash on its own:
//   root  - the function passed to a bundler wrapper  (var X = H(function(e,t){...}))  = whole CJS module / lazy init body
//   stmt  - a module-level statement (Program body, or the body of a wrapper function)
//   fn    - any other function (declaration, expression, arrow, method), at any depth
//   class - any class
//
// Identifiers: property names (a.b, {b:..}, methods) are KEPT (minifiers don't mangle them);
// names in GLOBALS are kept as G:name; every other identifier is emitted as $name and masked later.
// Literals are kept (S"...", N123, ...) and can be masked later per variant.
//
// LOOSE mode applies minifier-invariant normalisations (see normalise* below). STRICT mode emits the AST as parsed.
import * as acorn from '/work/exp095-library-fingerprints/lab/tools/node_modules/acorn/dist/acorn.mjs';
import { createHash } from 'node:crypto';

const EXTRA_GLOBALS = ['require', 'module', 'exports', '__dirname', '__filename', 'process', 'Buffer', 'global',
  'globalThis', 'window', 'self', 'document', 'navigator', 'undefined', 'arguments', 'console', 'setTimeout',
  'clearTimeout', 'setInterval', 'clearInterval', 'setImmediate', 'clearImmediate', 'queueMicrotask', 'Bun', 'Deno',
  'location', 'fetch', 'Headers', 'Request', 'Response', 'URL', 'URLSearchParams', 'TextEncoder', 'TextDecoder',
  'AbortController', 'AbortSignal', 'Blob', 'FormData', 'ReadableStream', 'WritableStream', 'TransformStream',
  'XMLHttpRequest', 'WebSocket', 'Event', 'EventTarget', 'crypto', 'performance', 'atob', 'btoa', 'structuredClone',
  'HTMLElement', 'Element', 'Node', 'File', 'FileReader', 'NaN', 'Infinity'];
export const GLOBALS = new Set([...Object.getOwnPropertyNames(globalThis).filter(n => n.length > 2 || n === 'NaN'), ...EXTRA_GLOBALS]);

const IDENT_RE = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
const FN_TYPES = new Set(['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression']);
const SKIP = new Set(['type', 'start', 'end', 'loc', 'range', 'raw', 'sourceType', 'shorthand', 'extra', 'bigint', 'regex', 'value', 'name', 'directive']);

export function parse(code) {
  const opts = { ecmaVersion: 'latest', allowHashBang: true, allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true, allowImportExportEverywhere: true };
  try { return acorn.parse(code, { ...opts, sourceType: 'module' }); }
  catch (e) { return acorn.parse(code, { ...opts, sourceType: 'script' }); }
}

// ---------- loose normalisation (pure functions returning new nodes; never mutate shared) ----------
const lit = (v, s, e) => ({ type: 'Literal', value: v, start: s, end: e });
const not = x => ({ type: 'UnaryExpression', operator: '!', prefix: true, argument: x, start: x.start, end: x.end });
const exprStmt = x => ({ type: 'ExpressionStatement', expression: x, start: x.start, end: x.end });
function terminal(s) {
  if (!s) return false;
  if (s.type === 'BlockStatement') return s.body.length > 0 && terminal(s.body[s.body.length - 1]);
  return s.type === 'ReturnStatement' || s.type === 'ThrowStatement' || s.type === 'ContinueStatement' || s.type === 'BreakStatement';
}
function bareReturn(s) {
  if (!s) return false;
  if (s.type === 'BlockStatement') return s.body.length === 1 && bareReturn(s.body[0]);
  return s.type === 'ReturnStatement' && !s.argument;
}
function isLit(n) { return n && n.type === 'Literal' && !n.regex; }

function normExpr(n) {
  if (!n) return n;
  switch (n.type) {
    case 'NewExpression':
      return { type: 'CallExpression', callee: n.callee, arguments: n.arguments, optional: false, start: n.start, end: n.end };
    case 'UnaryExpression':
      if (n.operator === '!' && isLit(n.argument) && typeof n.argument.value === 'number') return lit(!n.argument.value, n.start, n.end);
      if (n.operator === '!' && n.argument.type === 'UnaryExpression' && n.argument.operator === '!') return normExpr(n.argument.argument);
      if (n.operator === '!' && n.argument.type === 'SequenceExpression') {
        const xs = n.argument.expressions.slice(); const last = xs.pop();
        return { type: 'SequenceExpression', expressions: [...xs, not(last)], start: n.start, end: n.end };
      }
      if (n.operator === 'void' && isLit(n.argument) && n.argument.value === 0) return { type: 'Identifier', name: 'undefined', start: n.start, end: n.end };
      return n;
    case 'TemplateLiteral':
      if (n.expressions.length === 0) return lit(n.quasis[0].value.cooked, n.start, n.end);
      { // `a${b}c`  ==  "a"+b+"c"
        let acc = null;
        const push = x => { acc = acc ? { type: 'BinaryExpression', operator: '+', left: acc, right: x, start: n.start, end: n.end } : x; };
        n.quasis.forEach((q, i) => {
          if (q.value.cooked) push(lit(q.value.cooked, q.start, q.end));
          if (i < n.expressions.length) { if (!acc) push(lit('', q.start, q.end)); push(n.expressions[i]); }
        });
        return acc;
      }
    case 'BinaryExpression': {
      let op = n.operator, l = n.left, r = n.right;
      if (op === '===') op = '=='; else if (op === '!==') op = '!=';
      // typeof x < "u"  (esbuild/bun)  ==  typeof x == "undefined"
      if ((op === '<' || op === '>') && l.type === 'UnaryExpression' && l.operator === 'typeof' && isLit(r) && r.value === 'u') {
        op = op === '<' ? '==' : '!='; r = lit('undefined', r.start, r.end);
      }
      if ((op === '==' || op === '!=') && isLit(l) && !isLit(r)) { const t = l; l = r; r = t; }
      if ((op === '==' || op === '!=') && isLit(r) && r.value === 'undefined' && !(l.type === 'UnaryExpression' && l.operator === 'typeof')) { /* keep */ }
      return { ...n, operator: op, left: l, right: r };
    }
    case 'MemberExpression':
      if (n.computed && isLit(n.property) && typeof n.property.value === 'string' && IDENT_RE.test(n.property.value))
        return { ...n, computed: false, property: { type: 'Identifier', name: n.property.value, start: n.property.start, end: n.property.end } };
      return n;
    default: return n;
  }
}

// Turn one statement into a flat list of statements (loose mode).
function flattenStmt(s, out) {
  if (s.type === 'EmptyStatement') return;
  if (s.type === 'ExpressionStatement') {
    const e = s.expression;
    if (e.type === 'SequenceExpression') { for (const x of e.expressions) flattenStmt({ type: 'ExpressionStatement', expression: x, start: x.start, end: x.end }, out); return; }
    if (e.type === 'LogicalExpression' && (e.operator === '&&' || e.operator === '||')) {
      const test = e.operator === '&&' ? e.left : { type: 'UnaryExpression', operator: '!', prefix: true, argument: e.left, start: e.left.start, end: e.left.end };
      out.push({ type: 'IfStatement', test, consequent: { type: 'ExpressionStatement', expression: e.right, start: e.right.start, end: e.right.end }, alternate: null, start: s.start, end: s.end });
      return;
    }
    if (e.type === 'ConditionalExpression') {
      out.push({ type: 'IfStatement', test: e.test, consequent: { type: 'ExpressionStatement', expression: e.consequent, start: e.consequent.start, end: e.consequent.end },
        alternate: { type: 'ExpressionStatement', expression: e.alternate, start: e.alternate.start, end: e.alternate.end }, start: s.start, end: s.end });
      return;
    }
  }
  if (s.type === 'IfStatement' && s.alternate && terminal(s.consequent)) {
    out.push({ ...s, alternate: null });
    flattenStmt(s.alternate, out);
    return;
  }
  if (s.type === 'ReturnStatement' && s.argument && s.argument.type === 'ConditionalExpression') {
    const c = s.argument;
    out.push({ type: 'IfStatement', test: c.test, consequent: { type: 'ReturnStatement', argument: c.consequent, start: c.consequent.start, end: c.consequent.end }, alternate: null, start: s.start, end: s.end });
    flattenStmt({ type: 'ReturnStatement', argument: c.alternate, start: c.alternate.start, end: c.alternate.end }, out);
    return;
  }
  if (s.type === 'ReturnStatement' && s.argument && s.argument.type === 'SequenceExpression') {
    const xs = s.argument.expressions;
    for (let i = 0; i < xs.length - 1; i++) flattenStmt({ type: 'ExpressionStatement', expression: xs[i], start: xs[i].start, end: xs[i].end }, out);
    out.push({ ...s, argument: xs[xs.length - 1] });
    return;
  }
  if (s.type === 'VariableDeclaration' && s.declarations.length > 1) {
    // var a=1,b=2  ==  var a=1; var b=2   (minifiers merge adjacent declarations)
    for (const d of s.declarations) out.push({ type: 'VariableDeclaration', kind: s.kind, declarations: [d], start: d.start, end: d.end });
    return;
  }
  if (s.type === 'BlockStatement' && !s.body.some(b => b.type === 'VariableDeclaration' && b.kind !== 'var' || b.type === 'ClassDeclaration' || b.type === 'FunctionDeclaration')) {
    for (const b of s.body) flattenStmt(b, out); return;
  }
  out.push(s);
}
function flattenList(list, fnBody) {
  const out = []; for (const s of list) flattenStmt(s, out);
  if (!fnBody) return out;
  // a trailing bare `return;` in a function body is a no-op
  while (out.length && out[out.length - 1].type === 'ReturnStatement' && !out[out.length - 1].argument) out.pop();
  for (let i = 0; i < out.length; i++) {
    const s = out[i];
    if (s.type === 'IfStatement' && !s.alternate && bareReturn(s.consequent)) {
      const rest = flattenList(out.slice(i + 1), true);
      const inner = { type: 'IfStatement', test: not(s.test), consequent: { type: 'BlockStatement', body: rest, start: s.end, end: s.end, flat: true }, alternate: null, start: s.start, end: s.end };
      return [...out.slice(0, i), inner];
    }
  }
  return out;
}
function asBlock(s) { if (!s) return s; if (s.type === 'BlockStatement') return s; return { type: 'BlockStatement', body: [s], start: s.start, end: s.end }; }

// ---------- serialiser ----------
// emits tokens into ctx.tok; registers units into ctx.units: {type, start, end, a, b, nodes}
export function serialise(program, { loose, code }) {
  const tok = [];
  const units = [];
  let nodes = 0;

  function unitStart(type, node) { return { type, start: node.start, end: node.end, a: tok.length, n0: nodes }; }
  function unitEnd(u) { u.b = tok.length; u.nodes = nodes - u.n0; delete u.n0; units.push(u); }

  function list(arr, isModuleLevel, fnBody) {
    if (loose) arr = flattenList(arr, fnBody);
    tok.push('[');
    for (const s of arr) {
      if (isModuleLevel) {
        const wraps = wrapperFns(s);
        if (wraps) { // var X = H(function(){...}) : the wrapper statement itself is not a unit; the fn is "root"
          wrapperStmt(s, wraps);
          continue;
        }
        const u = unitStart('stmt', s); node(s); unitEnd(u);
      } else node(s);
    }
    tok.push(']');
  }

  function wrapperStmt(s, wraps) {
    nodes++; tok.push('(Wrap'); // structural marker, not part of any unit
    for (const d of s.declarations) {
      tok.push('$' + (d.id.name || '?'));
      const fn = d.init.arguments[0];
      const u = unitStart('root', fn); fnNode(fn, true); unitEnd(u);
    }
    tok.push(')');
  }

  function fnNode(n, moduleBody) {
    nodes++;
    tok.push(loose ? '(Fn' : '(' + n.type);
    if (!loose && n.id) ident(n.id);
    if (n.async) tok.push('k:async'); if (n.generator) tok.push('k:gen');
    tok.push('p' + n.params.length);
    for (const p of n.params) node(p);
    if (n.body.type === 'BlockStatement') { nodes++; list(n.body.body, moduleBody, !moduleBody); }
    else if (loose) { tok.push('['); nodes++; tok.push('(ReturnStatement'); node(n.body); tok.push(')'); tok.push(']'); }
    else node(n.body);
    tok.push(')');
  }

  function ident(n) {
    nodes++;
    if (GLOBALS.has(n.name)) tok.push('G:' + n.name); else tok.push('$' + n.name);
  }
  function propKey(n, computed) {
    if (computed) { node(n); return; }
    nodes++;
    if (n.type === 'Identifier') tok.push('.' + n.name);
    else if (n.type === 'PrivateIdentifier') tok.push('$#' + n.name);
    else if (n.type === 'Literal') {
      if (loose && typeof n.value === 'string' && IDENT_RE.test(n.value)) tok.push('.' + n.value);
      else if (loose && typeof n.value === 'number') tok.push('.' + String(n.value));
      else tok.push('.' + JSON.stringify(n.value));
    } else node(n);
  }

  function node(n) {
    if (n === null || n === undefined) { tok.push('_'); return; }
    if (loose) n = normExpr(n);
    switch (n.type) {
      case 'Identifier': ident(n); return;
      case 'PrivateIdentifier': nodes++; tok.push('$#' + n.name); return;
      case 'Literal': {
        nodes++;
        if (n.regex) tok.push('R/' + n.regex.pattern + '/' + n.regex.flags);
        else if (n.bigint !== undefined) tok.push('B' + n.bigint);
        else if (typeof n.value === 'string') tok.push('S' + JSON.stringify(n.value));
        else if (typeof n.value === 'number') tok.push('N' + String(n.value));
        else if (typeof n.value === 'boolean') tok.push(n.value ? 'b1' : 'b0');
        else if (n.value === null) tok.push('z');
        else tok.push('S' + JSON.stringify(String(n.value)));
        return;
      }
      case 'TemplateElement': nodes++; tok.push('S' + JSON.stringify(n.value.cooked ?? n.value.raw)); return;
      case 'FunctionDeclaration': case 'FunctionExpression': case 'ArrowFunctionExpression': {
        const u = unitStart('fn', n); fnNode(n, false); unitEnd(u); return;
      }
      case 'ClassDeclaration': case 'ClassExpression': {
        const u = unitStart('class', n);
        nodes++; tok.push('(Class'); node(n.superClass); node(n.body); tok.push(')');
        unitEnd(u); return;
      }
      case 'MemberExpression':
        nodes++; tok.push(n.optional ? '(Mem?' : '(Mem'); node(n.object); propKey(n.property, n.computed); tok.push(')'); return;
      case 'Property':
        nodes++; tok.push('(Prop' + (n.kind !== 'init' ? ':' + n.kind : '') + (!loose && n.method ? ':m' : ''));
        propKey(n.key, n.computed); node(n.value); tok.push(')'); return;
      case 'MethodDefinition': case 'PropertyDefinition':
        nodes++; tok.push('(' + n.type + ':' + (n.kind || '') + (n.static ? ':s' : ''));
        propKey(n.key, n.computed); node(n.value); tok.push(')'); return;
      case 'VariableDeclaration':
        nodes++; tok.push('(Var:' + (loose ? 'var' : n.kind)); for (const d of n.declarations) node(d); tok.push(')'); return;
      case 'Program': nodes++; list(n.body, true); return;
      case 'BlockStatement': nodes++; if (n.flat) { tok.push('['); for (const x of n.body) node(x); tok.push(']'); } else list(n.body, false); return;
      case 'StaticBlock': nodes++; tok.push('(Static'); list(n.body, false); tok.push(')'); return;
      case 'SwitchCase': nodes++; tok.push('(Case'); node(n.test); list(n.consequent, false); tok.push(')'); return;
      case 'IfStatement':
        nodes++; tok.push('(If'); node(n.test);
        if (loose) { node(asBlock(n.consequent)); node(n.alternate ? asBlock(n.alternate) : null); }
        else { node(n.consequent); node(n.alternate); }
        tok.push(')'); return;
      case 'ExpressionStatement':
        if (n.directive !== undefined && loose) { nodes++; tok.push('(Dir'); tok.push(')'); return; }
        break;
    }
    // generic
    nodes++;
    tok.push('(' + n.type + (n.operator ? n.operator : '') + (n.prefix === false ? ':post' : '') + (n.computed ? ':c' : '') + (n.optional ? ':?' : ''));
    for (const k in n) {
      if (SKIP.has(k) || k === 'operator' || k === 'prefix' || k === 'computed' || k === 'optional') continue;
      const v = n[k];
      if (v && typeof v === 'object') {
        if (Array.isArray(v)) { tok.push('['); for (const x of v) node(x); tok.push(']'); }
        else if (v.type) node(v);
      } else if (typeof v === 'boolean') { if (v) tok.push('k:' + k); }
      else if (typeof v === 'string' && k === 'kind') tok.push('k:' + v);
    }
    tok.push(')');
  }

  // CC: "(function(exports, require, module, __filename, __dirname) {...})" wrapping the whole program
  let prog = program;
  if (program.body.length === 1 && program.body[0].type === 'ExpressionStatement') {
    let e = program.body[0].expression;
    if (e.type === 'CallExpression') e = e.callee;
    if ((e.type === 'FunctionExpression') && e.params.length === 5) prog = { type: 'Program', body: e.body.body, start: e.body.start, end: e.body.end };
  }
  node(prog);
  return { tok, units };
}

// var X = H(function(e,t){...})  /  var X = H(() => {...})   — one or more such declarators, nothing else
function wrapperFns(s) {
  if (s.type !== 'VariableDeclaration') return null;
  for (const d of s.declarations) {
    const i = d.init;
    if (!i || i.type !== 'CallExpression' || i.callee.type !== 'Identifier' || i.arguments.length !== 1) return null;
    const f = i.arguments[0];
    if (!FN_TYPES.has(f.type) || f.body.type !== 'BlockStatement') return null;
  }
  return true;
}

export const h64 = s => createHash('sha1').update(s).digest('hex').slice(0, 16);

// variants of one unit's token slice
export function unitHashes(tok, a, b) {
  const sl = tok.slice(a, b);
  const mask = new Array(sl.length), nolit = new Array(sl.length), alpha = new Array(sl.length);
  const num = new Map();
  let strChars = 0;
  for (let i = 0; i < sl.length; i++) {
    const t = sl[i];
    const c = t.charCodeAt(0);
    if (c === 36 /* $ */) {
      mask[i] = 'I'; nolit[i] = 'I';
      let k = num.get(t); if (k === undefined) { k = 'v' + num.size; num.set(t, k); } alpha[i] = k;
    } else {
      mask[i] = t; alpha[i] = t;
      if (c === 83) strChars += t.length - 3;
      nolit[i] = (c === 83 /* S */) ? 'S' : (c === 78 /* N */) ? 'N' : (c === 82 /* R */) ? 'R' : t;
    }
  }
  return { mask: h64(mask.join(' ')), alpha: h64(alpha.join(' ')), nolit: h64(nolit.join(' ')), nTok: sl.length, strChars };
}
