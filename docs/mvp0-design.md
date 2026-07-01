# agent-sync MVP0 Design

MVP0 is a **read-only doctor + diff + plan loop** for AI coding agent state. It must explain what exists on a machine, classify risk, compare snapshots, and produce migration recipes without copying or printing sensitive contents.

## Goals

- Detect common local/project state surfaces for AI coding tools.
- Classify each finding by safety class and risk level.
- Produce a portable JSON manifest and a concise human report.
- Compare two manifests using portable paths such as `~/.codex/config.toml` and `<project>/AGENTS.md`.
- Generate a migration recipe with safe, review-required, and blocked groups.
- Support testing through `--home` and `--project` path overrides.

## Non-goals

- No secret/token migration.
- No raw transcript/session migration.
- No binary/plugin copying.
- No live sync, daemon, cloud backend, or auto-apply.
- No parsing private vendor DBs.

## CLI

```bash
agent-sync doctor [--json] [--home PATH] [--project PATH] [--output PATH] [--max-depth N] [--max-entries N]
agent-sync diff --from FROM.json --to TO.json [--json] [--output PATH]
agent-sync plan --from FROM.json --to TO.json [--target windows|macos|linux] [--json] [--output PATH]
```

Default output is a human-readable report. `--json` prints machine-readable output. `--output` writes the selected output format to a file.

## Manifest shape

```json
{
  "schemaVersion": "0.1",
  "generatedAt": "2026-07-01T00:00:00.000Z",
  "platform": { "os": "darwin", "arch": "arm64" },
  "inputs": { "home": "~", "project": "/repo" },
  "summary": { "agentsDetected": 2, "findings": 10, "bySafetyClass": {} },
  "agents": [
    {
      "id": "codex",
      "name": "OpenAI Codex",
      "detected": true,
      "roots": [],
      "findings": [
        {
          "path": "~/.codex/config.toml",
          "portablePath": "~/.codex/config.toml",
          "safetyClass": "safe_config",
          "risk": "low-medium"
        }
      ]
    }
  ]
}
```

## Safety classes

- `safe_config`: user/project text config or instruction files that can usually be diffed.
- `mcp_config`: MCP server config; may include environment variable names or command paths; review before applying.
- `memory_knowledge`: durable memory/rules/knowledge files; private content risk, but not credentials by default.
- `secret_bearing`: auth/token/key/env-like files; never print contents or copy by default.
- `raw_session`: raw transcripts/chat/workspace state; inventory only by default.
- `database`: SQLite/LevelDB-like storage; do not live-sync.
- `executable`: hooks/scripts/binaries that can execute commands; recipe-only by default.
- `binary_or_cache`: caches/plugins/build artifacts; do not copy blindly.
- `unknown`: needs manual review.

## Diff output

`diff` compares manifest metadata only:

- agent presence: both / only-from / only-to / absent
- missing source findings on target
- target-only findings
- metadata changes: kind, size, safety class, risk, recommendation

It intentionally ignores mtime to avoid noisy false positives across machines.

## Plan output

`plan` treats `--from` as the source machine and `--to` as the target machine:

- `safeCandidates`: text config/instructions that can be copied or merged after review.
- `reviewRequired`: MCP configs, memory/rules, hooks/scripts, and unknown surfaces.
- `blocked`: secrets, raw sessions, databases, binary/cache/plugin artifacts.

The plan is a recipe only. It never applies changes.

## Privacy contract

MVP0 records only path, portable path, file kind, size, mtime, safety class, risk, and reason. It does **not** read or emit file contents. Future versions may add opt-in hashing for safe text files, but secrets/raw sessions remain excluded by default.
