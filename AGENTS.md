# Linux Game Bar

Rust workspace: GUI-free `core`, Silver `ui-kit`, SCT/egui-wgpu `gamebar`, privileged
`fps-helper`. Keep heavy work in backend threads, use bounded snapshots and typed commands.
Unknown metrics are `Option`, never fabricated zeroes. Per-app and system metrics must stay
visibly distinct. Audio controls target streams, never a physical microphone/sink globally.

Use `cargo fmt --all --check`, `cargo check --workspace`,
`cargo clippy --workspace -- -D warnings`, `cargo build --release --workspace`.
The user currently requests **no tests**. Do not run or add tests without a new instruction.
Do not claim gameplay/compositor compatibility based on compilation.

KDE scripting JavaScript is only a compositor adapter; the UI is native Rust.
Privileged code must authenticate socket peers, validate process starttime and ownership,
and allow attachment only to executable file mappings of that process.
