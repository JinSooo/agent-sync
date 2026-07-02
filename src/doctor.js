import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { AGENT_SURFACES, isProjectGlob, resolveSurfaceRoot } from './surfaces.js';
import { classifyPath } from './classify.js';

const DEFAULT_MAX_DEPTH = 4;
const DEFAULT_MAX_ENTRIES = 500;

async function lstatIfExists(filePath) {
  try {
    return await fs.lstat(filePath);
  } catch (error) {
    if (error && error.code === 'ENOENT') return null;
    if (error && error.code === 'EACCES') return { inaccessible: true, error };
    throw error;
  }
}

function toIso(stats) {
  return stats && stats.mtime ? stats.mtime.toISOString() : null;
}

function redactBase(filePath, baseDir, marker) {
  const normalizedBase = path.resolve(baseDir);
  const normalized = path.resolve(filePath);
  if (normalized === normalizedBase) return marker;
  if (normalized.startsWith(normalizedBase + path.sep)) return marker + normalized.slice(normalizedBase.length);
  return null;
}

function redactHome(filePath, homeDir) {
  return redactBase(filePath, homeDir, '~') || filePath;
}

function portablePath(filePath, options) {
  const portable = redactBase(filePath, options.project, '<project>')
    || redactBase(filePath, options.home, '~')
    || filePath;
  return String(portable).replace(/\\/g, '/');
}

function rootRecord(filePath, scope, exists, options, note) {
  const record = {
    path: portablePath(filePath, options),
    scope,
    exists
  };
  if (note) record.note = note;
  return record;
}

function findingRecord(filePath, stats, agentId, options, depth) {
  const portable = portablePath(filePath, options);
  const classification = classifyPath(portable, stats, agentId);
  return {
    path: portable,
    portablePath: portable,
    kind: stats.isDirectory() ? 'directory' : stats.isSymbolicLink() ? 'symlink' : stats.isFile() ? 'file' : 'other',
    depth,
    size: stats.isFile() ? stats.size : null,
    mtime: toIso(stats),
    safetyClass: classification.safetyClass,
    risk: classification.risk,
    reason: classification.reason,
    recommendation: classification.recommendation
  };
}

function shouldPruneDirectory(filePath, options) {
  const portable = portablePath(filePath, options).replace(/\\/g, '/').toLowerCase();
  const segments = portable.split('/').map((segment) => segment.trim()).filter(Boolean);
  if (portable.includes('/plugins/cache/') || portable.endsWith('/plugins/cache')) return true;
  if (portable.includes('/.tmp/') || portable.endsWith('/.tmp')) return true;
  return segments.some((segment) => [
    'node_modules',
    'dist',
    'build',
    'out',
    'target',
    'vendor',
    '.git',
    'cache',
    'cached',
    'tmp',
    'temp',
    'logs',
    'log',
    'blob_storage',
    'gpucache',
    'code cache'
  ].includes(segment));
}

async function collectPath(filePath, options, agentId, depth = 0, results = []) {
  if (results.length >= options.maxEntries) return results;
  const stats = await lstatIfExists(filePath);
  if (!stats || stats.inaccessible) return results;

  results.push(findingRecord(filePath, stats, agentId, options, depth));
  if (!stats.isDirectory() || stats.isSymbolicLink() || depth >= options.maxDepth) return results;
  if (shouldPruneDirectory(filePath, options)) return results;

  let entries;
  try {
    entries = await fs.readdir(filePath, { withFileTypes: true });
  } catch (error) {
    results.push({
      path: portablePath(filePath, options),
      kind: 'directory',
      depth,
      safetyClass: 'unknown',
      risk: 'medium',
      reason: 'unable to read directory: ' + (error.code || error.message),
      recommendation: 'manual review required'
    });
    return results;
  }

  entries.sort((a, b) => a.name.localeCompare(b.name));
  for (const entry of entries) {
    if (results.length >= options.maxEntries) {
      results.push({
        path: portablePath(filePath, options),
        kind: 'directory',
        depth,
        safetyClass: 'unknown',
        risk: 'medium',
        reason: 'entry limit reached (' + options.maxEntries + ')',
        recommendation: 'rerun with --max-entries for fuller inventory',
        truncated: true
      });
      break;
    }
    await collectPath(path.join(filePath, entry.name), options, agentId, depth + 1, results);
  }
  return results;
}

async function expandProjectGlob(pattern, projectDir) {
  if (pattern !== '*.agent.md') return [];
  let entries;
  try {
    entries = await fs.readdir(projectDir, { withFileTypes: true });
  } catch {
    return [];
  }
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith('.agent.md'))
    .map((entry) => path.join(projectDir, entry.name));
}

