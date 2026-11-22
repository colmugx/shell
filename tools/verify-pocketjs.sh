#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Real PocketJS v0.17.0 compile, host admission, QuickJS frame execution.
# Never opens /dev/dri, stops Sway or claims a display.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
UPSTREAM="$ROOT/pocketjs-upstream"
if (( $# > 0 )); then UPSTREAM="$1"; fi
UPSTREAM="$(cd "$UPSTREAM" && pwd -P)"
command -v bun >/dev/null || { echo "Bun 1.4.2 required" >&2; exit 2; }
command -v cargo >/dev/null || { echo "Rust 1.98.1 required" >&2; exit 2; }
[[ -f "$UPSTREAM/programs/linux-kms/pocket.host.json" ]] ||
  { echo "PocketJS v0.17.0 Linux host contract not found" >&2; exit 2; }
[[ -f "$UPSTREAM/tools/pocket.ts" ]] ||
  { echo "PocketJS compiler missing" >&2; exit 2; }
EXPECTED="83c6cac996237968785125118b543cd4e339254c"
ACTUAL="$(git -C "$UPSTREAM" rev-parse HEAD)"
[[ "$ACTUAL" == "$EXPECTED" ]] ||
  { echo "PocketJS revision mismatch; requires pinned v0.17.0" >&2; exit 2; }
APP_DIR="$UPSTREAM/apps/pokshell"
[[ ! -e "$APP_DIR" ]] ||
  { echo "Refusing to replace existing upstream apps/pokshell" >&2; exit 2; }
cp -R "$ROOT/ui/pocketjs" "$APP_DIR"
python3 "$ROOT/tools/prepare-pocketjs-host.py" "$UPSTREAM"
cd "$UPSTREAM"
bun tools/pocket.ts check --config apps/pokshell/pocket.host.config.ts
bun tools/pocket.ts compile --config apps/pokshell/pocket.host.config.ts \
  --output dist/pokshell-home.pkb
bun tools/pocket.ts build --config programs/linux-kms/pocket.host.config.ts \
  --out dist/pokshell-linux
HOST="dist/pokshell-linux/runtime/pocket-linux-kms"
[[ -x "$HOST" ]] || { echo "Native PocketJS runtime absent" >&2; exit 1; }
"$HOST" --package apps/pokshell/dist/pokshell-home.pkb --check
"$HOST" --package apps/pokshell/dist/pokshell-home.pkb --headless --frames 4
echo "PASS P2-J01: native PocketJS Linux host executed original TSX/QuickJS UI for 4 ticks"
cargo test --manifest-path programs/linux-kms/Cargo.toml --bin pocket-linux-kms
python3 "$ROOT/tools/test-pocketjs-service.py" "$UPSTREAM"
