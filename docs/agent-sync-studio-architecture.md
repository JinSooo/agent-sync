# Agent Sync Studio Architecture

Agent Sync Studio is the Tauri-first product line for cross-platform Codex and Claude Code state sync.

The product syncs agent working context, not raw folders:

```text
Scan -> Select -> Transform -> Preview -> Apply -> Verify/Rollback
```

## Runtime split

- React/Vite frontend: control plane only, including native Tauri dialog pickers for bundle/home/project/backup paths.
- Tauri commands: narrow IPC boundary.
- Rust crates: all filesystem, bundle, transform, apply, backup, and storage work.

The frontend must not directly read or write `~/.codex`, `~/.claude`, raw sessions, or app databases.

## Crates

| Crate | Responsibility |
| --- | --- |
| `agent_sync_core` | Domain models, adapter capabilities, safety classes, path redaction, classification |
| `agent_sync_scan` | Codex/Claude surface scan and metadata-only session discovery |
| `agent_sync_transform` | Snapshot diff, project mapping, and transform-plan generation |
| `agent_sync_bundle` | `.asbundle` source snapshot, manifest, safe/review payload selection, redaction, and checksum handling |
| `agent_sync_apply` | Preflight, operation journal, safe/review payload apply, session native-file import, rollback with backups |
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
- explicitly selected `memory_knowledge` and `mcp_config` text payloads from a verified bundle, only after UI/CLI acknowledgement of the review gate, with backup and checksum verification.
- rollback of apply journals by restoring backup files or removing files created by the apply.
- metadata-only session archive records into Agent Sync Studio SQLite storage.
- explicitly selected raw session payloads into isolated staging, or into native Codex/Claude file locations under a chosen target home. Native file import is allowlisted to `~/.codex/**` and `~/.claude/**`, defaults to a Codex/Claude stopped-agent process preflight, backs up existing files, optionally rewrites source project paths to the target project path, verifies written checksums, and can roll back from the native import journal. The user can explicitly bypass the stopped-agent check for a manual override; the journal records blockers when the preflight fails.

## Current implementation status

Implemented:

