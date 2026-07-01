# AI Coding Agent Cross-Device Sync — Market & User-Need Research

Date: 2026-07-01
Scope: research only. No implementation, no local secret/session migration, no production data movement.

## Executive conclusion

There is a real and visible gap, but the safest first product is **not full session auto-sync**. The recommended next step is **Option B: an audit/diff + migration recipe generator for AI coding agent state**.

Why:

1. Direct tools expose many local customization surfaces (Codex `~/.codex`, Claude `~/.claude`, Cursor `.cursor`, Windsurf `.windsurf`/`.devin`, Continue `.continue`, Aider `.aider.*`), but none clearly covers the combined need: **config + plugins/MCP/skills + memory + conversation/session continuity + cross-platform conflict handling**.
2. Adjacent tools solve slices: VS Code/JetBrains sync editor settings; chezmoi solves dotfiles; memories.sh/Memorix solve memory/rules; rulesync solves rule-file generation; file sync can copy state folders. The integration glue, safety model, and semantic diff are still left to the user.
3. Full raw-session / secret / binary sync is high-risk and vendor-format-fragile. The first wedge should be read-only inventory, drift detection, and generated apply recipes with explicit excludes.

Confidence: **0.78**. Strong evidence exists for config/rules/MCP fragmentation and Cursor cross-device pain; weaker evidence exists for exact private session formats in newer Codex/Claude/Cursor builds because vendors do not document all local state internals.

## Decision-threshold result

- Existing stack threshold: **not met**. No single direct product or native integrated stack covers 6+ core dimensions without user-managed glue.
- Build/prototype threshold: **met**. No assessed product clearly covers both agent-specific memory/knowledge and conversation/session continuity across macOS + Windows, while config/plugin drift is also only partially covered.
- Default build route: **Option B**. Start with audit/diff + migration recipes, not Option A full auto-sync.
- Full auto-sync route: **roadmap only** until controls exist for secrets, raw transcripts, plugin binaries, DB corruption/conflicts, and path remapping.

## User jobs and pain signals

| Persona / use case | Pain | Evidence signal | Product implication |
|---|---|---|---|
| Individual AI power user switching Mac/Windows | Wants Codex/Claude/Cursor state, MCPs, skills, memories, and preferences to feel consistent | Cursor forum users explicitly ask to sync AI chat/workspace sessions across Mac devices; Cursor staff says chat history is locally stored and not cloud-synced, with Dropbox/iCloud symlink workaround and identical path caveat | Start with local inventory + safe recipes; session sync needs path remapping and backups |
| Multi-tool user using Codex + Claude Code + Cursor | Same instructions/skills/MCPs duplicated in different formats | rulesync, memories.sh, Memorix, OpenAI community migration discussions, cursor2claude-style tools all exist because users want one source of truth | Adapter layer and semantic transform have market pull |
| Team/onboarding | Wants deterministic setup across machines and team members | GitHub Copilot, VS Code, JetBrains, Claude managed settings show mature centralized config patterns | Team mode can later add policy/managed profiles |
| Security-conscious developer/org | Needs not to leak tokens/raw transcripts/plugin binaries | Claude managed-policy docs, VS Code keychain notes, chezmoi secret/encryption support all show secret/config separation is normal practice | Default must be read-only, diff-first, no secrets copied unless explicit encrypted backend |

## Direct / near-direct tool matrix

Legend: **Fact** = official/publicly documented; **Inference** = plausible from public paths/community but not an official capability statement; **Gap** = no public/native support found in this research pass.

