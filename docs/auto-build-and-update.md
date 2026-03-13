# Auto Build And Update

## GitHub Actions

The repository now includes:

- `.github/workflows/ci.yml`
  - runs on pull requests and `main` pushes only
  - runs `pnpm build`
  - runs `cargo test --manifest-path src-tauri/Cargo.toml`
- `.github/workflows/build-cli.yml`
  - runs only through `workflow_dispatch`
  - builds standalone CLI archives for Linux, macOS, and Windows
- `.github/workflows/release.yml`
  - triggers on tags like `v0.1.4`
  - builds zh-CN and en-US Windows installers
  - uploads signed assets to the private source repository release
  - mirrors updater assets to the public updater repository release

## Required GitHub Secrets

Set these repository secrets before using the release workflow:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- `PUBLIC_RELEASES_TOKEN`

`PUBLIC_RELEASES_TOKEN` must have access to:

- the private source repository `yipuwangc/claws_launcher`
- the public updater repository `yipuwangc/claws-launcher-releases`

The checked-in public key is:

- `src-tauri/updater-public-key.pub`

If you want to rotate the updater signing identity:

1. Generate a new signer keypair with `pnpm tauri signer generate`.
2. Replace `src-tauri/updater-public-key.pub`.
3. Update the two Tauri signing secrets.

## Release Flow

1. Bump the app version.
2. Push a tag such as `v0.1.4`.
3. GitHub Actions builds and uploads:
   - `xiaolongxia.exe`
   - `Craw Launcher.exe`
   - `xiaolongxia_<version>_windows_x64.exe`
   - `craw-launcher_<version>_windows_x64.exe`
   - `latest-zh-CN.json`
   - `latest-en-US.json`
4. The updater assets are mirrored to `yipuwangc/claws-launcher-releases`.

## Public Updater Repository

The updater repository must not stay empty. Create an initial commit once, for example:

1. Add a `README.md`
2. Push the default branch

After that, `release.yml` can create or update releases in the public repository.

## Auto Update

The launcher checks these endpoints:

- `https://github.com/yipuwangc/claws-launcher-releases/releases/latest/download/latest-zh-CN.json`
- `https://github.com/yipuwangc/claws-launcher-releases/releases/latest/download/latest-en-US.json`

The zh-CN build uses the zh-CN manifest, and the en-US build uses the en-US manifest.
