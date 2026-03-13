#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT_DIR"

TARGET="${1:-}"

run() {
  printf '\n===> %s\n' "$*"
  "$@"
}

usage() {
  cat <<'EOF'
Usage: tools/build-release.sh <target>

Targets:
  windows-release-zh
  windows-release-en
  windows-release-all
  windows-installer-zh
  windows-installer-en
  windows-installer-all
  mac-zh
  mac-en
  mac-all
  cli-zh
  cli-en
  cli-all
  all
EOF
}

case "$TARGET" in
  windows-release-zh)
    run pnpm build:release:zh-CN
    ;;
  windows-release-en)
    run pnpm build:release:en-US
    ;;
  windows-release-all)
    run pnpm build:release:zh-CN
    run pnpm build:release:en-US
    ;;
  windows-installer-zh)
    run pnpm build:nsis:zh-CN
    ;;
  windows-installer-en)
    run pnpm build:nsis:en-US
    ;;
  windows-installer-all)
    run pnpm build:nsis:zh-CN
    run pnpm build:nsis:en-US
    ;;
  mac-zh)
    run pnpm build:mac:zh-CN
    ;;
  mac-en)
    run pnpm build:mac:en-US
    ;;
  mac-all)
    run pnpm build:mac:zh-CN
    run pnpm build:mac:en-US
    ;;
  cli-zh)
    run pnpm build:cli:archive:zh-CN
    ;;
  cli-en)
    run pnpm build:cli:archive:en-US
    ;;
  cli-all)
    run pnpm build:cli:archive:zh-CN
    run pnpm build:cli:archive:en-US
    ;;
  all)
    run pnpm build:release:zh-CN
    run pnpm build:release:en-US
    run pnpm build:nsis:zh-CN
    run pnpm build:nsis:en-US
    run pnpm build:mac:zh-CN
    run pnpm build:mac:en-US
    run pnpm build:cli:archive:zh-CN
    run pnpm build:cli:archive:en-US
    ;;
  *)
    usage
    exit 1
    ;;
esac