| Tool | Config/settings | Plugins / MCP / skills | Memory / knowledge | Sessions / chat continuity | Secrets | Cross-platform / conflicts | Assessment |
|---|---|---|---|---|---|---|---|
| OpenAI Codex CLI | **Fact:** user config in `~/.codex/config.toml`, project `.codex/config.toml`, trusted project layers, profile/system precedence. | **Fact:** MCP in config; CLI and IDE share MCP config; skills are first-class in CLI/IDE/app; AGENTS.md guidance is global/project-layered. | **Fact:** Codex customization model includes memories as a layer, but public docs do not present cross-device memory sync. | **Gap:** no native cross-device session/chat sync surfaced in docs. | **Fact:** MCP env vars and approval/sandbox settings exist; secret sync is not a built-in cross-device feature. | **Fact:** macOS/Windows/Linux support; Windows native/WSL paths differ. Conflict sync not documented. | Strong config surface, weak continuity surface. |
| Claude Code | **Fact:** settings in `~/.claude/settings.json`; VS Code extension shares Claude Code settings for allowed commands, env vars, hooks, MCP. | **Fact:** skills, plugins, hooks, MCP servers, subagents; enterprise settings can restrict sources. | **Fact:** CLAUDE.md/CLAUDE.local.md discovery plus auto memory under `~/.claude/projects/<project>/memory/`; `/memory` edits/browses. | **Inference/Gap:** memory persists, but raw session handoff across machines is not documented as native sync. | **Fact:** enterprise managed settings can restrict hooks/MCP/skills/plugins. | **Fact:** native installs for macOS/Linux/WSL and Windows PowerShell/CMD; no public conflict model for local memory/session copy. | Strong customization; local memory helps, but not cross-device continuity by default. |
| Cursor | **Fact:** official docs cover Rules and MCP; third-party memory integrations target `.cursor/rules` and `.cursor/mcp.json`. | **Fact/Community:** global/project MCP config and rules are common; MCP tool enablement persistence has active feature requests. | **Community:** memories/rules are used; third-party MCP memory tools target Cursor. | **Community + staff:** Cursor forum says chat history is stored locally and not cloud-synced; workaround copies/symlinks `workspaceStorage`; same paths matter. | **Community:** MCP env/auth config exists; secret policy details less clear from accessible docs. | **Community:** settings sync and chat sync are repeated feature requests; path hashing creates portability constraints. | Strong pain signal; native sync gap visible. |
| Windsurf / Devin Desktop | **Fact:** local agent harness, MCP, memories/rules; memories stored locally under `~/.codeium/windsurf/memories/`; rules in `.devin/rules` or legacy `.windsurf/rules`; AGENTS.md discovered as rules. | **Fact:** MCP config in `~/.codeium/mcp_config.json`; supports stdio/HTTP/SSE/OAuth. | **Fact:** auto memories are workspace-associated and local-machine only; docs recommend rules/AGENTS.md for durable/team-shared knowledge. | **Gap:** no native raw session sync found. | **Fact:** account/enterprise features exist; full secret sync not documented as agent-state sync. | **Fact:** macOS/Windows/Linux. Rules are shareable; memories not shared by default. | Strong evidence that local memories are intentionally not durable/team-shared. |
| GitHub Copilot / Copilot cloud agent | **Fact:** repo-wide/path-specific instructions, organization instructions, AGENTS.md/CLAUDE.md support in VS Code; MCP works across Copilot surfaces; cloud agent sessions run on GitHub. | **Fact:** custom agents are `.agent.md` profiles with tools and MCP; repository MCP config can use `COPILOT_MCP_` secrets. | **Partial:** repository/org instructions persist; not a universal personal memory layer. | **Fact:** Copilot cloud agent carries GitHub chat context into a cloud agent session, but this is GitHub-hosted, not local Codex/Claude/Cursor state sync. | **Fact:** repository agents secrets/variables. | **Strong within GitHub ecosystem**, less useful for local multi-agent continuity. | Best native cloud continuity, but ecosystem-scoped. |
| Continue | **Fact:** `config.yaml`; configs include models, rules, prompts, docs, MCP servers; all config stored locally; `.continue/rules`, `.continue/mcpServers`. | **Fact:** MCP server blocks, can copy JSON MCP configs from Claude/Cursor/Cline into `.continue/mcpServers`. | **Partial:** rules/context config, not native cross-device session memory. | **Gap:** no native cross-device session sync found. | **Fact:** secrets via `${{ secrets.NAME }}` / env patterns. | **Fact:** local files can be versioned; conflict handling outsourced to git/user. | Good declarative config model; no session continuity. |
| Aider | **Fact:** home/repo/current `.aider.conf.yml` precedence; chat/input/LLM history files configurable; restore chat history flag. | **Partial:** CLI options/config; less plugin-like than Codex/Claude/Cursor. | **Partial:** repo map and history files, not semantic memory. | **Fact:** chat history file can be restored; cross-device requires file sync/git/user handling. | **Fact:** API keys via env/.env/YAML caveats. | **Partial:** file-based and git-friendly, but no native multi-device sync. | Simpler file-state model; easier to support as adapter. |

## Adjacent solution categories

