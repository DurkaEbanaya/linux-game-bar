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

## v0.1.0 release refinements

- Opacity applies only to in-game HUD. Holding opacity slider hides menu/dimming
  and draws actual HUD with a small settings window; edit-mode windows stay opaque.
- Frametime snapshots now include kernel monotonic end timestamps. UI deduplicates
  overlapping snapshots into bounded history (12 s / 20000 samples), draws a fixed
  10-second time axis with 650 ms delayed playhead, updates the scroll every 33 ms
  and eases scale changes. Sample peaks remain measured min/max envelopes.
- Metric windows snap to screen/neighbour edges within 12 logical points and choose
  closest unoccupied candidate on release/resize when sufficient screen space exists.
  Existing overlapping layouts are settled on opening; dashboard is an obstacle.
- HUD visibility setting and separately assigned global shortcut (Meta+Shift+G by
  default), CLI hud-toggle and KWin D-Bus integration. Hyprland/Sway native binds
  applied through compositor commands. HUD recreation uses no keyboard/pointer focus.
- Release archive includes both binaries, installer and documentation; full source
  archive and checksum accompany binaries. Runtime compositor acceptance for these
  latest interactions remains pending; no automated tests added/run.

Final fmt/check/clippy/release build and shell syntax checks passed. Updated panel
and system helper installed. Panel user service active/running with no startup
errors. KDE KGlobalAccel reports HUD action bound to Qt key 301989959
(Meta+Shift+G); this verifies registration, not keyboard gameplay acceptance.

## v0.1.1 missing Frametime diagnosis

The user reported no Frametime graph. Saved layout shows FrameTime visible=true,
on-screen at (19,221). Live helper response returned valid FPS/frame intervals but
zero timestamps/sample time. Root process PID 99434 was still an older executable
(`readlink /proc/PID/exe`: system helper "(deleted)"); the earlier systemd helper
launch exited because that older pkexec helper already owned the socket.

Stopped the actual old process, installed/restarted current system helper, and
hot-attached to Overwatch PID 88141. Live response: FPS 193.02836796185963,
385 intervals and 385 matching monotonic timestamps, lost=0. Panel restarted.
Added explicit old-helper warning inside Frametime instead of silent blank plot;
documented helper restart during upgrade. Workspace version now 0.1.1.
Fmt/check/clippy/release build passed; no automated tests or visual acceptance run.

## CPU/GPU scale and disk throughput widgets

CPU process/application uses host online logical CPU count: normalized to 0–100%.
GPU aggregates unique clients per device/engine before taking the busiest engine,
bounded to 0–100%; process and system readings likewise bounded. No missing metric
is converted to zero; GPU remains an fdinfo estimate rather than hardware occupancy.

Separate read/write deltas from /proc/PID/io populate application throughput.
System throughput samples /proc/diskstats whole physical devices with /sys/block/NAME/device,
excluding partitions and virtual stacked devices, sectors fixed at kernel-defined 512 bytes.
New AppDisk and SystemDisk widgets default visible, have individual HUD opacity/refresh
controls and are merged into older saved layouts without resetting user positions.
Resources I/O tooltip breaks down reads/writes. metrics-status [PID] provides diagnostics.

Fmt/check/clippy/release build passed. Live read-only metrics-status 88141 returned
Overwatch CPU 6.7808%, GPU 63.3088%; system CPU 11.8186%, GPU 84%. Read and write
counter deltas were legitimately zero during that sample (idle disk interval).
No synthetic disk load, automated tests or visual acceptance performed.
Final v0.1.2 fmt/check/clippy/release build and installer syntax checks passed.
Updated binaries installed; user panel service restarted and reports active/running.

## v0.1.3 grid / resources visibility / smooth zoom

Added menu-only alignment grid, independent visible/snap settings, grid steps
8/16/24/32/48/64 logical pixels and an align-all action. Snap/nearest-free candidates
use grid coordinates; existing neighbour/screen snapping remains available without grid.
Metrics rendered on both tabs. Removed forced resources window min-width 1000,
so the existing horizontal table scroll avoids covering the metric workspace.

Zoom target stored separately from displayed zoom; exponential time-based transition
(rate 9/s, dt max50ms, epsilon0.0002) and 16ms repaint deadlines while changing.
Removed 0.05 slider step. Pointer event coordinates and stationary pointer tracking
rescaled to actual displayed zoom each animation frame. Target, not transient
interpolated value, persists. Automated tests/graphical acceptance not run.
Final fmt/check/clippy/release build and shell syntax checks passed. Updated user
binaries installed and panel restarted; user systemd unit active/running without
startup errors. Existing FPS-helper protocol remains compatible (unchanged).

## v0.1.4 live grid drag fix and square windows

Previous snap corrected stored geometry after egui painted its own freely moving
title bar; egui drag state could override it on the next frame. Metric windows now
disable native movement/title bar and provide a draggable header with pencil/close.
Pointer press origin captures a fixed offset. Before Window::show, pointer position
minus offset is quantized to the grid and constrained to grid-aligned screen bounds;
current_pos applies that position with no competing native drag. Collision settling
remains on release. Resize uses the existing native handles. HUD transition cancels drag.
All theme window corner radii and explicit HUD frame radius are zero.
No automated tests or visual/gameplay acceptance run; compilation does not verify
mouse interaction. The new pre-paint drag path replaces the flawed prior implementation.
Final format/check/clippy/release build passed; updated panel installed and transient
user service restarted active/running. FPS-helper protocol unchanged.

## v0.1.5 animated grid motion and neighbour displacement

Separated persistent grid target positions from transient displayed positions.
Displayed windows interpolate exponentially at 22/s (dt capped50ms, epsilon0.15
logical points), repaint every16ms until settled, both edit windows and HUD. Native
Window current_pos always uses displayed position; paint positions never overwrite
saved targets. Pointer offset captures the displayed origin for dragging.

New layout.rs performs transactional neighbour displacement: active metric fixed,
four escape sides ordered by distance/drag direction, grid-aligned placements,
screen/dashboard constraints, recursive chains with ancestor locking, budget512
candidate attempts. Failed branches roll back; only impossible placements keep last
feasible dragged position. Resize also attempts displacement before old settling fallback.
No automated tests or visual/gameplay acceptance performed.
Final v0.1.5 fmt/check/clippy/release build passed. Updated user binaries installed;
panel restarted as transient user service, active/running without startup errors.

## v0.1.6 metric size grid

Outer width/height quantized to grid: saved dimensions round upward; resize uses
nearest step with per-widget minimum dimensions. Explicit bottom-right drag handle
captures press position and original size, applies snapped geometry before paint,
pushes neighbours transactionally and rejects out-of-screen/impossible growth.
Window fixed_size prevents native egui/content from overwriting quantized dimensions;
scrolling accommodates long content. HUD minimum/maximum content area uses matching
outer size minus frame margins. Grid disabled: one logical pixel resize increments.
No automated tests or visual/gameplay acceptance run.
Final v0.1.6 format/check/clippy/release build passed. Updated user binaries installed;
panel restarted active/running with no startup errors. FPS protocol unchanged.
