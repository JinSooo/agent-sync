function findingKey(finding) {
  return finding.portablePath || finding.path;
}

function relevantFingerprint(finding) {
  return JSON.stringify({
    kind: finding.kind,
    size: finding.size,
    safetyClass: finding.safetyClass,
    risk: finding.risk,
    recommendation: finding.recommendation
  });
}

function indexFindings(agent) {
  const index = new Map();
  for (const finding of agent.findings || []) {
    index.set(findingKey(finding), finding);
  }
  return index;
}

function byAgent(manifest) {
  const index = new Map();
  for (const agent of manifest.agents || []) {
    index.set(agent.id, agent);
  }
  return index;
}

function increment(map, key) {
  map[key] = (map[key] || 0) + 1;
}

function summarizeAgentDiff(agentDiff, summary) {
  if (agentDiff.status === 'only_from') summary.agents.onlyFrom += 1;
  if (agentDiff.status === 'only_to') summary.agents.onlyTo += 1;
  if (agentDiff.status === 'both') summary.agents.both += 1;
  if (agentDiff.status === 'absent') summary.agents.absent += 1;

  summary.findings.onlyFrom += agentDiff.onlyFrom.length;
  summary.findings.onlyTo += agentDiff.onlyTo.length;
  summary.findings.changed += agentDiff.changed.length;
  summary.findings.unchanged += agentDiff.unchanged;

  for (const finding of [...agentDiff.onlyFrom, ...agentDiff.onlyTo]) {
    increment(summary.bySafetyClass, finding.safetyClass);
    increment(summary.byRisk, finding.risk);
  }
  for (const changed of agentDiff.changed) {
    increment(summary.bySafetyClass, changed.from.safetyClass);
    increment(summary.bySafetyClass, changed.to.safetyClass);
    increment(summary.byRisk, changed.from.risk);
    increment(summary.byRisk, changed.to.risk);
  }
}

export function runDiff(fromManifest, toManifest, options = {}) {
  const fromAgents = byAgent(fromManifest);
  const toAgents = byAgent(toManifest);
  const agentIds = [...new Set([...fromAgents.keys(), ...toAgents.keys()])].sort();
  const agents = [];
  const summary = {
    agents: {
      onlyFrom: 0,
      onlyTo: 0,
      both: 0,
      absent: 0
    },
    findings: {
      onlyFrom: 0,
      onlyTo: 0,
      changed: 0,
      unchanged: 0
    },
    bySafetyClass: {},
    byRisk: {}
  };

  for (const id of agentIds) {
    const fromAgent = fromAgents.get(id);
    const toAgent = toAgents.get(id);
    const status = fromAgent && toAgent
      ? fromAgent.detected && toAgent.detected
        ? 'both'
        : fromAgent.detected
          ? 'only_from'
          : toAgent.detected
            ? 'only_to'
            : 'absent'
      : fromAgent
        ? 'only_from'
        : 'only_to';
    const name = (fromAgent && fromAgent.name) || (toAgent && toAgent.name) || id;
    const fromIndex = fromAgent ? indexFindings(fromAgent) : new Map();
    const toIndex = toAgent ? indexFindings(toAgent) : new Map();
    const keys = [...new Set([...fromIndex.keys(), ...toIndex.keys()])].sort();
    const agentDiff = {
      id,
      name,
      status,
      onlyFrom: [],
      onlyTo: [],
      changed: [],
      unchanged: 0
    };

    for (const key of keys) {
      const fromFinding = fromIndex.get(key);
      const toFinding = toIndex.get(key);
      if (fromFinding && !toFinding) {
        agentDiff.onlyFrom.push(fromFinding);
      } else if (!fromFinding && toFinding) {
        agentDiff.onlyTo.push(toFinding);
      } else if (relevantFingerprint(fromFinding) !== relevantFingerprint(toFinding)) {
        agentDiff.changed.push({
          path: key,
          from: fromFinding,
          to: toFinding,
          changedFields: changedFields(fromFinding, toFinding)
        });
      } else {
        agentDiff.unchanged += 1;
      }
    }

    summarizeAgentDiff(agentDiff, summary);
    agents.push(agentDiff);
  }

  return {
    schemaVersion: '0.1',
    generatedAt: new Date().toISOString(),
    kind: 'agent-sync-diff',
    from: {
      label: options.fromLabel || 'from',
      generatedAt: fromManifest.generatedAt,
      platform: fromManifest.platform,
      project: fromManifest.inputs && fromManifest.inputs.project
    },
    to: {
      label: options.toLabel || 'to',
      generatedAt: toManifest.generatedAt,
      platform: toManifest.platform,
      project: toManifest.inputs && toManifest.inputs.project
    },
    summary,
    agents
  };
}