| Category | What it solves | What it does not solve for AI-agent sync |
|---|---|---|
| VS Code Settings Sync | Syncs settings, keybindings, snippets, tasks, UI state, extensions, profiles; has merge/replace/conflict UI and backups. | Does not sync extensions in remote windows; does not understand Codex/Claude/Cursor memory/session semantics. |
| JetBrains Backup and Sync | Syncs IDE settings and plugin install/enablement across JetBrains IDEs. | IDE-only; no AI-agent memory/session semantics. |
| Dotfile managers: chezmoi | Cross-machine dotfiles, templates for machine differences, password manager support, encryption, scripts, Windows/macOS/Linux. | Great substrate, but user must know what to include/exclude and how to transform vendor formats. |
| File sync: Dropbox/iCloud/Syncthing-like | Can copy local folders, including app support directories. | Unsafe for live SQLite/LevelDB/session stores; path hashes and OS paths break; conflict semantics are blind. |
| Memory/rules tools: memories.sh, Memorix, Memory Store, ContextStream | Cross-agent or MCP-based memory/rules layers; some can ingest/apply config files for `.cursor`, `.codex`, `.claude`, `.windsurf`. | Usually memory/rules-focused; raw sessions, plugin binaries, secret governance, and conflict recovery remain incomplete. |
| Rule converters: rulesync, cursor2claude, Agent Rules Sync | Generate/copy rule files across tools. | Mostly instruction/rule files; not full config, sessions, memory DBs, secrets, or plugin state. |
| Backup tools: agent_settings_backup_script | Git-versioned backup of agent configuration folders. | Backup/restore, not semantic safe migration; may still capture sensitive content unless classified. |
| Cloud dev environments / GitHub cloud agent | Avoids device drift by moving execution to one cloud environment. | Does not solve local terminal/IDE agent state parity for users who must use Mac + Windows locally. |
| Secret managers | Safely store tokens/API keys and inject them at runtime. | Do not know which agent configs need transformation; not a session/memory sync layer. |

## Full-migration risk table

| Surface | Risk | Severity | Safe default | Later control if full sync is desired |
|---|---:|---:|---|---|
| API keys, OAuth tokens, local auth cookies | Secret leakage, account takeover | Critical | Detect and redact; never copy by default | Secret-manager references only; per-machine re-auth; encrypted export with explicit allowlist |
| Raw transcripts / chat history | Proprietary code and private prompts leak; vendor ToS/data portability unknowns | High | Inventory/count only; optional summaries instead of raw copy | Encrypted snapshots; user review; per-project allowlist; retention policy |
| Local DBs: SQLite/LevelDB/workspaceStorage | Corruption, concurrent writes, path-hash mismatch | High | Do not live-sync; back up before migration | Offline export/import adapters; schema/version detection; rollback |
| Plugins/skills/hooks binaries/scripts | Supply-chain and arbitrary command execution | High | Hash and list; install recipes instead of copying binaries | Signature/registry verification; script preview; policy engine |
| MCP configs | Tool overexposure; env var leakage; dangerous tools enabled | High | Diff config excluding secrets; disabled by default on target unless user approves | Tool allow/deny policy; per-tool safety labels; env var placeholders |
| Instruction/rule files | Conflicts and divergent behavior | Medium | Diff/merge text with source-of-truth manifest | Transform adapters with comments and conflict markers |
| OS-specific paths/shells | Broken configs on Windows vs macOS | Medium-high | Detect absolute paths and shell commands | Path variables, templates, per-machine overlays |
| Vendor/private formats | Breakage after product update | Medium-high | Version-stamp and mark unknowns | Adapter tests against fixtures; compatibility matrix |

## Product opportunity

The market is not missing another generic sync engine. It is missing a **semantic inventory and migration layer for AI coding agents**.

Useful positioning:

> “Show me why my AI coding setup differs between machines, generate a safe migration plan, and apply only the parts I approve.”

Differentiators versus existing tools:

1. Knows AI-agent-specific surfaces: Codex/Claude/Cursor/Windsurf/Continue/Aider/Copilot instructions, skills, agents, hooks, MCP, memory files, sessions.
2. Classifies state by safety: public config, project rules, executable hooks, secrets, raw transcripts, binary/plugin state.
3. Produces recipes rather than blind sync: chezmoi templates, git commits, install commands, MCP add commands, secret-manager placeholders, path remap hints.
4. Has cross-platform awareness: Windows PowerShell/WSL/macOS paths, app support directories, case sensitivity, CRLF, keychains.
5. Gives rollback: backups, manifests, dry-run, diff, restore.

## Recommended first tool shape

### MVP 0: read-only `agent-sync doctor`

- Detect installed agents and state roots:
  - `~/.codex`, project `.codex`, `AGENTS.md`
  - `~/.claude`, project `.claude`, `CLAUDE.md`, `.mcp.json`
  - `~/.cursor`, `.cursor/rules`, `.cursor/mcp.json`, workspaceStorage metadata
  - `~/.codeium/windsurf`, `.devin/rules`, `.windsurf/rules`, `~/.codeium/mcp_config.json`
  - `~/.continue`, `.continue/rules`, `.continue/mcpServers`
  - `.aider.conf.yml`, `.aider.chat.history.md`, `.aider.input.history`
  - `.github/copilot-instructions.md`, `.github/instructions`, `AGENTS.md`, `*.agent.md`
- Output a classified manifest: safe config, executable, secret-bearing, raw session, binary/cache, unknown.
- Redact likely secrets.
- No copying.

