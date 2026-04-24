# Codex Token Viewer

A lightweight Tauri desktop app for viewing local Codex token usage, session trends, and rate limit windows in real time.

![Codex Token Viewer screenshot](./docs/images/app-screenshot.png)

## What It Does

Codex Token Viewer reads local Codex session logs from `~/.codex/sessions/*.jsonl` and turns them into a small desktop dashboard.

It shows:

- Current session token totals
- Last response token usage
- 5-hour and weekly rate-limit windows
- 24-hour and 7-day trend views
- Context window size
- Tray minimize / restore behavior
- Login autostart and edge snapping
- Privacy mode for hiding local log paths

## Why It Exists

OpenAI's public API usage endpoints are designed for organization billing and API usage, not local Codex desktop session analytics.

This app focuses on local Codex usage reconstruction by reading session log files already available on your machine.

## Tech Stack

- [Tauri v2](https://tauri.app/)
- Rust
- Vanilla HTML / CSS / JavaScript

## Requirements

- Node.js 20+
- Rust toolchain with `cargo`
- Windows: Microsoft Visual Studio C++ Build Tools
- macOS: Xcode Command Line Tools

## Development

```bash
npm install
npm run dev
```

## Build

```bash
npm run build
```

Built desktop artifacts are generated under:

- `src-tauri/target/release/bundle/msi`
- `src-tauri/target/release/bundle/nsis`

## How Data Loading Works

The app:

1. Resolves the Codex sessions directory
2. Recursively scans `.jsonl` files
3. Extracts `token_count` events
4. Builds the latest token snapshot and trend series
5. Renders the result in the desktop UI

Default log location:

```text
~/.codex/sessions
```

You can also override the log directory from the settings panel in the app.

## Privacy Note

This project reads local session logs from your machine. It does not call the public OpenAI organization usage API to populate the dashboard.

If you plan to share screenshots, use the built-in privacy toggle to hide local file paths.

## Open Source Status

This project is intended to be a small open source utility for Codex users. Issues, fixes, and UX improvements are welcome.

## Contributing

Please read [CONTRIBUTING.md](./CONTRIBUTING.md) before opening a pull request.

## License

[MIT](./LICENSE)
