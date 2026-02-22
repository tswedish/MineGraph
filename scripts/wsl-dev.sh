#!/usr/bin/env bash
# RamseyNet WSL2 dev helper
# Run from Windows: wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh <command>
#
# Commands:
#   test      - cargo test --all
#   clippy    - cargo clippy --all-targets -- -D warnings
#   build     - cargo build --all
#   web       - pnpm install && pnpm build (in web/)
#   ci        - run full CI suite (clippy + test + web build)
#   sync      - copy Windows repo to WSL2 filesystem
#   server    - start the API server on port 3001

set -euo pipefail

# Use WSL2-native repo path
REPO="/root/RamseyNet"
cd "$REPO"

# Ensure cargo is in PATH
source /root/.cargo/env 2>/dev/null || true

cmd="${1:-ci}"
shift 2>/dev/null || true

case "$cmd" in
  test)
    echo "=== Running tests ==="
    cargo test --all "$@"
    ;;
  clippy)
    echo "=== Running clippy ==="
    cargo clippy --all-targets -- -D warnings "$@"
    ;;
  build)
    echo "=== Building all crates ==="
    cargo build --all "$@"
    ;;
  web)
    echo "=== Building web app ==="
    cd web
    CI=true pnpm install
    pnpm build
    ;;
  ci)
    echo "=== Full CI suite ==="
    echo "--- clippy ---"
    cargo clippy --all-targets -- -D warnings
    echo "--- tests ---"
    cargo test --all
    echo "--- web build ---"
    cd web
    CI=true pnpm install
    pnpm build
    echo "=== CI passed! ==="
    ;;
  sync)
    echo "=== Syncing from Windows ==="
    rsync -av --delete \
      --exclude='target/' \
      --exclude='web/node_modules/' \
      --exclude='web/build/' \
      --exclude='web/.svelte-kit/' \
      --exclude='.claude/' \
      --exclude='ramseynet.db' \
      /mnt/c/Users/trist/RamseyNet/ /root/RamseyNet/
    echo "=== Sync complete ==="
    ;;
  server)
    echo "=== Starting server on :3001 ==="
    cargo run -p ramseynet-server "$@"
    ;;
  *)
    echo "Usage: wsl-dev.sh {test|clippy|build|web|ci|sync|server}"
    exit 1
    ;;
esac
