// Pick every stable release since 2025-01-01 (capped, evenly spaced, newest always kept) for the CC-relevant packages.
// node scripts/pick.mjs > versions.json
import { execFileSync } from 'node:child_process';
const PKGS = {
  'highlight.js': { specs: ['highlight.js'], since: '2021-01-01', cap: 30 },
  '@azure/msal-node': { specs: ['@azure/msal-node'], cap: 40 },
  'marked': { specs: ['marked'], cap: 40 },
  '@modelcontextprotocol/sdk': { specs: ['@modelcontextprotocol/sdk/client/index.js', '@modelcontextprotocol/sdk/client/stdio.js', '@modelcontextprotocol/sdk/client/sse.js', '@modelcontextprotocol/sdk/client/streamableHttp.js', '@modelcontextprotocol/sdk/client/auth.js', '@modelcontextprotocol/sdk/types.js', '@modelcontextprotocol/sdk/server/index.js', '@modelcontextprotocol/sdk/server/mcp.js', '@modelcontextprotocol/sdk/server/stdio.js', '@modelcontextprotocol/sdk/shared/protocol.js'], cap: 40 },
  'axios': { specs: ['axios'], cap: 30 },
  'zod': { specs: ['zod', 'zod/v4', 'zod/v3'], cap: 40 },
  'zod-to-json-schema': { specs: ['zod-to-json-schema'], cap: 20 },
  'execa': { specs: ['execa'], cap: 20 },
  'lru-cache': { specs: ['lru-cache'], cap: 20 },
  '@anthropic-ai/sdk': { specs: ['@anthropic-ai/sdk'], cap: 40 },
  'ajv': { specs: ['ajv'], cap: 15 },
  'yaml': { specs: ['yaml'], cap: 20 },
  'ws': { specs: ['ws'], cap: 15 },
  'chokidar': { specs: ['chokidar'], cap: 15 },
  'diff': { specs: ['diff'], cap: 15 },
  'semver': { specs: ['semver'], cap: 10 },
  'commander': { specs: ['commander'], cap: 15 },
  'undici': { specs: ['undici'], cap: 20 },
};
const out = [];
for (const [name, cfg] of Object.entries(PKGS)) {
  const time = JSON.parse(execFileSync('npm', ['view', name, 'time', '--json'], { encoding: 'utf8' }));
  let vs = Object.entries(time).filter(([v, t]) => /^\d+\.\d+\.\d+$/.test(v) && t >= (cfg.since || '2025-01-01')).sort((a, b) => a[1].localeCompare(b[1])).map(([v]) => v);
  if (vs.length > cfg.cap) { const step = vs.length / cfg.cap; const pick = []; for (let i = 0; i < cfg.cap; i++) pick.push(vs[Math.floor(vs.length - 1 - i * step)]); vs = [...new Set(pick)].reverse(); }
  out.push({ name, specs: cfg.specs, versions: vs });
}
console.log(JSON.stringify(out, null, 1));
