// Winnowed k-gram windows of Claude Code's raw bundle (same k/w as window.mjs). usage: node scripts/cc_window.mjs <ver>
import fs from 'node:fs';
import { windows } from './window.mjs';
const ver = process.argv[2];
const src = fs.readFileSync(`/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${ver}/binary-decompiled/src/entrypoints/index.js`, 'utf8');
const out = `/work/exp095-library-fingerprints/bundler-variation-and-boundaries/data/windows/cc/min/cc-${ver}`;
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(out + '/raw.tsv', windows(src, 20, 8).map((r) => r.join('\t')).join('\n') + '\n');
