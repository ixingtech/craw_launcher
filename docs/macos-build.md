# macOS build notes

## OpenClaw paths

- Default OpenClaw state directory: `~/.openclaw`
- Preferred app bundle path: `/Applications/OpenClaw.app/Contents/MacOS/OpenClaw`
- Common CLI paths:
  - `/opt/homebrew/bin/openclaw`
  - `/usr/local/bin/openclaw`
  - `~/.local/bin/openclaw`

The launcher now accepts either a macOS `.app` bundle or a direct `openclaw` binary path and normalizes `.app` selections to the real executable inside `Contents/MacOS`.

## Build commands

Run macOS bundles on a macOS machine:

```bash
pnpm build:mac
pnpm build:mac-app
pnpm build:mac-dmg
```

Expected artifacts are emitted by Tauri under `src-tauri/target/release/bundle/`.

## Distribution notes

- Local testing can use unsigned `.app` / `.dmg` bundles.
- Public distribution should add Apple code signing and notarization, otherwise Gatekeeper may block installs.
- Finder-launched GUI apps may not inherit shell `PATH`; standard install paths are preferred for `openclaw`.
