import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { runDoctor, renderDoctorReport } from '../src/doctor.js';

const execFileAsync = promisify(execFile);

async function write(filePath, content = '') {
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, content);
}

async function createFixture() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'agent-sync-'));
  const home = path.join(root, 'home');
  const project = path.join(root, 'project');

  await write(path.join(home, '.codex', 'config.toml'), 'model = "gpt-test"\n');
  await write(path.join(home, '.codex', 'auth.json'), '{"token":"sk-test-secret"}\n');
  await write(path.join(home, '.codex', 'sessions', 'rollout.jsonl'), '{"prompt":"private"}\n');

  await write(path.join(project, 'AGENTS.md'), '# Project instructions\n');
  await write(path.join(project, '.claude', 'hooks', 'stop.sh'), 'echo stop\n');
  await write(path.join(project, '.mcp.json'), '{"mcpServers":{"x":{"env":{"API_KEY":"secret"}}}}\n');
  await write(path.join(project, '.cursor', 'rules', 'style.md'), '# Cursor rule\n');
  await write(path.join(project, '.cursor', 'mcp.json'), '{"mcpServers":{}}\n');
  await write(path.join(project, '.aider.chat.history.md'), '# chat transcript\n');
  await write(path.join(project, 'review.agent.md'), '# custom copilot agent\n');

  return { root, home, project };
}

function allFindings(manifest) {
  return manifest.agents.flatMap((agent) => agent.findings.map((finding) => ({ agent: agent.id, ...finding })));
}

test('doctor detects agent roots and classifies risky surfaces without file contents', async (t) => {
  const fixture = await createFixture();
  t.after(() => fs.rm(fixture.root, { recursive: true, force: true }));

  const manifest = await runDoctor({
    home: fixture.home,
    project: fixture.project,
    maxDepth: 5,
    maxEntries: 100
  });

  assert.equal(manifest.schemaVersion, '0.1');
  assert.ok(manifest.summary.agentsDetected >= 5);

  const findings = allFindings(manifest);
  assert.ok(findings.some((finding) => finding.path === '~/.codex/auth.json' && finding.safetyClass === 'secret_bearing'));
  assert.ok(findings.some((finding) => finding.path.includes('/.codex/sessions') && finding.safetyClass === 'raw_session'));
  assert.ok(findings.some((finding) => finding.path.endsWith('/.mcp.json') && finding.safetyClass === 'mcp_config'));
  assert.ok(findings.some((finding) => finding.path.endsWith('/stop.sh') && finding.safetyClass === 'executable'));
  assert.ok(findings.some((finding) => finding.path.endsWith('/review.agent.md') && finding.agent === 'github-copilot'));

  const serialized = JSON.stringify(manifest);
  assert.equal(serialized.includes('sk-test-secret'), false);
  assert.equal(serialized.includes('private'), false);
});

test('rendered report is human-readable and omits raw secret contents', async (t) => {
  const fixture = await createFixture();
  t.after(() => fs.rm(fixture.root, { recursive: true, force: true }));

  const manifest = await runDoctor({
    home: fixture.home,
    project: fixture.project,
    maxDepth: 5,
    maxEntries: 100
  });
  const report = renderDoctorReport(manifest);

  assert.match(report, /agent-sync doctor/);
  assert.match(report, /OpenAI Codex/);
  assert.match(report, /secret_bearing/);
  assert.equal(report.includes('sk-test-secret'), false);
});

test('CLI outputs JSON manifest and supports output file', async (t) => {
  const fixture = await createFixture();
  t.after(() => fs.rm(fixture.root, { recursive: true, force: true }));
  const output = path.join(fixture.root, 'snapshot.json');

  const { stdout } = await execFileAsync('node', [
    path.join(process.cwd(), 'bin', 'agent-sync.js'),
    'doctor',
    '--json',
    '--home',
    fixture.home,
    '--project',
    fixture.project,
    '--output',
    output
  ]);

  assert.equal(stdout, '');
  const manifest = JSON.parse(await fs.readFile(output, 'utf8'));
  assert.equal(manifest.schemaVersion, '0.1');
  assert.ok(manifest.summary.findings > 0);
  assert.equal(JSON.stringify(manifest).includes('sk-test-secret'), false);
});