function changedFields(fromFinding, toFinding) {
  const fields = [];
  for (const field of ['kind', 'size', 'safetyClass', 'risk', 'recommendation']) {
    if (fromFinding[field] !== toFinding[field]) fields.push(field);
  }
  return fields;
}

function formatCountMap(map) {
  const entries = Object.entries(map).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0) return ['  - none'];
  return entries.map(([key, count]) => '  - ' + key + ': ' + count);
}

export function renderDiffReport(diff) {
  const lines = [];
  lines.push('agent-sync diff');
  lines.push('===============');
  lines.push('Generated: ' + diff.generatedAt);
  lines.push('From: ' + diff.from.label + ' (' + platformLabel(diff.from.platform) + ')');
  lines.push('To: ' + diff.to.label + ' (' + platformLabel(diff.to.platform) + ')');
  lines.push('');
  lines.push('Agents: both=' + diff.summary.agents.both + ', only-from=' + diff.summary.agents.onlyFrom + ', only-to=' + diff.summary.agents.onlyTo + ', absent=' + diff.summary.agents.absent);
  lines.push('Findings: only-from=' + diff.summary.findings.onlyFrom + ', only-to=' + diff.summary.findings.onlyTo + ', changed=' + diff.summary.findings.changed + ', unchanged=' + diff.summary.findings.unchanged);
  lines.push('');
  lines.push('Safety classes involved:');
  lines.push(...formatCountMap(diff.summary.bySafetyClass));
  lines.push('');

  for (const agent of diff.agents) {
    const hasChanges = agent.onlyFrom.length || agent.onlyTo.length || agent.changed.length || agent.status === 'only_from' || agent.status === 'only_to';
    if (!hasChanges) continue;
    lines.push(agent.name + ' (' + agent.id + ') — ' + agent.status);
    for (const finding of agent.onlyFrom.slice(0, 10)) {
      lines.push('  - missing on target: [' + finding.safetyClass + '/' + finding.risk + '] ' + findingKey(finding));
    }
    if (agent.onlyFrom.length > 10) lines.push('  ... ' + (agent.onlyFrom.length - 10) + ' more only-from findings');
    for (const finding of agent.onlyTo.slice(0, 5)) {
      lines.push('  - target-only: [' + finding.safetyClass + '/' + finding.risk + '] ' + findingKey(finding));
    }
    if (agent.onlyTo.length > 5) lines.push('  ... ' + (agent.onlyTo.length - 5) + ' more only-to findings');
    for (const changed of agent.changed.slice(0, 5)) {
      lines.push('  - changed: ' + changed.path + ' (' + changed.changedFields.join(', ') + ')');
    }
    if (agent.changed.length > 5) lines.push('  ... ' + (agent.changed.length - 5) + ' more changed findings');
  }

  lines.push('');
  lines.push('Privacy: diff compares manifest metadata only; file contents are not read.');
  return lines.join('\n');
}

function platformLabel(platform) {
  if (!platform) return 'unknown';
  return (platform.os || 'unknown') + '/' + (platform.arch || 'unknown');
}

export { findingKey };
