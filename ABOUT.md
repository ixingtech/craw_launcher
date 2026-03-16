# About This Repository

`openclaw-launcher` is a Tauri desktop launcher for operating local OpenClaw runtimes and profiles on Windows and WSL.

It exists to solve the operational layer around OpenClaw:

- selecting the correct executable and data directory
- managing the default local profile plus imported profiles
- launching one profile with the right runtime and gateway wiring
- inspecting profile files such as README, skills, cron jobs, conversations, and notifications
- importing and exporting `.claw` packages with manifest and SHA-256 verification

The repository is intentionally split into a React frontend and a Rust backend:

- `src/` contains the UI, state, and Tauri invoke bridge
- `src-tauri/src/lib.rs` contains runtime, profile, import/export, gateway, and chat logic

Current platform focus:

- Windows desktop builds
- Windows + WSL OpenClaw runtime management
- localized `zh-CN` and `en-US` packaging

Important product constraints:

- profile export is conservative by default and avoids leaking memory/account/device data unless explicitly requested
- imported package verification is part of the product contract and should not be weakened
- profile scanning must remain compatible with OpenClaw's existing on-disk layout
- runtime switching must keep Windows and WSL state isolated where appropriate

This repo is not a general-purpose OpenClaw UI rewrite. Its main value is reliable local profile operations, packaging safety, and runtime orchestration.
