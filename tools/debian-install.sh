#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cmake -S "$root" -B "$root/build" -DCMAKE_BUILD_TYPE=Release
cmake --build "$root/build" --parallel 2
sudo cmake --install "$root/build" --prefix /usr/local
echo 'Installed. Run sudo /usr/local/bin/pocket-session --game-bin /usr/local/bin/pocket-game'
