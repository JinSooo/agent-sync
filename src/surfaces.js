import os from 'node:os';
import path from 'node:path';

const home = (...parts) => ({ scope: 'home', parts });
const project = (...parts) => ({ scope: 'project', parts });
const globProject = (pattern) => ({ scope: 'projectGlob', pattern });
const platformHome = (platforms) => ({ scope: 'platformHome', platforms });

export const AGENT_SURFACES = [
  {
    id: 'codex',
    name: 'OpenAI Codex',
    roots: [
      home('.codex'),
      project('.codex'),
      project('AGENTS.md')
    ]
  },
  {
    id: 'claude',
    name: 'Claude Code',
    roots: [
      home('.claude'),
      project('.claude'),
      project('CLAUDE.md'),
      project('CLAUDE.local.md'),
      project('.mcp.json')
    ]
  },
  {
    id: 'cursor',
    name: 'Cursor',
    roots: [
      home('.cursor'),
      project('.cursor'),
      platformHome({
        darwin: ['Library', 'Application Support', 'Cursor', 'User'],
        win32: ['AppData', 'Roaming', 'Cursor', 'User'],
        linux: ['.config', 'Cursor', 'User']
      })
    ]
  },
  {
    id: 'windsurf',
    name: 'Windsurf / Devin Desktop',
    roots: [
      home('.codeium'),
      home('.codeium', 'mcp_config.json'),
      project('.windsurf'),
      project('.devin')
    ]
  },
  {
    id: 'continue',
    name: 'Continue',
    roots: [
      home('.continue'),
      project('.continue')
    ]
  },
  {
    id: 'aider',
    name: 'Aider',
    roots: [
      home('.aider.conf.yml'),
      home('.aider.model.settings.yml'),
      home('.aider.model.metadata.json'),
      project('.aider.conf.yml'),
      project('.aider.chat.history.md'),
      project('.aider.input.history'),
      project('.aider.tags.cache.v4')
    ]
  },
  {
    id: 'github-copilot',
    name: 'GitHub Copilot / GitHub Coding Agent',
    roots: [
      project('.github', 'copilot-instructions.md'),
      project('.github', 'instructions'),
      project('.github', 'agents'),
      globProject('*.agent.md')
    ]
  }
];

export function resolveSurfaceRoot(root, inputs) {
  const platform = inputs.platform || os.platform();
  if (root.scope === 'home') return path.join(inputs.home, ...root.parts);
  if (root.scope === 'project') return path.join(inputs.project, ...root.parts);
  if (root.scope === 'platformHome') {
    const parts = root.platforms[platform];
    return parts ? path.join(inputs.home, ...parts) : null;
  }
  return null;
}

export function isProjectGlob(root) {
  return root.scope === 'projectGlob';
}
