// Optional live Context7 acceptance check. No LLM key required.
// node scripts/verify_context7.mjs [path-to-ycode]
// Uses CONTEXT7_API_KEY when set; otherwise tests anonymous access.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const repo = path.resolve(import.meta.dirname, '..');
const executable = path.resolve(process.argv[2] || path.join(repo, 'target/debug', process.platform === 'win32' ? 'ycode.exe' : 'ycode'));
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ycode-context7-'));
const authenticated = Boolean(process.env.CONTEXT7_API_KEY?.trim());
const server = {
  type: 'http', url: 'https://mcp.context7.com/mcp',
  read_only_tools: ['resolve-library-id', 'query-docs'],
  ...(authenticated ? { bearer_env: 'CONTEXT7_API_KEY' } : {}),
};
fs.writeFileSync(path.join(root, '.mcp.json'), JSON.stringify({ mcpServers: { context7: server } }));
const checks = [];
const resultPath = path.join(repo, 'target/context7-interoperability/results.json');
const report = { verifiedAt: new Date().toISOString(), platform: process.platform, authenticated, checks };
const run = (args) => {
  const result = spawnSync(executable, ['--no-global-config', '--allow-mcp', '--plan', '--cwd', root, 'mcp', ...args], { encoding: 'utf8', timeout: 90000, windowsHide: true });
  // Do not log response bodies or inherited credentials on failure.
  assert.equal(result.status, 0, `Context7 ${args[0]} failed (exit ${result.status}); check credentials, quota and connectivity.`);
  const value = JSON.parse(result.stdout);
  assert.equal(value.isError || false, false);
  return value;
};
const record = name => { checks.push({ name, status: 'passed' }); console.log(`PASS ${name}`); };
try {
  const catalog = run(['check', 'context7']);
  const names = catalog.catalogs.tools.map(tool => tool.name);
  assert.ok(names.includes('resolve-library-id') && names.includes('query-docs'));
  report.protocol = catalog.peer.protocolVersion;
  report.serverVersion = catalog.peer.serverInfo.version;
  record('context7-discovery');
  const resolved = run(['call', 'context7', 'resolve-library-id', '--arguments', JSON.stringify({ libraryName: 'React', query: 'How to use the React useState hook?' })]);
  const text = (resolved.content || []).filter(item => item.type === 'text').map(item => item.text).join('\n');
  // Select an ID returned by the service, never invent one for the docs request.
  const libraryId = text.match(/Context7-compatible library ID:\s*(\/[^\s]+)/i)?.[1];
  assert.ok(libraryId, 'Context7 did not return a usable library ID; authentication or quota may be required.');
  report.libraryId = libraryId;
  record('context7-resolve-in-plan-mode');
  const docs = run(['call', 'context7', 'query-docs', '--arguments', JSON.stringify({ libraryId, query: 'How to use useState to increment a counter?', language: 'JavaScript' })]);
  const docsText = (docs.content || []).filter(item => item.type === 'text').map(item => item.text).join('\n');
  assert.match(docsText, /useState/);
  assert.match(docsText, /https?:\/\//);
  record('context7-documentation-in-plan-mode');
  console.log(`Verified ${checks.length} Context7 checks (${authenticated ? 'API key' : 'anonymous'} access).`);
} catch (error) {
  report.failure = error.message;
  throw error;
} finally {
  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, JSON.stringify(report, null, 2));
}
