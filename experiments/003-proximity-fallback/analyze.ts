import { parseSync } from '@babel/core';
import { readFileSync } from 'fs';
import { buildFunctionGraph } from '../../src/analysis/function-graph.js';
import { clusterFunctions } from '../../src/split/cluster.js';

// Map function names to their original Preact modules
const moduleMap: Record<string, string> = {
  assign: 'util', removeNode: 'util',
  _catchError: 'catch-error',
  createElement: 'create-element', createVNode: 'create-element',
  createRef: 'create-element', Fragment: 'create-element', isValidElement: 'create-element',
  BaseComponent: 'component', getDomSibling: 'component',
  renderComponent: 'component', updateParentDomPointers: 'component',
  enqueueRender: 'component', depthSort: 'component', process: 'component',
  diffChildren: 'diff-children', constructNewChildrenArray: 'diff-children',
  insert: 'diff-children', toChildArray: 'diff-children', findMatchingIndex: 'diff-children',
  setStyle: 'diff-props', setProperty: 'diff-props', createEventProxy: 'diff-props',
  dispatchEvent: 'diff-props', dispatchEventCapture: 'diff-props',
  diff: 'diff-index', commitRoot: 'diff-index', diffElementNodes: 'diff-index',
  applyRef: 'diff-index', unmount: 'diff-index',
  doRender: 'render', render: 'render', hydrate: 'render',
  cloneElement: 'clone-element',
  createContext: 'create-context',
};

const code = readFileSync('experiments/001-baseline-clustering/fixtures/preact-v1/output/deobfuscated.js', 'utf-8');
let ast: any;
try { ast = parseSync(code, { sourceType: 'module' }); } catch { ast = parseSync(code, { sourceType: 'script' }); }

const fns = buildFunctionGraph(ast!, 'deobfuscated.js');
const topLevel = fns.filter(fn => fn.scopeParent === undefined || fn.scopeParent === null);

// Build name map
const names = new Map<string, string>();
for (const fn of fns) {
  const node = fn.path.node as any;
  let name = '<anon>';
  if (node.id) name = node.id.name;
  names.set(fn.sessionId, name);
}

// Run clustering with merge + proximity
const result = clusterFunctions(topLevel, { minClusterSize: 3, proximityFallback: true });

console.log(`=== Experiment 003: Clusters with merge + proximity ===\n`);
console.log(`Clusters: ${result.clusters.length}, Shared: ${result.shared.size}, Orphans: ${result.orphans.size}\n`);

const clustersBySize = [...result.clusters].sort((a, b) => b.members.size - a.members.size);

for (const c of clustersBySize) {
  const memberNames = Array.from(c.members).map(id => {
    const name = names.get(id) || '?';
    const fn = fns.find(f => f.sessionId === id);
    const line = fn?.path.node.loc?.start.line || 0;
    return { name, line };
  }).sort((a, b) => a.line - b.line);

  // Map to original modules
  const moduleCounts = new Map<string, number>();
  for (const { name } of memberNames) {
    const mod = moduleMap[name] || 'hooks';
    moduleCounts.set(mod, (moduleCounts.get(mod) || 0) + 1);
  }

  console.log(`Cluster ${c.id.slice(0, 8)} (${c.members.size} members, lines ${memberNames[0].line}-${memberNames[memberNames.length - 1].line}):`);
  console.log(`  Functions: ${memberNames.map(m => m.name).join(', ')}`);
  console.log(`  Original modules: ${Array.from(moduleCounts.entries()).map(([m, n]) => `${m}(${n})`).join(', ')}`);
  console.log();
}

// Quality assessment: what fraction of each cluster comes from its dominant module?
console.log(`=== Module purity analysis ===\n`);
let totalCorrect = 0;
let totalMembers = 0;
for (const c of clustersBySize) {
  const moduleCounts = new Map<string, number>();
  for (const id of c.members) {
    const name = names.get(id) || '?';
    const mod = moduleMap[name] || 'hooks';
    moduleCounts.set(mod, (moduleCounts.get(mod) || 0) + 1);
  }
  const dominant = Math.max(...moduleCounts.values());
  const purity = dominant / c.members.size;
  totalCorrect += dominant;
  totalMembers += c.members.size;
  const dominantMod = Array.from(moduleCounts.entries()).find(([_, n]) => n === dominant)![0];
  console.log(`  Cluster ${c.id.slice(0, 8)}: purity ${(purity * 100).toFixed(0)}% (dominant: ${dominantMod})`);
}
console.log(`\n  Overall purity: ${(totalCorrect / totalMembers * 100).toFixed(1)}%`);
