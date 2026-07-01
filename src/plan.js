import { findingKey, runDiff } from './diff.js';

const ACTIONS = {
  safe_config: {
    bucket: 'safeCandidates',
    action: 'copy_or_merge_text_config',
    rationale: 'text config or instruction metadata can usually be reviewed and merged'
  },
  memory_knowledge: {
    bucket: 'reviewRequired',
    action: 'review_memory_or_rule_merge',
    rationale: 'memory/rules may contain private or behavior-changing context'
  },
  mcp_config: {
    bucket: 'reviewRequired',
    action: 'review_mcp_config_and_recreate_env',
    rationale: 'MCP can expose tools, commands, paths, and env references'
  },
  executable: {
    bucket: 'reviewRequired',
    action: 'review_executable_before_install',
    rationale: 'hooks/scripts/plugins may execute commands'
  },
  secret_bearing: {
    bucket: 'blocked',
    action: 'do_not_copy_reauth_or_secret_manager_reference',
    rationale: 'credentials and tokens must not be copied blindly'
  },
  raw_session: {
    bucket: 'blocked',
    action: 'do_not_live_sync_raw_session',
    rationale: 'raw transcripts/session stores need explicit offline adapter and backups'
  },
  database: {
    bucket: 'blocked',
    action: 'do_not_live_sync_database',
    rationale: 'database stores can corrupt or depend on app versions and path hashes'
  },
  binary_or_cache: {
    bucket: 'blocked',
    action: 'exclude_binary_or_cache',
    rationale: 'generated caches and binary/plugin artifacts should be recreated or installed'
  },
  unknown: {
    bucket: 'reviewRequired',
    action: 'manual_review_unknown_surface',
    rationale: 'unknown state surface needs manual classification'
  }
};

function classifyAction(finding) {
  return ACTIONS[finding.safetyClass] || ACTIONS.unknown;
}

function addItem(plan, finding, agent, changeType, extra = {}) {
  const action = classifyAction(finding);
  const item = {
    agentId: agent.id,
    agentName: agent.name,
    path: findingKey(finding),
    safetyClass: finding.safetyClass,
    risk: finding.risk,
    changeType,
    action: action.action,
    rationale: action.rationale,
    recommendation: finding.recommendation,
    ...extra
  };
  plan[action.bucket].push(item);
}

function pathWarning(pathValue, targetPlatform) {
  const normalizedTarget = normalizeTarget(targetPlatform);
  const warnings = [];
  if (/^[A-Za-z]:[\\/]/.test(pathValue)) warnings.push('windows_absolute_path');
  if (pathValue.startsWith('/Users/') || pathValue.startsWith('/home/')) warnings.push('posix_absolute_path');
  if (normalizedTarget === 'windows' && pathValue.includes('/')) warnings.push('check_windows_path_mapping');
  if ((normalizedTarget === 'macos' || normalizedTarget === 'linux') && /^[A-Za-z]:[\\/]/.test(pathValue)) warnings.push('check_posix_path_mapping');
  return warnings;
}

function normalizeTarget(targetPlatform) {
  if (targetPlatform === 'win32') return 'windows';
  if (targetPlatform === 'darwin') return 'macos';
  return targetPlatform;
}

export function runPlan(fromManifest, toManifest, options = {}) {
  const diff = options.diff || runDiff(fromManifest, toManifest, options);
  const targetPlatform = normalizeTarget(options.target || (diff.to.platform && diff.to.platform.os) || 'unknown');
  const plan = {
    schemaVersion: '0.1',
    generatedAt: new Date().toISOString(),
    kind: 'agent-sync-plan',
    target: targetPlatform,
    from: diff.from,
    to: diff.to,
    summary: {
      safeCandidates: 0,
      reviewRequired: 0,
      blocked: 0,
      changed: 0,
      missingOnTarget: 0
    },
    safeCandidates: [],
    reviewRequired: [],
    blocked: [],
    notes: [
      'Plan is a recipe, not an auto-apply operation.',
      'No file contents are read by this command.',
      'Secrets, raw sessions, databases, and binary/cache artifacts are blocked by default.'
    ]
  };

  for (const agent of diff.agents) {
    for (const finding of agent.onlyFrom) {
      addItem(plan, finding, agent, 'missing_on_target', {
        pathWarnings: pathWarning(findingKey(finding), targetPlatform)
      });
      plan.summary.missingOnTarget += 1;
    }
    for (const changed of agent.changed) {
      addItem(plan, changed.from, agent, 'changed_between_snapshots', {
        changedFields: changed.changedFields,
        pathWarnings: pathWarning(changed.path, targetPlatform)
      });
      plan.summary.changed += 1;
    }
  }

  plan.summary.safeCandidates = plan.safeCandidates.length;
  plan.summary.reviewRequired = plan.reviewRequired.length;
  plan.summary.blocked = plan.blocked.length;
  return plan;
}

export function renderPlanReport(plan) {
  const lines = [];
  lines.push('agent-sync plan');
  lines.push('===============');
  lines.push('Generated: ' + plan.generatedAt);
  lines.push('Target: ' + plan.target);
  lines.push('');
  lines.push('Summary: safe=' + plan.summary.safeCandidates + ', review=' + plan.summary.reviewRequired + ', blocked=' + plan.summary.blocked + ', missing=' + plan.summary.missingOnTarget + ', changed=' + plan.summary.changed);
  lines.push('');

  renderBucket(lines, 'Safe candidates', plan.safeCandidates, 12);
  renderBucket(lines, 'Review required', plan.reviewRequired, 12);
  renderBucket(lines, 'Blocked by default', plan.blocked, 12);

  lines.push('Notes:');
  for (const note of plan.notes) lines.push('  - ' + note);
  return lines.join('\n');
}

function renderBucket(lines, title, items, limit) {
  lines.push(title + ':');
  if (items.length === 0) {
    lines.push('  - none');
    lines.push('');
    return;
  }
  for (const item of items.slice(0, limit)) {
    lines.push('  - ' + item.path + ' [' + item.agentId + ', ' + item.safetyClass + '/' + item.risk + ']');
    lines.push('    action: ' + item.action);
    if (item.pathWarnings && item.pathWarnings.length) lines.push('    warnings: ' + item.pathWarnings.join(', '));
  }
  if (items.length > limit) lines.push('  ... ' + (items.length - limit) + ' more');
  lines.push('');
}
