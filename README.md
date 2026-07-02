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
- Export a verified `.asbundle` containing the source snapshot, safe text payloads, explicitly selected memory/MCP review payloads, metadata-only session archive entries, and explicitly selected raw session payloads. Sensitive memory/MCP and raw session payload export is protected with age encryption when either a passphrase, local device key file, OS keychain device key, or one/more public recipient files are provided; unencrypted sensitive export still requires an explicit acknowledgement.
- Save trusted public-recipient profiles with label/device/platform notes in the local SQLite store, then select them visually or by CLI profile id for repeat cross-device encrypted exports. Profiles store public age recipients only; private identities stay in a key file or OS keychain.
- Review stale trusted recipients with a rotation plan in desktop or CLI before using old remote public keys for new exports.
- Exchange public recipient inventory files between devices so a new machine can import trusted public recipients without copying private keys. Inventory imports deduplicate by age recipient and skip revoked profiles by default.
- Export and restore an OS keychain bundle identity through a passphrase-encrypted backup file for lost-device or machine-rebuild recovery. The backup contains the private age identity, so the file and passphrase must be stored separately.
- Import and verify a remote `.asbundle` in the desktop UI.
- Save local scans into the Agent Sync Studio SQLite store, refresh snapshot history, and reload a prior scan after restarting the app.
- Create a remote-to-local transform plan.
- Show project mapping confidence by normalized git remote, directory basename, or manual fallback, while clearly warning when the adapter does not support DB/index-level project remap.
- Select auto-safe operations visually.
- Choose memory/rules/prompts and MCP config payloads for explicit review export, then apply selected review payloads only after an acknowledgement gate.
- Search/filter local Codex/Claude sessions and memory/MCP payloads, then choose exactly which raw session payloads or review-required payloads should be included in the next bundle; raw session selection remains gated by adapter export capability.
- Search/filter remote Codex/Claude session archives and import selected archives into the local Agent Sync Studio archive store with global or per-session target-project mapping; native-file actions are gated by adapter import capability.
- Refresh persisted session archive history from the local SQLite store after app restart to review imported archive metadata; raw payload recovery still requires the original encrypted bundle or native import journal.
- Stage selected raw session payloads into an isolated native-import directory with optional source-project to target-project path rewriting; per-session target-project overrides win over the global target path.
- Check native session import readiness before writing, including raw-payload presence, adapter capability, stopped-agent preflight posture, rollback limits, and DB/index remap gaps.
- Discover native Codex/Claude DB/index store candidates in read-only mode, including SQLite table/column schema summaries without row contents.
- Generate a native compatibility evidence matrix in desktop or CLI that shows per-agent session-file support, SQLite exact-match remap candidates, opaque DB/index candidates, and whether the adapter deliberately avoids claiming broad DB/index remap.
- Preview likely SQLite project-remap columns for native Codex/Claude DB/index stores in schema-only mode; the preview reports confidence without reading rows, and writes only happen through the separate explicit apply step.
- Dry-run explicitly selected SQLite project-remap candidates with read-only exact source-project row counts before any DB write; the desktop apply button is locked until the current selection has a successful dry-run.
- Apply explicitly selected SQLite project-remap candidates with exact source-project matching, stopped-agent preflight, whole-database backup, transaction commit, row-count journal, local journal persistence, and backup restore rollback.
- Import selected raw session payloads into the target home as native Codex/Claude session files, limited to `~/.codex/**` and `~/.claude/**`, with backup, path rewriting, checksum journal, default stopped-agent preflight for Codex/Claude, and native-file rollback.
- Apply selected safe payloads with backups and checksum verification.
- Roll back apply journals and native session import journals by restoring backed-up files or removing files that did not exist before the apply/import.
- Persist snapshots, apply journals, and native session import journals in a local SQLite record store so rollback points survive app restarts.
- Gate pushes and pull requests with a GitHub Actions matrix for macOS, Windows, and Linux covering Rust tests, legacy Node checks, desktop web build, and Tauri no-bundle build.

## Commands

