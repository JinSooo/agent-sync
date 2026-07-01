import path from 'node:path';

const TEXT_EXTENSIONS = new Set([
  '.md', '.mdx', '.txt', '.json', '.jsonc', '.toml', '.yaml', '.yml', '.ini', '.conf', '.cfg'
]);

const EXECUTABLE_EXTENSIONS = new Set([
  '.sh', '.bash', '.zsh', '.fish', '.ps1', '.bat', '.cmd', '.js', '.mjs', '.cjs', '.ts', '.py', '.rb', '.pl'
]);

const BINARY_OR_CACHE_EXTENSIONS = new Set([
  '.zip', '.tar', '.gz', '.tgz', '.rar', '.7z', '.bin', '.exe', '.dll', '.dylib', '.so', '.wasm', '.node', '.png', '.jpg', '.jpeg', '.gif', '.webp', '.ico', '.pdf'
]);

const SECRET_RE = /(^|[/._-])(auth|token|tokens|secret|secrets|credential|credentials|keychain|apikey|api_key|oauth|cookie|cookies|session_token)([/._-]|$)/i;
const ENV_RE = /(^|\/)\.env(\.|$)|(^|\/)env(\.local|\.production|\.development)?$/i;
const SESSION_RE = /(^|[/._-])(session|sessions|conversation|conversations|chat-history|chat_history|workspaceStorage|workspace-storage|history)([/._-]|$)/i;
const MEMORY_RE = /(^|[/._-])(memory|memories|rules|instructions|agents|skills|prompts|knowledge|claude-mem|memo)([/._-]|$)/i;
const MCP_RE = /(^|[/._-])(mcp|mcpServers|mcp_servers|mcp_config)([/._-]|$)|\.mcp\.json$/i;
const DB_RE = /\.(sqlite|sqlite3|db|ldb|log)$/i;
const CACHE_RE = /(^|[/._-])(cache|cached|tmp|temp|node_modules|dist|build|out|logs?|lockfile|blob_storage|gpucache|code cache)([/._-]|$)/i;
const HOOK_RE = /(^|[/._-])(hook|hooks|scripts?|bin)([/._-]|$)/i;

function normalizedPath(filePath) {
  return filePath.split(path.sep).join('/');
}

function fileKind(stats) {
  if (stats.isDirectory()) return 'directory';
  if (stats.isSymbolicLink()) return 'symlink';
  if (stats.isFile()) return 'file';
  return 'other';
}

export function classifyPath(filePath, stats, agentId = 'unknown') {
  const rel = normalizedPath(filePath);
  const base = path.basename(filePath);
  const ext = path.extname(base).toLowerCase();
  const kind = fileKind(stats);

  if (SECRET_RE.test(rel) || ENV_RE.test(rel)) {
    return {
      safetyClass: 'secret_bearing',
      risk: 'critical',
      reason: 'path suggests credentials, tokens, cookies, OAuth state, or env secrets',
      recommendation: 'detect only; never print contents or copy by default'
    };
  }

  if (SESSION_RE.test(rel)) {
    return {
      safetyClass: 'raw_session',
      risk: 'high',
      reason: 'path suggests raw chat, session, history, or workspace state',
      recommendation: 'inventory only; migrate only through explicit offline adapter with backups'
    };
  }

  if (DB_RE.test(base) || rel.includes('/leveldb/') || rel.includes('/IndexedDB/')) {
    return {
      safetyClass: 'database',
      risk: 'high',
      reason: 'database-like storage can corrupt or mismatch across app versions and paths',
      recommendation: 'do not live-sync; use offline export/import only'
    };
  }

  if (MCP_RE.test(rel)) {
    return {
      safetyClass: 'mcp_config',
      risk: 'high',
      reason: 'MCP config can expose tools, commands, paths, and env references',
      recommendation: 'diff structure and require per-tool approval on target machine'
    };
  }

  if (HOOK_RE.test(rel) || EXECUTABLE_EXTENSIONS.has(ext)) {
    return {
      safetyClass: 'executable',
      risk: 'high',
      reason: 'file may run commands as part of hooks, scripts, skills, or plugins',
      recommendation: 'show path and hash in future; install from trusted source rather than blind copy'
    };
  }

  if (CACHE_RE.test(rel) || BINARY_OR_CACHE_EXTENSIONS.has(ext)) {
    return {
      safetyClass: 'binary_or_cache',
      risk: 'medium-high',
      reason: 'cache, binary, plugin artifact, or generated file',
      recommendation: 'exclude from migration recipes unless explicitly supported'
    };
  }

  if (MEMORY_RE.test(rel)) {
    return {
      safetyClass: 'memory_knowledge',
      risk: agentId === 'github-copilot' ? 'medium' : 'medium-high',
      reason: 'path suggests rules, instructions, skills, prompts, agents, or durable memory',
      recommendation: 'diff carefully; preserve project/team source-of-truth boundaries'
    };
  }

  if (TEXT_EXTENSIONS.has(ext) || kind === 'directory') {
    return {
      safetyClass: 'safe_config',
      risk: 'low-medium',
      reason: 'text config or directory metadata suitable for read-only inventory',
      recommendation: 'safe to include in manifest; review before applying changes'
    };
  }

  return {
    safetyClass: 'unknown',
    risk: 'medium',
    reason: 'unrecognized state surface',
    recommendation: 'manual review before migration'
  };
}
