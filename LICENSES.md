# Licenses and provenance

- Rust code outside `crates/fps-helper`: **MIT OR Apache-2.0**, at your option.
- `crates/fps-helper` (Rust service and its embedded BPF object): **GPL-2.0-only**.
- `crates/ui-kit` adapts Tokens, typography, buttons and original Painter geometry
  from the supplied `rust-gui-silver-bullet` kit, offered under MIT OR Apache-2.0.
  The original editor/canvas application was not copied.
- `crates/fps-helper/src/present.bpf.c`: **GPL-2.0-only**, as identified by its SPDX
  header and BPF program license. It is compiled into the helper. Redistribution
  of that combined binary must satisfy applicable GPL obligations; the complete
  corresponding source is included in this repository.
- egui/egui-wgpu/wgpu, SCT and all other dependencies retain their own licenses.
  Native PipeWire/Wayland/libbpf/libelf dependencies are linked from the system.
- No MangoHud source or renderer was copied. Its Vulkan implementation was researched
  to understand dispatch and metrics requirements.
- No binary fonts or third-party artwork are bundled. Locally installed optional
  fonts retain their licenses (Inter/JetBrains Mono: OFL; DejaVu: its font license).

MIT terms: [LICENSE-MIT](LICENSE-MIT). Apache 2.0 terms:
<https://www.apache.org/licenses/LICENSE-2.0>.
GPLv2 terms: <https://www.gnu.org/licenses/old-licenses/gpl-2.0.html>.
