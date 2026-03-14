# openclaw-launcher

`openclaw-launcher` is a desktop launcher for managing and running local OpenClaw profiles. It provides a GUI for selecting profile directories, importing and exporting `.claw` packages, viewing profile contents, chatting with the current profile, and maintaining the local gateway/runtime state used by OpenClaw.

Chinese README:

- [README.zh-CN.md](README.zh-CN.md)

Chinese product name:

- `xiaolongxia` / `小龙虾启动器`

English build name:

- `Craw Launcher`

## Why this project exists

OpenClaw itself is usually operated from its own local directory structure and runtime environment. Once users start keeping multiple profiles, moving profiles between machines, or checking package integrity, direct manual management becomes error-prone.

This launcher focuses on those operational tasks:

- detect the OpenClaw executable and default data directory
- manage the default local profile and imported profiles in one place
- launch a selected profile with the correct runtime wiring
- inspect conversations, notifications, docs, skills, jobs, memory, and account-related files
- import/export `.claw` packages with manifest and SHA-256 verification
- provide a lightweight chat surface and quick links to the control page / command line
- support localized Windows builds and standalone CLI builds

## Feature overview

### Profile management

- Recognize the default local profile and imported profiles
- Import `.claw` packages into launcher-managed storage
- Rename or delete imported profiles
- Export profiles with configurable inclusion of memory/history/account data
- Track recent launch history

### Launch and runtime management

- Auto-detect OpenClaw startup entry and system data directory
- Start and stop the runtime for the current profile
- Poll health endpoints and reflect runtime status in the UI
- Open the control web page or OpenClaw command line for the running profile
- Optionally stop all launcher-started profiles when the launcher exits

### Content inspection

- Browse profile documentation such as `README.md` and workspace files
- Inspect skills under `workspace/skills/*/SKILL.md`
- View cron jobs from `cron/jobs.json`
- Inspect conversations and notifications
- Preview profile inventory without manually opening directories

### Package safety

- `.claw` packages are ZIP-based archives with a root `manifest.json`
- Import verifies required files, file list consistency, size, and SHA-256
- Export defaults are intentionally conservative to reduce accidental leakage
- Integrity failures are surfaced before import continues

## Tech stack

- Frontend: Vite + React + TypeScript
- Desktop shell: Tauri 2
- Backend: Rust
- State/query: Zustand + TanStack Query

## Repository structure

```text
src/
  App.tsx                 Main application UI
  lib/api.ts              Tauri invoke bridge
  lib/store.ts            Zustand store
  lib/types.ts            Shared frontend types
src-tauri/
  src/lib.rs              Core Tauri backend commands and logic
  src/main.rs             Tauri entry
  resources/*.mjs         Runtime helper scripts
tools/
  *.mjs                   Build and packaging helpers
docs/
  *.md                    User, build, CLI, update, and packaging docs
```

## Supported builds

The repository currently includes build flows for:

- Windows installer: `zh-CN`, `en-US`
- Windows release binaries: `zh-CN`, `en-US`
- macOS app/dmg: `zh-CN`, `en-US`
- standalone CLI archives: `zh-CN`, `en-US`

Common scripts are defined in [package.json](package.json):

```bash
pnpm build
pnpm build:nsis
pnpm build:nsis:zh-CN
pnpm build:nsis:en-US
pnpm build:mac
pnpm build:mac:zh-CN
pnpm build:mac:en-US
pnpm build:cli
pnpm build:cli:archive
```

## Local development

### Prerequisites

- Node.js 18+
- `pnpm`
- Rust toolchain
- Tauri 2 prerequisites for your platform
- A usable local OpenClaw installation if you want to test full launch behavior

### Install dependencies

```bash
pnpm install
```

### Frontend development

```bash
pnpm dev
```

### Tauri development

```bash
pnpm tauri dev
```

### Minimal verification

- Frontend-only changes: `pnpm build`
- Tauri/Rust changes: `cargo test --manifest-path src-tauri/Cargo.toml`
- Cross-layer changes: run both

## Build and packaging

### Windows NSIS installers

```bash
pnpm build:nsis
```

Localized builds:

```bash
pnpm build:nsis:zh-CN
pnpm build:nsis:en-US
```

### Windows release binaries without installer

```bash
pnpm build:release:zh-CN
pnpm build:release:en-US
```

### macOS bundles

Run these on macOS:

```bash
pnpm build:mac
pnpm build:mac-app
pnpm build:mac-dmg
```

See [docs/macos-build.md](docs/macos-build.md) for path, signing, and notarization notes.

### Standalone CLI

```bash
pnpm build:cli
pnpm build:cli:archive
```

See [docs/cli.md](docs/cli.md) for supported commands and archive output details.

## Auto update and release flow

The repository contains GitHub Actions workflows for CI, release packaging, and standalone CLI builds. The updater release artifacts are designed to be mirrored to a public release repository so the app can fetch locale-specific update manifests.

See:

- [docs/auto-build-and-update.md](docs/auto-build-and-update.md)

Key points:

- CI runs `pnpm build` and Rust tests
- release tags such as `v0.1.x` produce localized Windows assets
- updater JSON manifests are published separately for `zh-CN` and `en-US`

## `.claw` package format and security

`.claw` is a ZIP-based archive used by the launcher for profile import/export. A valid package includes a `manifest.json` describing exported entries and their hashes.

Import checks include:

- manifest existence and parsing
- supported format version
- missing or extra files
- file size consistency
- SHA-256 consistency

Related docs:

- [docs/claw-package-verification.md](docs/claw-package-verification.md)
- [docs/user-guide-zh.md](docs/user-guide-zh.md)

### Important safety note

Even when export options exclude memory or account data, package safety should still be verified manually before sharing with others. A profile may contain secrets written by custom skills, third-party tools, or user-created files outside the launcher's intended sensitive-data filters.

Recommended practice:

1. Export a package.
2. Import it into a clean location.
3. Inspect the directory contents and talk to the profile if needed.
4. Confirm no personal secrets remain.
5. Re-export the reviewed copy for distribution.

## Current limitations

- The launcher depends on local OpenClaw runtime behavior and filesystem layout
- Some runtime checks are platform-specific and may need per-platform validation
- CLI chat currently supports non-streaming mode only
- macOS distribution still requires proper code signing and notarization for public installs

## Documentation

- [docs/user-guide-zh.md](docs/user-guide-zh.md)
- [docs/cli.md](docs/cli.md)
- [docs/macos-build.md](docs/macos-build.md)
- [docs/claw-package-verification.md](docs/claw-package-verification.md)
- [docs/auto-build-and-update.md](docs/auto-build-and-update.md)

## Contributing

Issues and pull requests are welcome. Before submitting changes:

- keep TypeScript and Rust field names consistent
- avoid weakening `.claw` export/import safety rules
- test the smallest relevant surface first
- preserve compatibility for both default local profiles and imported profiles

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).
