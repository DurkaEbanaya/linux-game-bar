#!/usr/bin/env bash
set -euo pipefail
root="$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")"
prefix="${PREFIX:-$HOME/.local}"
for binary in gamebar gamebar-fps-helper; do
  if [[ ! -x "$root/target/release/$binary" ]]; then
    printf 'Missing release binary: %s. Run cargo build --release --workspace first.\n' "$binary" >&2
    exit 1
  fi
done
install -d "$prefix/bin" "$prefix/share/applications"
install -m 0755 "$root/target/release/gamebar" "$root/target/release/gamebar-fps-helper" "$prefix/bin/"
if [[ "$prefix" == *$'\n'* || "$prefix" == *'"'* || "$prefix" == *'%'* || "$prefix" == *'\\'* || "$prefix" == *'`'* || "$prefix" == *'$'* ]]; then
  printf 'Unsupported special characters in PREFIX for desktop Exec.\n' >&2
  exit 1
fi
printf '[Desktop Entry]\nType=Application\nName=Linux Game Bar\nComment=Wayland gaming overlay, audio mixer and resources\nExec="%s/bin/gamebar" run\nIcon=applications-games\nTerminal=false\nCategories=Utility;System;\nStartupNotify=false\n' "$prefix" > "$prefix/share/applications/linux-game-bar.desktop"
printf 'Installed to %s. Start: "%s/bin/gamebar" run\n' "$prefix" "$prefix"
