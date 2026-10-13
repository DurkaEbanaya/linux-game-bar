#!/usr/bin/env bash
set -euo pipefail
root="$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")"
version="${1:-0.1.6}"
name="linux-game-bar-${version}-linux-x86_64"
mkdir -p "$root/dist/$name/bin" "$root/dist/$name/scripts"
install -m 0755 "$root/target/release/gamebar" "$root/target/release/gamebar-fps-helper" "$root/dist/$name/bin/"
install -m 0755 "$root/scripts/install.sh" "$root/dist/$name/scripts/"
cp "$root/README.md" "$root/LICENSE-MIT" "$root/LICENSES.md" "$root/dist/$name/"
tar -C "$root/dist" -czf "$root/dist/$name.tar.gz" "$name"
(cd "$root/dist" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
printf 'Packaged: %s/dist/%s.tar.gz\n' "$root" "$name"
