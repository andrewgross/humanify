import { canonUnit } from './canon.mjs';
import { parseAny } from './scan.mjs';
const cases = [
  ['if(typeof e==="string")e=c(e);if(e?.length!==16)throw RangeError("x")', 'if(typeof e=="string"&&(e=c(e)),e?.length!==16)throw new RangeError("x")', 'if("string"==typeof e&&(e=c(e)),16!==e?.length)throw RangeError("x")'],
  ['function f(a){return a?1:2}', 'function g(b){if(b)return 1;return 2}', 'function h(c){if(!c)return 2;else return 1}'],
  ['var x=!0,y=void 0', 'let x=true;let y=undefined', 'const x=!0;const y=void 0'],
  ['a.b&&c()', 'if(a.b)c()', 'a["b"]&&c()'],
  ['f=(a)=>a+1', 'f=function(a){return a+1}', 'f=a=>{return a+1}'],
];
for (const c of cases) {
  const r = c.map((s) => { const ast = parseAny(s); const u = ast.body.length === 1 ? ast.body[0] : {type:'BlockStatement',body:ast.body,start:0,end:s.length}; return canonUnit(u, s, 1); });
  console.log(['h1','h2','h3'].map((k) => k + ':' + (new Set(r.map((x) => x[k])).size === 1 ? 'SAME' : 'diff')).join(' '), '|', c[0]);
}
const more = [
  ['function f(a){if(a.length>100)return;g(a)}', 'function f(a){if(!(a.length>100)){g(a)}}'],
  ['var X=class{constructor(){this.a=1}}', 'class X{constructor(){this.a=1}}'],
  ['try{a()}catch(e){b()}', 'try{a()}catch{b()}'],
  ['while(a)b()', 'for(;a;)b()'],
  ['if(a!=b)x();else y()', 'if(a==b)y();else x()'],
  ['var i=0;for(;i<3;i++)f(i)', 'for(var i=0;i<3;i++)f(i)'],
  ['function f(){a();return}', 'function f(){a()}'],
  ['(0,a.b)(c)', 'a.b(c)'],
  ['x=e==null?"c":e', 'x=e??"c"'],
  ['x=16!==e?.length', 'x=e?.length!==16'],
];
for (const c of more) {
  const r = c.map((s) => { const ast = parseAny(s); const u = ast.body.length === 1 ? ast.body[0] : {type:'BlockStatement',body:ast.body,start:0,end:s.length}; return canonUnit(u, s, 1); });
  console.log('h3:' + (new Set(r.map((x) => x.h3)).size === 1 ? 'SAME' : 'diff'), '|', c[0]);
}
const more2 = [
  ['function f(a){if(a)return!1;if(a.b)return!1;return g()}', 'function f(a){if(a||a.b)return!1;return g()}'],
  ['for(;;){if(a)continue;b()}', 'for(;;){if(!a)b()}'],
  ['x=function y(a,b){return a}', 'x=function(a){return a}'],
  ['function f(a){return void a()}', 'function f(a){a()}'],
  ['class A{#x=1;g(){return this.#x}}', 'class A{#y=1;g(){return this.#y}}'],
];
for (const c of more2) {
  const r = c.map((s) => { const ast = parseAny(s); const u = ast.body.length === 1 ? ast.body[0] : {type:'BlockStatement',body:ast.body,start:0,end:s.length}; return canonUnit(u, s, 1); });
  console.log('h3:' + (new Set(r.map((x) => x.h3)).size === 1 ? 'SAME' : 'diff'), '|', c[0]);
}