- Tauri 2 desktop shell with React/Vite UI.
- Rust scan/diff/transform/preflight/journal commands with adapter capabilities included in snapshots.
- Real `.asbundle` container with source snapshot, payload checksums, safe config payloads, explicitly selected memory/MCP review payloads, metadata-only session archive entries, explicitly selected raw session payloads, and secret redactions. Plain JSON remains supported for non-sensitive/backward-compatible bundles; passphrase-protected exports are written as age/scrypt ciphertext, and device-key/public-recipient/trusted-recipient-profile exports are written as age/x25519 ciphertext.
- Sensitive memory/MCP and raw session payload export uses whole-bundle age encryption when a passphrase, Agent Sync device key file, OS keychain device key, one/more public recipients, or one/more trusted recipient profiles are supplied. Export without encryption remains available only behind explicit UI/CLI acknowledgement for trusted transport or local debugging.
- Local SQLite store for snapshots, apply journals, native session import journals, and session archive records as JSON records; the desktop UI can refresh and reload stored scan snapshots after restart.
- Safe config plus acknowledged memory/MCP review apply path with visual operation selection, backup, operation journal, checksum verification, automatic journal persistence, history loading, and journal rollback.
- Session Library flow: choose export-capable local sessions for raw payload export; choose remote Codex/Claude session archives, bind them to the target home/project path globally or with per-session target-project overrides, import metadata-only records into local Agent Sync Studio SQLite storage, browse persisted session archive history after restart, check native import readiness without writing files, discover native DB/index store candidates with schema-only SQLite inspection, generate a read-only per-agent native compatibility evidence matrix, preview likely SQLite project-remap columns with no row reads, dry-run selected SQLite columns with read-only exact-match row counts, explicitly select SQLite columns for exact source-project remap with DB backup/transaction/rollback, stage selected raw payloads into an isolated native-import directory, or write selected payloads to native Codex/Claude session-file locations with project-path rewrite evidence, a stopped-agent preflight, native import-journal persistence/history loading, and native-file rollback. Session native actions are gated by adapter capabilities and warn when broad DB/index remap is not supported.
- Rust CLI: `scan`, `bundle-manifest`, `generate-bundle-key`, `generate-bundle-keychain`, `export-bundle-recipient`, `export-bundle-keychain-recipient`, `forget-bundle-keychain`, `export-bundle-keychain-backup`, `restore-bundle-keychain-backup`, `save-bundle-recipient-profile`, `list-bundle-recipient-profiles`, `bundle-recipient-rotation-plan`, `export-bundle-recipient-inventory`, `import-bundle-recipient-inventory`, `revoke-bundle-recipient-profile`, `forget-bundle-recipient-profile`, `export-bundle`, `verify-bundle`, `check-native-sessions`, `discover-native-stores`, `native-compatibility-report`, `preview-native-remap`, `dry-run-native-remap`, `apply-native-remap`, `import-native-sessions`, `rollback-journal`, `rollback-native-session-journal`, `rollback-native-remap-journal`, `self-plan`; keychain commands store/rotate/remove only the local age identity in the platform credential store, export shareable public recipients, and export/restore passphrase-encrypted private identity backups for recovery; trusted recipient profile commands store public recipients plus label/device/platform/note metadata in the local SQLite store for repeat export selection, stale-recipient rotation planning, public inventory exchange, soft revoke, and hard local deletion; `check-native-sessions`, `discover-native-stores`, `native-compatibility-report`, `preview-native-remap`, and `dry-run-native-remap` are read-only, while `apply-native-remap` and `import-native-sessions` default to the stopped-agent check with `--skip-agent-stopped-check` for explicit override. `import-native-sessions` also accepts repeated `--session-target SESSION_ID=PROJECT_PATH` values so selected conversations can be copied into different target project identities in one import run.

Implemented in the current product loop:

- Native file picker flow for bundle import/export, target home, target project, staging, backup directory, file-based bundle private keys, OS keychain account keys, encrypted keychain backup/restore, public recipient export, public recipient inventory import/export, trusted recipient profile selection, stale-recipient rotation warnings, and local recipient soft revoke.
- Snapshot history UI for restoring saved local scans from the selected archive store without rescanning.
- Session archive history UI for reviewing persisted `session_archive` metadata records without implying raw payload reconstruction from metadata alone.
- Searchable/filterable selection lists for local session payload export, review-required memory/MCP payload export, and imported remote session archive/native-file actions, with filtered-result bulk selection and per-session target-project override fields.
- Project-mapping UI with git-remote exact match, basename fallback, confidence, manual-review warning, and capability-visible DB/index remap limitations.
- Native compatibility evidence UI that explains which parts are supported today, which SQLite remap candidates are exact-match/write-gated, and which opaque DB/index surfaces remain fixture-gated rather than claimed.
- GitHub Actions CI matrix for macOS, Windows, and Linux running Rust tests, legacy Node checks, desktop web build, and Tauri no-bundle build.

Still to deepen:
- Codex native session import/remap into opaque Codex-owned secondary indexes/databases beyond explicit SQLite exact-match candidate updates; current SQLite remap has preview, read-only dry-run counts, backup/transaction/rollback, but adapter-specific index semantics still need fixtures before broad capability claims.
- Claude Code native session import/remap into opaque Claude-owned secondary indexes/databases beyond explicit SQLite exact-match candidate updates; current SQLite remap has preview, read-only dry-run counts, backup/transaction/rollback, but adapter-specific index semantics still need fixtures before broad capability claims.
- Key lifecycle policy beyond local generate/rotate/export/forget/encrypted backup/restore/trusted recipient profiles, public inventory exchange, and local soft revoke: remote recipient revocation cannot be enforced from this local tool, so deeper work is focused on cryptographically signed recipient ownership proofs across devices.
