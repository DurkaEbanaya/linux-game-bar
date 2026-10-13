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

## User-reported Overwatch issues and follow-up

User opened the overlay in Overwatch and reported missing FPS, immovable/fixed-size
panel, and reluctant/invisible cursor. Investigation found no FPS helper running
(user confirmed authorization was never started), fixed window geometry in code,
and no Wayland cursor installation. The game uses a custom Mesa ICD different from
the system ICD, inside Steam's runtime.

Changes: in-panel helper authorization + attach, automatic attachment on selection
when helper exists; resolve target ICD's Mesa WSI symbol; draggable/resizable panel,
UI zoom and geometry retained across hide/show; SCT themed cursor on pointer enter
and egui cursor-shape updates.

`gamebar fps-probes 88141` resolved the running Overwatch ICD to
`mesa-overwatch/26.3.0-devel-5c80eda3/lib/libvulkan_radeon.so`, file offset `0x1fdde0`
(`wsi_QueuePresentKHR`). This is resolution evidence, **not a measured FPS claim**.
Privileged attach still requires user authorization; FPS counts, resize interaction
and cursor behavior remain awaiting user confirmation after update.

Updated binaries installed and the old panel shut down through control IPC.
New panel started as transient user `linux-game-bar.service`; systemd reports
active/running, with no startup errors in its journal. The game was left running.
This confirms daemon startup only, not graphical interactions or FPS accuracy.
