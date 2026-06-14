# Changelog

## 1.1.2 - 2026-06-14

### Usage statistics

- Added relay-friendly token trend reconstruction from local Codex `token_count` events.
- Added 24-hour hourly token buckets with total tokens and event counts.
- Added 7-day token buckets for relay-backed weekly trend rendering.
- Added `usage_mode` so the dashboard can switch between official rate-limit percentages and relay token usage totals.
- Updated relay mode cards to show `24h Tokens` and `7d Tokens` instead of misleading remaining quota percentages.
- Added hourly bar tooltips showing token totals and event counts.
- Preserved official route behavior for 5-hour and weekly rate-limit windows.

### Relay configuration

- Backfilled relay settings from the active `~/.codex/config.toml` provider.
- Supports active provider detection from `model_provider`.
- Supports provider table values and root-level fallback values for `base_url`, `experimental_bearer_token`, and `api_key`.
- Refreshing relay status now backfills the relay form without blocking usage data rendering.

### Startup reliability

- Fixed installed-app startup where data remained blank until the refresh button was clicked.
- Moved the first usage snapshot request ahead of non-critical startup tasks.
- Made Tauri window move listener registration non-blocking and guarded.
- Added an immediate initial refresh request after refresh button wiring, plus a queued fallback request.
- Kept app version, relay status, history status, context entries, window sizing, and autostart checks from blocking the first usage render.

### Packaging and identity

- Bumped app version to `1.1.2`.
- Changed app identifier to `xyz.moshushi.app.codextoolkit`.
- Set bundle publisher to `moshushi`.
- Updated copyright metadata to `Copyright (c) 2026 moshushi`.
- Replaced app icons with sharper multi-size Windows icon assets.

### Validation

- Added Rust tests for 24-hour token bucket construction and relay usage mode detection.
- Verified `cargo test` passes with 20 tests.
- Verified Windows bundle build with `npm run build`.
