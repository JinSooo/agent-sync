import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { runDiff } from '../src/diff.js';
import { runDoctor } from '../src/doctor.js';
import { runPlan } from '../src/plan.js';

const execFileAsync = promisify(execFile);

async function write(filePath, content = '') {
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, content);
}

async function createMachine(root, name, options = {}) {
  const home = path.join(root, name + '-home');
  const project = path.join(root, name + '-project');
  await write(path.join(home, '.codex', 'config.toml'), 'model = "gpt-test"\n');
  await write(path.join(project, 'AGENTS.md'), '# instructions\n');

  if (options.withSecrets !== false) {
    await write(path.join(home, '.codex', 'auth.json'), '{"token":"secret-' + name + '"}\n');
  }
  if (options.withMcp) {
    await write(path.join(project, '.mcp.json'), '{"mcpServers":{"demo":{}}}\n');
  }
  if (options.withSafeConfig) {
    await write(path.join(home, '.codex', 'preferences.toml'), 'theme = "dark"\n');
  }
  if (options.withCursorRule) {
    await write(path.join(project, '.cursor', 'rules', 'style.md'), '# rule\n');
  }
  if (options.withSession) {
    await write(path.join(home, '.codex', 'sessions', 'thread.jsonl'), '{"prompt":"private-' + name + '"}\n');
  }
  if (options.withDatabase) {
    await write(path.join(home, '.cursor', 'ai-tracking', 'ai-code-tracking.db'), 'not a real db\n');
  }

  return {
    home,
    project,
    manifest: await runDoctor({ home, project, maxDepth: 5, maxEntries: 100 })
  };
}

test('diff reports missing target findings using portable project paths', async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'agent-sync-diff-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));

  const source = await createMachine(root, 'source', {
    withSafeConfig: true,
    withMcp: true,
    withCursorRule: true,
    withSession: true,
    withDatabase: true
  });
  const target = await createMachine(root, 'target', {
    withSecrets: false
  });

  const diff = runDiff(source.manifest, target.manifest);
  assert.ok(diff.summary.findings.onlyFrom > 0);

  const cursor = diff.agents.find((agent) => agent.id === 'cursor');
  assert.ok(cursor.onlyFrom.some((finding) => finding.path === '<project>/.cursor/rules/style.md'));

  const claude = diff.agents.find((agent) => agent.id === 'claude');
  assert.ok(claude.onlyFrom.some((finding) => finding.path === '<project>/.mcp.json'));
});

test('plan converts diff into safe, review, and blocked buckets', async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'agent-sync-plan-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));

  const source = await createMachine(root, 'source', {
    withSafeConfig: true,
    withMcp: true,
    withCursorRule: true,
    withSession: true,
    withDatabase: true
  });
  const target = await createMachine(root, 'target', {
    withSecrets: false
  });

  const plan = runPlan(source.manifest, target.manifest, { target: 'windows' });
  assert.ok(plan.summary.safeCandidates > 0);
  assert.ok(plan.summary.reviewRequired > 0);
  assert.ok(plan.summary.blocked > 0);
  assert.ok(plan.blocked.some((item) => item.safetyClass === 'secret_bearing'));
  assert.ok(plan.blocked.some((item) => item.safetyClass === 'raw_session'));
  assert.ok(plan.blocked.some((item) => item.safetyClass === 'database'));
  assert.ok(plan.reviewRequired.some((item) => item.safetyClass === 'mcp_config'));
});

test('diff and plan CLI commands work with snapshot files', async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'agent-sync-cli-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));

  const source = await createMachine(root, 'source', {
    withMcp: true,
    withCursorRule: true,
    withSession: true
  });
  const target = await createMachine(root, 'target', {
    withSecrets: false
  });
  const sourcePath = path.join(root, 'source.json');
  const targetPath = path.join(root, 'target.json');
  await fs.writeFile(sourcePath, JSON.stringify(source.manifest, null, 2));
  await fs.writeFile(targetPath, JSON.stringify(target.manifest, null, 2));

  const diffRun = await execFileAsync('node', [
    path.join(process.cwd(), 'bin', 'agent-sync.js'),
    'diff',
    '--from',
    sourcePath,
    '--to',
    targetPath,
    '--json'
  ]);
  const diff = JSON.parse(diffRun.stdout);
  assert.equal(diff.kind, 'agent-sync-diff');

  const planRun = await execFileAsync('node', [
    path.join(process.cwd(), 'bin', 'agent-sync.js'),
    'plan',
    '--from',
    sourcePath,
    '--to',
    targetPath,
    '--target',
    'windows',
    '--json'
  ]);
  const plan = JSON.parse(planRun.stdout);
  assert.equal(plan.kind, 'agent-sync-plan');
  assert.ok(plan.summary.blocked > 0);
});

test('doctor prunes generated plugin dependency caches from inventory', async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'agent-sync-prune-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));

  const home = path.join(root, 'home');
  const project = path.join(root, 'project');
  await write(
    path.join(home, '.codex', 'plugins', 'cache', 'example', 'node_modules', 'classic-level', 'deps', 'leveldb', 'leveldb.gyp'),
    'generated dependency\n'
  );
  await write(path.join(project, 'AGENTS.md'), '# instructions\n');

  const manifest = await runDoctor({ home, project, maxDepth: 12, maxEntries: 200 });
  const codex = manifest.agents.find((agent) => agent.id === 'codex');
  assert.ok(codex.findings.some((finding) => finding.path === '~/.codex/plugins/cache'));
  assert.ok(!codex.findings.some((finding) => finding.path.includes('node_modules')));
  assert.ok(!codex.findings.some((finding) => finding.path.includes('leveldb.gyp')));
});
