# Codex Toolkit

<p align="center">
  <strong>A local-first desktop toolkit for Codex usage monitoring, relay provider management, and history sync.</strong>
</p>

<p align="center">
  <a href="./README.md">中文</a> | English
</p>

<p align="center">
  <img src="./docs/images/relay-management.png" alt="Codex Toolkit relay management panel" width="420" />
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> |
  <a href="#features">Features</a> |
  <a href="#relay-management">Relay Management</a> |
  <a href="#history-sync">History Sync</a> |
  <a href="#development">Development</a>
</p>

Codex Toolkit reads local Codex session logs and turns token activity into a compact desktop dashboard, including token totals grouped by provider. It also manages Codex relay/API configuration, so you can switch between the official route and a relay endpoint without hand-editing `~/.codex/config.toml`. When needed, it can sync historical session provider metadata to your current route.

Currently tested mainly on Windows. macOS compatibility has not been fully verified yet, and issue reports are welcome.

## Screenshots

<p>
  <img src="./docs/images/relay-management.png" alt="Codex Toolkit relay configuration panel" width="31%" />
  <img src="./docs/images/history-sync.png" alt="Codex Toolkit history sync panel with provider records" width="31%" />
  <img src="./docs/images/about-panel.png" alt="Codex Toolkit about panel with version and GitHub link" width="31%" />
</p>

<p>
  <img src="./docs/images/app-screenshot.png" alt="Codex Toolkit dark dashboard" width="31%" />
  <img src="./docs/images/dashboard-view.png" alt="Codex Toolkit light dashboard" width="31%" />
  <img src="./docs/images/settings-dark-view.png" alt="Codex Toolkit dark settings" width="31%" />
</p>

## Quick Start

```bash
npm install
npm run dev
```

Build desktop installers:

```bash
npm run build
```

Build outputs are generated under:

- `src-tauri/target/release/bundle/msi`
- `src-tauri/target/release/bundle/nsis`
- `src-tauri/target/release/bundle/dmg`
- `src-tauri/target/release/bundle/macos`

## Features

| Area | What you get |
| --- | --- |
| Token dashboard | Current session totals, last response usage, trend views, context window size, provider token totals |
| Rate-limit view | 5-hour and weekly usage windows based on local Codex session logs |
| Relay management | Provider ID, API Base URL, API Key, apply, restore official, apply and restart |
| History sync | Review provider history counts and sync session files plus local SQLite history to the current provider |
| Desktop behavior | Tray minimize/restore, login autostart, edge snapping, privacy mode |
| UI | English/Chinese menu switching and day/night theme toggle |

## Relay Management

Codex Toolkit stores relay settings locally, then writes them to Codex only when you apply the configuration.

With the default Provider ID `moapi`, the generated Codex config looks like:

```toml
model_provider = "moapi"

[model_providers.moapi]
name = "moapi"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://your-relay.example.com/v1"
experimental_bearer_token = "sk-..."
```

The Provider ID is editable. If you set it to `myrelay`, Codex Toolkit writes:

```toml
model_provider = "myrelay"

[model_providers.myrelay]
name = "myrelay"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://your-relay.example.com/v1"
experimental_bearer_token = "sk-..."
```

Before writing, the existing Codex config is backed up as:

```text
config.toml.codexviewer-backup-YYYYMMDD-HHMMSS
```

Restore official removes the active toolkit-managed provider, the default `moapi` provider, and the legacy `CodexViewerRelay` provider if present.

## History Sync

The history sync panel reads the current Codex provider and counts provider records from:

- rollout session files under `~/.codex/sessions` and `~/.codex/archived_sessions`
- thread rows in `~/.codex/state_5.sqlite`
- backups created by the toolkit history sync flow

Clicking "Sync history" updates historical sessions that do not match the current provider and updates local SQLite thread rows. Before syncing, Codex Toolkit backs up the original session metadata, `state_5.sqlite`, Codex config, and global state files under:

```text
~/.codex/backups_state/toolkit-history-sync/YYYYMMDDTHHMMSS.sssZ
```

If older histories contain `encrypted_content` from another provider, the panel shows a warning: sync can restore list visibility, but continuing those sessions may still be affected by provider-specific encrypted content.

## How Data Loading Works

The app:

1. Resolves the Codex sessions directory
2. Recursively scans `.jsonl` files
3. Reads `model_provider` from the first-line `session_meta`
4. Extracts `token_count` events
5. Reads the current toolkit-managed Codex provider status
6. Builds the latest token snapshot, trend series, and provider token summaries
7. Labels usage as official or relay-backed
8. Renders the result in the desktop UI

Default log location:

```text
~/.codex/sessions
```

You can override the log directory from the settings panel.

## Why It Exists

OpenAI's public API usage endpoints are designed for organization billing and API usage, not local Codex desktop session analytics. Relay providers also differ in how they expose usage data.

Codex Toolkit focuses on local Codex usage reconstruction by reading session log files already available on your machine, then shows history distribution from each session's provider metadata and your current Codex route.

## Development

Requirements:

- Node.js 20+
- Rust toolchain with `cargo`
- Windows: Microsoft Visual Studio C++ Build Tools
- macOS: Xcode Command Line Tools

Useful checks:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Frontend syntax-only check on Windows:

```powershell
$tmp = Join-Path $env:TEMP 'codex-toolkit-renderer-check.mjs'
Copy-Item src\renderer.js $tmp -Force
node --check $tmp
```

## Release Automation

This repository includes two GitHub Actions workflows:

- `CI`: runs tests and verifies the app builds on pushes and pull requests
- `Release`: builds Windows and macOS bundles and uploads them to GitHub Releases when you push a version tag like `v1.0.0`

Example release flow:

```bash
git tag v1.0.0
git push origin main
git push origin v1.0.0
```

## Platform Notes

- Windows release artifacts are generated as `.msi` and `setup.exe`
- The Windows release executable uses the GUI subsystem, so it does not open a console window
- macOS release artifacts are generated as `.dmg` and `.app`
- macOS signing and notarization are not configured yet, so Gatekeeper warnings may still appear on first launch

## Privacy

Codex Toolkit reads local session logs and the local Codex SQLite state database from your machine. It does not call the public OpenAI organization usage API to populate the dashboard.

API keys are stored locally in the toolkit settings file and written to Codex config only when you apply relay settings. History sync rewrites local session metadata and local SQLite provider markers, with automatic backups before changes are applied. Avoid sharing screenshots that reveal full local paths or sensitive relay details; use the built-in privacy toggle when needed.

## Contributing

Please read [CONTRIBUTING.md](./CONTRIBUTING.md) before opening a pull request.

## License

[MIT](./LICENSE)
