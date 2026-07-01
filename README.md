# agent-sync

`agent-sync` is a small read-only diagnostic tool for AI coding agent setup drift across machines.

It now supports a safe three-step loop:

1. `doctor`: scan common Codex, Claude Code, Cursor, Windsurf/Devin, Continue, Aider, and GitHub Copilot state surfaces.
2. `diff`: compare two snapshot manifests.
3. `plan`: generate a migration recipe that separates safe candidates, review-required items, and blocked-by-default surfaces.

## Quick start

```bash
node ./bin/agent-sync.js doctor
node ./bin/agent-sync.js doctor --json --output snapshot.json
node ./bin/agent-sync.js diff --from mac.json --to windows.json
node ./bin/agent-sync.js plan --from mac.json --to windows.json --target windows
```

Useful test overrides:

```bash
node ./bin/agent-sync.js doctor --home /tmp/fake-home --project /tmp/repo --json
```

## Safety

This prototype is inventory and recipe only. It does not migrate, copy, parse, or print file contents. It reports metadata and safety classifications only.

Blocked by default:

- credentials, tokens, cookies, OAuth state, `.env`-like files
- raw chat/session/transcript stores
- SQLite/LevelDB-like state
- binary/cache/plugin artifacts

See `docs/mvp0-design.md` for the product boundary.
