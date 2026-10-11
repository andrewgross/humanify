// node showunit.mjs <mode> <buildDir> <startChar> [<buildDir2> <startChar2>]  : print unit tokens (and first divergence)
import { readFileSync } from 'node:fs';
import { parse, serialise } from './canon.mjs';
const LAB = '/work/exp095-library-fingerprints/lab/';
const [mode, d1, s1, d2, s2] = process.argv.slice(2);
function get(d, s) {
  const code = readFileSync(d.startsWith('/') ? d : LAB + d + '/out.js', 'utf8');
  const { tok, units } = serialise(parse(code), { loose: mode === 'loose', code });
  const u = units.find(u => u.start === +s);
  return { toks: tok.slice(u.a, u.b).map(t => t[0] === '$' ? 'I' : t), text: code.slice(u.start, u.end) };
}
const A = get(d1, s1);
if (!d2) { console.log(A.text.slice(0, 2000)); console.log(A.toks.join(' ').slice(0, 4000)); process.exit(); }
const B = get(d2, s2);
let i = 0; while (i < A.toks.length && A.toks[i] === B.toks[i]) i++;
console.log('A:', A.text.slice(0, 1500)); console.log('B:', B.text.slice(0, 1500));
console.log('diverge at', i, 'of', A.toks.length, B.toks.length);
console.log('A ctx:', A.toks.slice(Math.max(0, i - 25), i + 15).join(' '));
console.log('B ctx:', B.toks.slice(Math.max(0, i - 25), i + 15).join(' '));
