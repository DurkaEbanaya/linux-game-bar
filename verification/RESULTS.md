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
- Counter accuracy calibration/overhead (actual verifier load/hot-attach performed later).
- Overwatch/Vulkan gameplay and OpenGL acceptance.
- Hyprland/Sway sessions, fullscreen focus restoration, hotplug and fractional scaling.

Compilation is not proof of runtime compositor/game compatibility. The later live
diagnosis below supersedes the initial no-attach status for Overwatch/Vulkan only.

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

## Live FPS diagnosis after authorization

The user reported FPS still absent after authorization. Direct `pkexec` execution
failed with `Permission denied`. SELinux audit (`ausearch -m AVC -ts recent -i`)
confirmed denied `{ entrypoint }` on `~/.local/bin/gamebar-fps-helper`, type `home_bin_t`.
SELinux remained enforcing. Installed root-owned helper to `/usr/local/libexec`,
restored default `bin_t` label, then started it through authorized systemd-run.

Real hot-attach to running Overwatch PID 88141 succeeded with target custom Mesa WSI.
One actual helper snapshot after accumulation:

- present FPS: `162.58951225715137`
- mean frame interval: `6.150458206790123 ms`
- p95 interval: `6.555699 ms`
- events in window: `325`, lost: `0`
- swapchain: `0x7f70b010dda0`

This demonstrates actual event delivery and computation in Overwatch, not calibrated
accuracy or overhead. No automated tests were run. Earlier “privileged attach not
performed” statements describe the initial build; this diagnosis supersedes them
for this Overwatch session only. OpenGL remains unverified.

Panel now prefers the root-owned system helper, captures stderr and exit status,
and displays actionable SELinux installation diagnostics. Install script supports
`--system-helper` with restorecon.

## Movable metric widgets and persistent HUD

The user confirmed FPS works, then requested simpler application/audio lists,
AVG / 1% / 0.1% lows, individual frametimes, movable/hideable metrics and an
Afterburner-style HUD retained in-game.

Implemented:
- Two tabs; automatic active-window target, integrated playback/capture controls
  in the CPU/GPU-heavy application list and resources table.
- Current FPS over 2 s; AVG/lows over up to 30 s with explicit sample count/time,
  lows unavailable below 100/1000 intervals. Loss clears statistics; swapchain
  changes discard earlier intervals. Frame graph uses per-present timestamps,
  chronological samples and min/max pixel envelopes (up to 4096 samples).
- Independently movable/resizable/hideable metric windows. Layout, theme, scale,
  visibility, HUD enablement and background opacity saved to user config JSON.
- Closing menu retains a transparent HUD: empty wl_surface input region and
  layer-shell keyboard interactivity None. Menu reopens with exclusive keyboard
  focus and a fresh layer surface. HUD disable destroys surface/GPU presentation.

Compilation/formatting/clippy/release checks passed. No automated tests added/run.
Updated user binaries and root-owned system helper installed; both transient
systemd services restarted and report active/running without startup errors.
The new HUD's graphical interaction, input passthrough and low-FPS accuracy have
not received gameplay/visual acceptance; the previous working FPS counter does
not establish those new behaviors.

## Per-widget controls and Overwatch audio identity fix

Added a native Painter pencil to every metric with independently saved background
opacity and reading update interval (250/500 ms, 1/2/5/10 s). Each widget caches its
own reading/frametime plot and schedules its deadline; FPS polling separated from
the one-second resources sampler and runs at 250 ms. Existing layout JSON is
compatible via defaults for added fields.

Live read-only audio diagnosis found Overwatch node props omit application PID/name;
the associated PipeWire Client owns them. Backend now subscribes to Clients and
merges node/client identity, even when Client info arrives after Node discovery.
Monitor/capture-sink nodes are filtered after full Node info (not only registry props).
Active game's audio controls are placed immediately below its title in the panel.

`cargo run --quiet --bin gamebar -- audio-list` returned exactly 3 Overwatch playback
streams and 1 Overwatch capture stream, PID 88141/starttime 3311996, all controllable,
volume 1.0 and unmuted. Their application key matches the sampled Wine application.
This confirms discovery/association/writable Props, not an audible mute acceptance.
No automated tests were run; actual audio was left at its original volume/mute.
Format/check/clippy/release build passed on final sources. Updated binaries installed;
panel restarted as transient user service and reports active/running. Installed
`gamebar audio-list` confirms the same four controllable Overwatch streams.
