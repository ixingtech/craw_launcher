# Standalone CLI

The repository now includes a standalone CLI binary: `claws-cli`.

## Supported commands

```bash
claws-cli settings show
claws-cli profiles list
claws-cli profiles launch [profile-id]
claws-cli profiles import ./demo.claw --name demo
claws-cli profiles export <profile-id> --out ./demo.claw
claws-cli inventory show <profile-id>
claws-cli inventory preview <profile-id> skills weather
claws-cli inventory readme <profile-id>
claws-cli chat send "hello" --profile-id <profile-id>
```

Notes:

- Chat currently supports non-streaming mode only.
- `--launcher-home` can override the launcher data directory for the CLI.
- `--json` prints machine-readable output.

## Local build

```bash
pnpm build:cli
pnpm build:cli:archive
```

Current-platform archives are written to `src-tauri/target/release`.

## CI build

GitHub Actions workflow: `.github/workflows/build-cli.yml`

It builds standalone CLI artifacts for:

- Windows
- macOS
- Linux
