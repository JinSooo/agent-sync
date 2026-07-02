# Agent Sync Studio

`agent-sync` is becoming a Tauri-first cross-platform visual sync tool for AI coding agent state. The product target is not raw file copying; it is a safe migration loop for the working context behind Codex and Claude Code.

```text
Scan -> Select -> Transform -> Preview -> Apply -> Verify/Rollback
```

## Current architecture

- `apps/desktop`: Tauri 2 + React/Vite desktop control plane with native bundle/home/project/backup path pickers.
- `crates/agent_sync_core`: domain models, safety classes, path redaction/classification.
- `crates/agent_sync_scan`: Rust scanner for Codex and Claude Code surfaces.
- `crates/agent_sync_transform`: snapshot diff, project-mapping suggestions, and transform-plan generation.
- `crates/agent_sync_bundle`: `.asbundle` source snapshot, manifest, payload, checksum, and redaction handling.
- `crates/agent_sync_apply`: preflight, operation journal, safe payload apply, session native-file import, backup, and checksum verification.
- `crates/agent_sync_adapters_codex`: Codex adapter capability and session metadata entry points.
- `crates/agent_sync_adapters_claude`: Claude Code adapter capability and session metadata entry points.
- `crates/agent_sync_cli`: Rust CLI that uses the same core as the desktop app.

The older Node CLI remains available as a legacy reference while the Rust/Tauri product line takes over.

## What works now

- Scan local Codex and Claude Code surfaces without printing file contents.
- Export a verified `.asbundle` containing the source snapshot, safe text payloads, explicitly selected memory/MCP review payloads, metadata-only session archive entries, and explicitly selected raw session payloads.
- Import and verify a remote `.asbundle` in the desktop UI.
- Create a remote-to-local transform plan.
- Show project mapping confidence by normalized git remote, directory basename, or manual fallback.
- Select auto-safe operations visually.
- Choose memory/rules/prompts and MCP config payloads for explicit review export, then apply selected review payloads only after an acknowledgement gate.
- Choose local Codex/Claude sessions whose raw payloads should be included in the next bundle.
- Choose remote Codex/Claude session archives and import them into the local Agent Sync Studio archive store with target-project mapping.
- Stage selected raw session payloads into an isolated native-import directory with optional source-project to target-project path rewriting.
- Import selected raw session payloads into the target home as native Codex/Claude session files, limited to `~/.codex/**` and `~/.claude/**`, with backup, path rewriting, and checksum journal.
- Apply selected safe payloads with backups and checksum verification.
- Persist snapshots in a local SQLite record store.

## Commands

```bash
# Rust core
cargo test --workspace
cargo run -p agent_sync_cli -- scan
cargo run -p agent_sync_cli -- bundle-manifest
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-local.asbundle
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-review.asbundle --payload "codex:~/.codex/memories/guide.md" --payload "claude:~/.claude/mcp.json"
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-sessions.asbundle --max-depth 8 --max-entries 5000 --include-session-payloads --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl"
cargo run -p agent_sync_cli -- verify-bundle --input agent-sync-local.asbundle
cargo run -p agent_sync_cli -- import-native-sessions --input agent-sync-sessions.asbundle --target-home "$HOME" --target-project "$PWD" --backup-dir agent-sync-backups --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl"
cargo run -p agent_sync_cli -- self-plan

# Desktop frontend and app
pnpm install
pnpm --dir apps/desktop build
pnpm --dir apps/desktop tauri build --no-bundle
pnpm --dir apps/desktop tauri:dev

# Legacy Node reference
npm run check && npm test
```

## Safety policy

Blocked by product policy:

- credentials, tokens, cookies, OAuth state, `.env`-like files
- direct live patching of Codex/Claude databases while the app is running
- binary/cache/plugin artifacts without adapter support
- apply operations without backup and verification journal entries

Review-required by default:

- raw chat/session/transcript stores
- MCP configs
- hooks/scripts/commands
- memory/rules/skills/prompts/agents

Automatically applicable today:

- safe text config payloads from a verified bundle, only through selected operations, with backup and checksum verification.
- explicitly selected `memory_knowledge` and `mcp_config` text payloads from a verified bundle, only after the review-acknowledgement gate, with backup and checksum verification.
- metadata-only session archive records into Agent Sync Studio SQLite storage.
- selected raw session payloads into an isolated staging directory with project-path rewrite journal.
- selected raw session payloads into native Codex/Claude file locations under a chosen target home, with strict `~/.codex/**` / `~/.claude/**` allowlisting, backup, optional project-path rewrite, and checksum verification. This does not rewrite native Codex/Claude databases or secondary indexes.

See `.omx/plans/agent-sync-studio-full-architecture-20260701.md` and `docs/agent-sync-studio-architecture.md` for the full implementation architecture.
