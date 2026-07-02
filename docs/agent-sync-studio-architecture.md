# Agent Sync Studio Architecture

Agent Sync Studio is the Tauri-first product line for cross-platform Codex and Claude Code state sync.

The product syncs agent working context, not raw folders:

```text
Scan -> Select -> Transform -> Preview -> Apply -> Verify/Rollback
```

## Runtime split

- React/Vite frontend: control plane only, including native Tauri dialog pickers for bundle/project/backup paths.
- Tauri commands: narrow IPC boundary.
- Rust crates: all filesystem, bundle, transform, apply, backup, and storage work.

The frontend must not directly read or write `~/.codex`, `~/.claude`, raw sessions, or app databases.

## Crates

| Crate | Responsibility |
| --- | --- |
| `agent_sync_core` | Domain models, safety classes, path redaction, classification |
| `agent_sync_scan` | Codex/Claude surface scan and metadata-only session discovery |
| `agent_sync_transform` | Snapshot diff, project mapping, and transform-plan generation |
| `agent_sync_bundle` | `.asbundle` source snapshot, manifest, payload, redaction, and checksum handling |
| `agent_sync_apply` | Preflight, operation journal, safe payload apply with backups |
| `agent_sync_storage` | Agent Sync Studio local SQLite record store |
| `agent_sync_adapters_codex` | Codex adapter capabilities and session metadata entry point |
| `agent_sync_adapters_claude` | Claude Code adapter capabilities and session metadata entry point |
| `agent_sync_platform` | Platform path utility entry point |
| `agent_sync_cli` | Rust CLI using the same core as desktop |

## Safety policy

Blocked from export/apply:

- credentials, tokens, OAuth/cookies, env-like files
- direct live database patching
- binary/cache/plugin artifacts without adapter support

Review-required:

- raw sessions/transcripts
- memory/rules/skills/prompts/agents
- MCP configs
- hooks/scripts/commands

Automatically applicable today:

- safe text config payloads from a verified bundle, with backup and checksum verification.

## Current implementation status

Implemented:

- Tauri 2 desktop shell with React/Vite UI.
- Rust scan/diff/transform/preflight/journal commands.
- Real `.asbundle` JSON container with source snapshot, payload checksums, metadata-only session archive entries, and secret redactions.
- Local SQLite store for snapshots/plans/journals as JSON records.
- Safe config apply path with visual operation selection, backup, operation journal, and checksum verification.
- Session Library flow: choose remote Codex/Claude session archives, bind them to the target project path, and import metadata-only records into local Agent Sync Studio SQLite storage.
- Rust CLI: `scan`, `bundle-manifest`, `export-bundle`, `verify-bundle`, `self-plan`.

Implemented in the current product loop:

- Native file picker flow for bundle import/export, target project, and backup directory.
- Project-mapping UI with git-remote exact match, basename fallback, confidence, and manual-review warning.

Still to deepen:
- Codex native session import/remap beyond Agent Sync Studio archive storage; raw session identity rewrite remains review-only until adapter-specific import is implemented.
- Claude Code native session import/remap beyond Agent Sync Studio archive storage; raw session identity rewrite remains review-only until adapter-specific import is implemented.
- Encrypted bundle payloads for sensitive selected content.
