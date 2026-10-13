# Verification status

Environment: openSUSE Tumbleweed, x86_64, Rust/Cargo 1.98.1,
KDE Plasma Wayland/KWin 6.7.5, PipeWire 1.6.9, libbpf 1.7.0,
clang 23. The workspace requires Rust 1.95+.

## Performed

- `cargo check --workspace` — passed.
- `cargo fmt --all --check` — passed on final sources.
- `cargo clippy --workspace -- -D warnings` — passed on final sources.
- `cargo build --release --workspace` — passed; both executables built.
- eBPF C compiled by clang as part of helper build.
- `bash -n scripts/install.sh` — passed (shell syntax only).
- `bash scripts/install.sh` — completed; `gamebar` and `gamebar-fps-helper`
  installed to `~/.local/bin`, desktop entry to `~/.local/share/applications`.
  Autostart was not enabled and the panel/helper were not launched.
- Host protocol discovery: layer-shell available; KWin scripting interface available.

## Not performed (user requested no tests)

- Automated unit/integration tests.
- Opening/closing the actual overlay and screenshots/visual acceptance.
- Audio playback/capture mute behavior, new-stream policy, server-restart acceptance.
- Privileged eBPF verifier load, actual hot-attach/counter accuracy/overhead.
- Overwatch/Vulkan gameplay and OpenGL acceptance.
- Hyprland/Sway sessions, fullscreen focus restoration, hotplug and fractional scaling.

No screenshot, FPS numbers or performance timing is claimed. Compilation is not
proof of runtime compositor/game compatibility. Mandatory FPS coverage remains
an **unverified experimental implementation** pending real-game checks.

The README lists unsupported capture-node fallback, persistence, namespace/driver
matching and input/scaling limitations explicitly.
