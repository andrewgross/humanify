import { parseSync } from '@babel/core';
import { readFileSync } from 'fs';
import { buildFunctionGraph } from '../../src/analysis/function-graph.js';

const code = readFileSync('experiments/001-baseline-clustering/fixtures/preact-v1/output/deobfuscated.js', 'utf-8');
let ast: any;
try { ast = parseSync(code, { sourceType: 'module' }); } catch { ast = parseSync(code, { sourceType: 'script' }); }

const fns = buildFunctionGraph(ast!, 'deobfuscated.js');
const topLevel = fns.filter(fn => fn.scopeParent === undefined || fn.scopeParent === null);

const names = new Map<string, string>();
for (const fn of fns) {
  const node = fn.path.node as any;
  let name = '<anon>';
  if (node.id) name = node.id.name;
  names.set(fn.sessionId, name);
}

const topIds = new Set(topLevel.map(fn => fn.sessionId));

console.log('=== Isolated top-level functions (no top-level callers AND no top-level callees) ===\n');
let isoCount = 0;
for (const fn of topLevel) {
  const topCallers = Array.from(fn.callers).filter(c => topIds.has(c.sessionId));
  const topCallees = Array.from(fn.internalCallees).filter(c => topIds.has(c.sessionId));
  if (topCallers.length === 0 && topCallees.length === 0) {
    const line = fn.path.node.loc?.start.line;
    const allCallerNames = Array.from(fn.callers).map(c => names.get(c.sessionId) || '?');
    const allCalleeNames = Array.from(fn.internalCallees).map(c => names.get(c.sessionId) || '?');
    console.log(`  line ${line}: ${names.get(fn.sessionId)} | callers(nested)=[${allCallerNames.join(', ')}] callees(nested)=[${allCalleeNames.join(', ')}]`);
    isoCount++;
  }
}
console.log(`\nTotal isolated: ${isoCount} of ${topLevel.length} top-level functions\n`);

// Now show the 3 big clusters' function names
import { clusterFunctions } from '../../src/split/cluster.js';
const result = clusterFunctions(topLevel, { minClusterSize: 3 });

console.log('=== Cluster composition ===\n');
const clustersBySize = [...result.clusters].sort((a, b) => b.members.size - a.members.size);
for (const c of clustersBySize) {
  if (c.members.size < 2) continue;
  const memberNames = Array.from(c.members).map(id => {
    const fn = fns.find(f => f.sessionId === id);
    return names.get(id) || '?';
  }).sort();
  console.log(`Cluster ${c.id.slice(0,8)} (${c.members.size} members):`);
  console.log(`  ${memberNames.join(', ')}\n`);
}