function summarize(agents) {
  const summary = {
    agentsDetected: 0,
    findings: 0,
    bySafetyClass: {},
    byRisk: {}
  };
  for (const agent of agents) {
    if (agent.detected) summary.agentsDetected += 1;
    for (const finding of agent.findings) {
      summary.findings += 1;
      summary.bySafetyClass[finding.safetyClass] = (summary.bySafetyClass[finding.safetyClass] || 0) + 1;
      summary.byRisk[finding.risk] = (summary.byRisk[finding.risk] || 0) + 1;
    }
  }
  return summary;
}

export async function runDoctor(userOptions = {}) {
  const options = {
    home: path.resolve(userOptions.home || os.homedir()),
    project: path.resolve(userOptions.project || process.cwd()),
    platform: userOptions.platform || os.platform(),
    arch: userOptions.arch || os.arch(),
    maxDepth: Number.isFinite(userOptions.maxDepth) ? userOptions.maxDepth : DEFAULT_MAX_DEPTH,
    maxEntries: Number.isFinite(userOptions.maxEntries) ? userOptions.maxEntries : DEFAULT_MAX_ENTRIES
  };

  const agents = [];
  for (const surface of AGENT_SURFACES) {
    const agent = {
      id: surface.id,
      name: surface.name,
      detected: false,
      roots: [],
      findings: []
    };

    for (const root of surface.roots) {
      const rootPaths = isProjectGlob(root)
        ? await expandProjectGlob(root.pattern, options.project)
        : [resolveSurfaceRoot(root, options)].filter(Boolean);

      if (rootPaths.length === 0 && isProjectGlob(root)) {
        agent.roots.push(rootRecord(path.join(options.project, root.pattern), 'projectGlob', false, options, 'no matching files'));
      }

      for (const rootPath of rootPaths) {
        const stats = await lstatIfExists(rootPath);
        const exists = Boolean(stats && !stats.inaccessible);
        agent.roots.push(rootRecord(rootPath, root.scope, exists, options, stats && stats.inaccessible ? stats.error.code : undefined));
        if (!exists) continue;
        agent.detected = true;
        await collectPath(rootPath, options, agent.id, 0, agent.findings);
      }
    }

    agents.push(agent);
  }

  return {
    schemaVersion: '0.1',
    generatedAt: new Date().toISOString(),
    platform: {
      os: options.platform,
      arch: options.arch,
      node: process.version
    },
    inputs: {
      home: redactHome(options.home, options.home),
      project: options.project,
      maxDepth: options.maxDepth,
      maxEntries: options.maxEntries
    },
    summary: summarize(agents),
    agents
  };
}

export function renderDoctorReport(manifest) {
  const lines = [];
  lines.push('agent-sync doctor');
  lines.push('=================');
  lines.push('Generated: ' + manifest.generatedAt);
  lines.push('Platform: ' + manifest.platform.os + '/' + manifest.platform.arch + ' (' + manifest.platform.node + ')');
  lines.push('Project: ' + manifest.inputs.project);
  lines.push('');
  lines.push('Detected agents: ' + manifest.summary.agentsDetected + '/' + manifest.agents.length);
  lines.push('Findings: ' + manifest.summary.findings);
  lines.push('');

  lines.push('Safety classes:');
  const classes = Object.entries(manifest.summary.bySafetyClass).sort(([a], [b]) => a.localeCompare(b));
  if (classes.length === 0) lines.push('  - none');
  for (const [name, count] of classes) lines.push('  - ' + name + ': ' + count);
  lines.push('');

  for (const agent of manifest.agents) {
    lines.push((agent.detected ? '✓' : '·') + ' ' + agent.name + ' (' + agent.id + ')');
    const existingRoots = agent.roots.filter((root) => root.exists);
    if (existingRoots.length === 0) {
      lines.push('  no known roots detected');
      continue;
    }
    for (const root of existingRoots) lines.push('  root: ' + root.path);
    const topFindings = agent.findings.slice(0, 12);
    for (const finding of topFindings) {
      lines.push('  - [' + finding.safetyClass + '/' + finding.risk + '] ' + finding.path);
      lines.push('    ' + finding.reason);
    }
    if (agent.findings.length > topFindings.length) {
      lines.push('  ... ' + (agent.findings.length - topFindings.length) + ' more findings omitted from text report; use --json for full manifest');
    }
  }

  lines.push('');
  lines.push('Privacy: file contents were not read or printed. This is inventory only.');
  return lines.join('\n');
}