```bash
# Rust core
cargo test --workspace
cargo run -p agent_sync_cli -- scan
cargo run -p agent_sync_cli -- bundle-manifest
cargo run -p agent_sync_cli -- generate-bundle-key --output agent-sync-device-key.json
cargo run -p agent_sync_cli -- generate-bundle-keychain --bundle-keychain work-laptop
cargo run -p agent_sync_cli -- export-bundle-recipient --bundle-key agent-sync-device-key.json --output agent-sync-recipient.json
cargo run -p agent_sync_cli -- export-bundle-keychain-recipient --bundle-keychain work-laptop --output work-laptop-recipient.json
cargo run -p agent_sync_cli -- export-bundle-keychain-backup --bundle-keychain work-laptop --output work-laptop-key-backup.age --backup-passphrase "store-this-elsewhere"
cargo run -p agent_sync_cli -- restore-bundle-keychain-backup --bundle-keychain work-laptop-restored --input work-laptop-key-backup.age --backup-passphrase "store-this-elsewhere"
cargo run -p agent_sync_cli -- save-bundle-recipient-profile --store agent-sync-studio.sqlite --label "Windows desktop" --platform windows --recipient windows-agent-sync-recipient.json
cargo run -p agent_sync_cli -- list-bundle-recipient-profiles --store agent-sync-studio.sqlite
cargo run -p agent_sync_cli -- bundle-recipient-rotation-plan --store agent-sync-studio.sqlite --stale-days 90
cargo run -p agent_sync_cli -- export-bundle-recipient-inventory --store agent-sync-studio.sqlite --output macbook-recipient-inventory.json --label "MacBook Pro"
cargo run -p agent_sync_cli -- import-bundle-recipient-inventory --store agent-sync-studio.sqlite --input macbook-recipient-inventory.json
cargo run -p agent_sync_cli -- revoke-bundle-recipient-profile --store agent-sync-studio.sqlite --id "PROFILE_ID" --note "remote key rotated"
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-local.asbundle
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-review.asbundle --payload "codex:~/.codex/memories/guide.md" --payload "claude:~/.claude/mcp.json" --bundle-key agent-sync-device-key.json
cargo run -p agent_sync_cli -- export-bundle --store agent-sync-studio.sqlite --output agent-sync-review-profile.asbundle --payload "codex:~/.codex/memories/guide.md" --bundle-recipient-profile "PROFILE_ID"
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-review-keychain.asbundle --payload "codex:~/.codex/memories/guide.md" --bundle-keychain work-laptop
cargo run -p agent_sync_cli -- export-bundle --output agent-sync-sessions.asbundle --max-depth 8 --max-entries 5000 --include-session-payloads --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl" --bundle-key agent-sync-device-key.json --bundle-recipient windows-agent-sync-recipient.json
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- verify-bundle --input agent-sync-sessions.asbundle
AGENT_SYNC_BUNDLE_KEYCHAIN="work-laptop" cargo run -p agent_sync_cli -- verify-bundle --input agent-sync-review-keychain.asbundle
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- check-native-sessions --input agent-sync-sessions.asbundle --home "$HOME" --project "$PWD"
cargo run -p agent_sync_cli -- discover-native-stores --home "$HOME" --project "$PWD" --max-depth 8 --max-entries 5000
cargo run -p agent_sync_cli -- native-compatibility-report --home "$HOME" --project "$PWD" --max-depth 8 --max-entries 5000
cargo run -p agent_sync_cli -- preview-native-remap --home "$HOME" --project "$PWD" --source-project "/source/project" --max-depth 8 --max-entries 5000
cargo run -p agent_sync_cli -- dry-run-native-remap --home "$HOME" --project "$PWD" --source-project "/source/project" --candidate 'codex|~/.codex/state/sessions.db|conversations|project_path'
cargo run -p agent_sync_cli -- apply-native-remap --home "$HOME" --project "$PWD" --source-project "/source/project" --candidate 'codex|~/.codex/state/sessions.db|conversations|project_path' --backup-dir agent-sync-backups
cargo run -p agent_sync_cli -- rollback-native-remap-journal --input agent-sync-native-remap-journal.json
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- import-native-sessions --input agent-sync-sessions.asbundle --target-home "$HOME" --target-project "$PWD" --backup-dir agent-sync-backups --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl"
# Copy one selected conversation into a different target project identity than the global default:
AGENT_SYNC_BUNDLE_KEY="agent-sync-device-key.json" cargo run -p agent_sync_cli -- import-native-sessions --input agent-sync-sessions.asbundle --target-home "$HOME" --target-project "$PWD" --backup-dir agent-sync-backups --session "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl" --session-target "codex:~/.codex/sessions/YYYY/MM/DD/session.jsonl=/other/project"
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

Sensitive raw payloads are encrypted at the whole-bundle file boundary when a passphrase, Agent Sync device key file, OS keychain key, public recipient, or saved trusted recipient profile is provided. Use the desktop Bundle passphrase field, desktop private key picker, desktop OS keychain account controls, desktop public recipient list, desktop trusted recipient profiles, CLI `--bundle-passphrase` / `AGENT_SYNC_BUNDLE_PASSPHRASE`, CLI `--bundle-key` / `AGENT_SYNC_BUNDLE_KEY`, CLI `--bundle-keychain` / `AGENT_SYNC_BUNDLE_KEYCHAIN`, repeated CLI `--bundle-recipient AGE_OR_JSON`, or repeated CLI `--bundle-recipient-profile PROFILE_ID`. Importing, verifying, checking, or native-importing an encrypted bundle requires the matching passphrase, private key file, or OS keychain account. The private key file contains an age identity and should be stored/transferred like a secret; OS keychain entries keep that identity in the local platform credential store; encrypted keychain backup files also contain the private age identity and must be protected by storing the backup file separately from its backup passphrase (`--backup-passphrase` / `AGENT_SYNC_BUNDLE_BACKUP_PASSPHRASE`). Public recipient files and trusted recipient profiles contain only age recipients and are safe to share/use for export selection. Forgetting a trusted recipient profile only removes local trust metadata; it does not revoke a remote device key, so future exports should simply stop selecting that recipient. Exporting memory/MCP review payloads or raw session payloads without encryption still requires an explicit UI acknowledgement or CLI `--allow-unencrypted-sensitive-payloads`.

Automatically applicable today:

- safe text config payloads from a verified bundle, only through selected operations, with backup and checksum verification.
- explicitly selected `memory_knowledge` and `mcp_config` text payloads from a verified bundle, only after the review-acknowledgement gate, with backup and checksum verification.
- apply journal rollback for backup-backed changes and files created by the apply.
- local scan snapshot persistence into the local SQLite store, with UI history loading so a saved scan can be restored after restart.
- automatic apply-journal, native session import-journal, and native DB remap-journal persistence into the local SQLite store, with UI loading of stored rollback points.
- metadata-only session archive records into Agent Sync Studio SQLite storage, including per-session target-project metadata when the user overrides the global target.
- session archive history browsing in desktop from persisted `session_archive` records; these records are metadata/audit entries and do not replace the original raw-payload bundle.
- selected raw session payloads into an isolated staging directory with project-path rewrite journal; per-session target overrides are recorded in the stage journal.
- native session import readiness reports in desktop and CLI that are read-only and explicitly warn when the current adapter supports native-file import but not Codex/Claude DB/index project remap.
- read-only native session store discovery in desktop and CLI, including SQLite schema metadata only; this is evidence gathering for future DB/index remap and does not write native stores.
- read-only native compatibility reports in desktop and CLI that summarize, per detected agent, native session-file import/export support, SQLite exact-match project-remap evidence, opaque store candidates that still require fixtures, and explicit non-claims for broad DB/index remap.
- read-only native DB/index project-remap preview in desktop and CLI, using SQLite schema column names only. It does not read database rows or mutate native stores; recognized SQLite project-identity columns are marked as explicitly write-supported candidates while LevelDB/IndexedDB/unknown stores remain preview-only.
- read-only native DB/index project-remap dry-run in desktop and CLI, using exact `source_project` matching to count affected SQLite rows before any write. The desktop UI invalidates dry-run evidence when checkbox selection changes and requires a successful dry-run for the current selection before apply.
- explicitly selected SQLite DB project-remap candidates in desktop and CLI, with exact `source_project` matching, whole-DB backup, SQLite transaction, row-count journal, stopped-agent preflight by default, local journal persistence, and rollback by restoring the DB backup. The UI exposes candidate checkboxes after preview and a dry-run gate before apply; the CLI requires `--candidate 'AGENT|PORTABLE|TABLE|COLUMN'`. LevelDB/IndexedDB and unknown native stores remain preview-only.
- selected raw session payloads into native Codex/Claude file locations under a chosen target home, with strict `~/.codex/**` / `~/.claude/**` allowlisting, adapter capability gating, default Codex/Claude stopped-agent preflight, backup, optional project-path rewrite, per-session target-project override (`--session-target SESSION_ID=PROJECT_PATH` in CLI), checksum verification, and rollback from the native import journal. The UI exposes per-session target inputs plus an explicit manual stopped-agent override, and the CLI exposes `--skip-agent-stopped-check`; this does not rewrite opaque native Codex/Claude databases or secondary indexes, and the adapter capability model does not claim broad DB/index remap support until fixtures prove it.
- local trusted recipient profile management in desktop and CLI, with label/device/platform/note metadata, repeat selection for encrypted exports, and local forget semantics. This is not remote key revocation; it is a local allowlist for public recipients.
- stale trusted-recipient warnings, rotation playbooks, and local soft-revoke records in desktop and CLI. The plan tells you when to generate a new key on the remote device, save the new public recipient, verify a new encrypted bundle, and then mark the old profile revoked locally; it does not remotely revoke old private keys. `forget-bundle-recipient-profile` remains a hard local record delete for cleanup.
- public trusted-recipient inventory export/import in desktop and CLI. Inventory files contain public age recipients plus local trust metadata and a digest; they are suitable for cross-device setup, but they are not private-key backups and do not prove remote ownership.
- passphrase-encrypted OS keychain device-key backup and restore in desktop and CLI for machine rebuilds or lost-device recovery; the backup is encrypted but still contains the private age identity after decryption.

See `.omx/plans/agent-sync-studio-full-architecture-20260701.md` and `docs/agent-sync-studio-architecture.md` for the full implementation architecture.