### MVP 1: `agent-sync diff --from snapshot-a --to snapshot-b`

- Compare two machines/snapshots.
- Show missing skills/MCP/rules/hooks/settings.
- Flag OS-specific path problems.
- Mark session DBs as “manual/offline migration only.”

### MVP 2: `agent-sync plan --target windows|macos`

- Generate a migration recipe:
  - create missing directories/files,
  - copy or transform safe rule files,
  - suggest `codex mcp add` / Claude/Cursor MCP config edits,
  - generate chezmoi-compatible templates,
  - produce secret-manager placeholder checklist.

### Deferred

- Encrypted cloud sync.
- Raw session export/import adapters.
- Continuous background sync.
- Team/org policy packs.

## Go / no-go recommendation

**Go, but only for audit/diff + recipes.**

Do not build full auto-sync first. The first prototype should prove that the tool can discover and explain drift between Mac and Windows setups without copying sensitive state. If that feels valuable on the user’s real Codex/Claude/Cursor setup, then add safe apply for instructions/MCP/skills and only much later consider encrypted session/memory migration.

## Source list

### Direct tool docs

- OpenAI Codex config basics: https://developers.openai.com/codex/config-basic
- OpenAI Codex MCP: https://developers.openai.com/codex/mcp
- OpenAI Codex AGENTS.md: https://developers.openai.com/codex/guides/agents-md
- OpenAI Codex skills: https://developers.openai.com/codex/skills
- OpenAI Codex customization layers: https://developers.openai.com/codex/concepts/customization
- OpenAI Codex CLI: https://developers.openai.com/codex/cli
- Claude Code settings: https://code.claude.com/docs/en/settings
- Claude Code memory: https://code.claude.com/docs/en/memory
- Claude Code hooks: https://code.claude.com/docs/en/hooks
- Claude Code hooks guide: https://code.claude.com/docs/en/hooks-guide
- Claude Code skills: https://code.claude.com/docs/en/skills
- Claude Code IDE integrations: https://code.claude.com/docs/en/ide-integrations
- Cursor rules docs: https://cursor.com/docs/rules
- Cursor MCP docs: https://cursor.com/docs/mcp
- Cursor cross-device forum: https://forum.cursor.com/t/is-cross-device-sync-possible/147030
- Cursor settings sync forum: https://forum.cursor.com/t/sync-of-keybindings-and-settings/31?page=6
- Cursor MCP tool persistence request: https://forum.cursor.com/t/add-the-ability-to-pre-configure-which-mcp-tools-are-enabled-disabled-via-the-mcp-json-configuration-file-eliminating-the-need-to-manually-enable-tools-in-the-ui-after-every-cursor-restart/148139
- Windsurf/Devin memories and rules: https://docs.devin.ai/desktop/cascade/memories
- Windsurf/Devin getting started: https://docs.devin.ai/desktop/getting-started
- Windsurf full docs text (MCP/rules/AGENTS): https://docs.windsurf.com/llms-full.txt
- GitHub Copilot repository instructions: https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/add-custom-instructions/add-repository-instructions
- GitHub Copilot MCP: https://docs.github.com/en/copilot/concepts/context/mcp
- GitHub Copilot repository MCP config: https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/configure-mcp-servers
- GitHub Copilot custom agents: https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/customize-cloud-agent/create-custom-agents
- GitHub Copilot cloud agent: https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent
- Continue config reference: https://docs.continue.dev/reference
- Continue configuration overview: https://docs.continue.dev/guides/understanding-configs
- Continue rules: https://docs.continue.dev/customize/deep-dives/rules
- Continue MCP: https://docs.continue.dev/customize/deep-dives/mcp
- Aider options reference: https://aider.chat/docs/config/options.html
- Aider YAML config: https://aider.chat/docs/config/aider_conf.html

### Adjacent tools / competitors

- VS Code Settings Sync: https://code.visualstudio.com/docs/configure/settings-sync
- JetBrains Backup and Sync: https://www.jetbrains.com/help/idea/sharing-your-ide-settings.html
- chezmoi: https://www.chezmoi.io/
- chezmoi machine differences: https://www.chezmoi.io/user-guide/manage-machine-to-machine-differences/
- memories.sh Cursor integration: https://memories.sh/docs/integrations/cursor
- memories.sh Windsurf integration: https://memories.sh/docs/integrations/windsurf
- Memorix MCP server: https://mcpservers.org/servers/avids2/memorix
- rulesync: https://github.com/dyoshikawa/rulesync
- Agent Settings Backup: https://github.com/Dicklesworthstone/agent_settings_backup_script
- OpenAI community discussion on Codex/Claude config sync: https://community.openai.com/t/sync-codex-and-claude-code-configs-skills-agents-mcp-permissions/1380517
