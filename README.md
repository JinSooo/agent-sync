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

- Scan local Codex and Claude Code surfaces without printing file contents, including adapter capabilities that drive UI availability.
- Export a verified `.asbundle` containing the source snapshot, safe text payloads, explicitly selected memory/MCP review payloads, metadata-only session archive entries, and explicitly selected raw session payloads. Sensitive memory/MCP and raw session payload export is protected with age encryption when either a passphrase, local device key, or one/more public recipient files are provided; unencrypted sensitive export still requires an explicit acknowledgement.
- Import and verify a remote `.asbundle` in the desktop UI.
- Create a remote-to-local transform plan.
- Show project mapping confidence by normalized git remote, directory basename, or manual fallback, while clearly warning when the adapter does not support DB/index-level project remap.
- Select auto-safe operations visually.
- Choose memory/rules/prompts and MCP config payloads for explicit review export, then apply selected review payloads only after an acknowledgement gate.
- Choose local Codex/Claude sessions whose raw payloads should be included in the next bundle, gated by adapter export capability.
- Choose remote Codex/Claude session archives and import them into the local Agent Sync Studio archive store with target-project mapping; native-file actions are gated by adapter import capability.
- Stage selected raw session payloads into an isolated native-import directory with optional source-project to target-project path rewriting.
- Check native session import readiness before writing, including raw-payload presence, adapter capability, stopped-agent preflight posture, rollback limits, and DB/index remap gaps.
- Discover native Codex/Claude DB/index store candidates in read-only mode, including SQLite table/column schema summaries without row contents.
- Preview likely SQLite project-remap columns for native Codex/Claude DB/index stores in schema-only mode; the preview reports confidence without reading rows, and writes only happen through the separate explicit apply step.
- Apply explicitly selected SQLite project-remap candidates with exact source-project matching, stopped-agent preflight, whole-database backup, transaction commit, row-count journal, local journal persistence, and backup restore rollback.
- Import selected raw session payloads into the target home as native Codex/Claude session files, limited to `~/.codex/**` and `~/.claude/**`, with backup, path rewriting, checksum journal, default stopped-agent preflight for Codex/Claude, and native-file rollback.
- Apply selected safe payloads with backups and checksum verification.
- Roll back apply journals and native session import journals by restoring backed-up files or removing files that did not exist before the apply/import.
- Persist snapshots, apply journals, and native session import journals in a local SQLite record store so rollback points survive app restarts.

## Commands

```bash
# Rust core
cargo test --workspace
cargo run -p agent_sync_cli -- scan
cargo run -p agent_sync_cli -- bundle-manifest
cargo run -p agent_sync_cli -- generate-bundle-key --output agent-sync-device-key.json
cargo run -p agent_sync_cli -- export-bundle-recipient --bundle-key agent-sync-device-key.json --output agent-sync-recipient.json
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-local.asbundle
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-review.asbundle --payload "codex:~/.codex/memories/guide.md" --payload "claude:~/.claude/mcp.json" --bundle-key agent-sync-device-key.json
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-sessions.asbundle --max-depth 8 --max-entries 5000 --include-session-payloads --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl" --bundle-key agent-sync-device-key.json --bundle-recipient windows-agent-sync-recipient.json
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- verify-bundle --input agent-sync-sessions.asbundle
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- check-native-sessions --input agent-sync-sessions.asbundle --home "$HOME" --project "$PWD"
cargo run -p agent_sync_cli -- discover-native-stores --home "$HOME" --project "$PWD" --max-depth 8 --max-entries 5000
cargo run -p agent_sync_cli -- preview-native-remap --home "$HOME" --project "$PWD" --source-project "/source/project" --max-depth 8 --max-entries 5000
cargo run -p agent_sync_cli -- apply-native-remap --home "$HOME" --project "$PWD" --source-project "/source/project" --candidate 'codex|~/.codex/state/sessions.db|conversations|project_path' --backup-dir agent-sync-backups
cargo run -p agent_sync_cli -- rollback-native-remap-journal --input agent-sync-native-remap-journal.json
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- import-native-sessions --input agent-sync-sessions.asbundle --target-home "$HOME" --target-project "$PWD" --backup-dir agent-sync-backups --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl"
# Explicit override when you have manually accepted the risk of importing while the target agent may be running:
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- import-native-sessions --input agent-sync-sessions.asbundle --target-home "$HOME" --target-project "$PWD" --backup-dir agent-sync-backups --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl" --skip-agent-stopped-check
cargo run -p agent_sync_cli -- rollback-journal --input agent-sync-journal.json
cargo run -p agent_sync_cli -- rollback-native-session-journal --input agent-sync-native-session-journal.json
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

Sensitive raw payloads are encrypted at the whole-bundle file boundary when a passphrase, Agent Sync device key file, or public recipient is provided. Use the desktop Bundle passphrase field, desktop private key picker, desktop public recipient list, CLI `--bundle-passphrase` / `AGENT_SYNC_BUNDLE_PASSPHRASE`, CLI `--bundle-key` / `AGENT_SYNC_BUNDLE_KEY`, or repeated CLI `--bundle-recipient AGE_OR_JSON`. Importing, verifying, checking, or native-importing an encrypted bundle requires the matching passphrase or private key file. The private key file contains an age identity and should be stored/transferred like a secret; public recipient files contain only the age recipient and are safe to share with the exporting device. Exporting memory/MCP review payloads or raw session payloads without encryption still requires an explicit UI acknowledgement or CLI `--allow-unencrypted-sensitive-payloads`.

Automatically applicable today:

- safe text config payloads from a verified bundle, only through selected operations, with backup and checksum verification.
- explicitly selected `memory_knowledge` and `mcp_config` text payloads from a verified bundle, only after the review-acknowledgement gate, with backup and checksum verification.
- apply journal rollback for backup-backed changes and files created by the apply.
- automatic apply-journal and native session import-journal persistence into the local SQLite store, with UI loading of stored rollback points.
- metadata-only session archive records into Agent Sync Studio SQLite storage.
- selected raw session payloads into an isolated staging directory with project-path rewrite journal.
- native session import readiness reports in desktop and CLI that are read-only and explicitly warn when the current adapter supports native-file import but not Codex/Claude DB/index project remap.
- read-only native session store discovery in desktop and CLI, including SQLite schema metadata only; this is evidence gathering for future DB/index remap and does not write native stores.
- read-only native DB/index project-remap preview in desktop and CLI, using SQLite schema column names only. It does not read database rows, does not mutate native stores, and reports every candidate with `write_supported=false`.
- explicitly selected SQLite DB project-remap candidates in desktop and CLI, with exact `source_project` matching, whole-DB backup, SQLite transaction, row-count journal, stopped-agent preflight by default, local journal persistence, and rollback by restoring the DB backup. The UI exposes candidate checkboxes after preview; the CLI requires `--candidate 'AGENT|PORTABLE|TABLE|COLUMN'`. LevelDB/IndexedDB and unknown native stores remain preview-only.
- selected raw session payloads into native Codex/Claude file locations under a chosen target home, with strict `~/.codex/**` / `~/.claude/**` allowlisting, adapter capability gating, default Codex/Claude stopped-agent preflight, backup, optional project-path rewrite, checksum verification, and rollback from the native import journal. The UI exposes an explicit manual override and the CLI exposes `--skip-agent-stopped-check`; this does not rewrite opaque native Codex/Claude databases or secondary indexes, and the adapter capability model does not claim broad DB/index remap support until fixtures prove it.

See `.omx/plans/agent-sync-studio-full-architecture-20260701.md` and `docs/agent-sync-studio-architecture.md` for the full implementation architecture.
