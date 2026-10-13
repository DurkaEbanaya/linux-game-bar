// Keep protocol/state-machine branches visually separate rather than folding every condition.
#![allow(clippy::collapsible_if)]
mod audio;
mod backend;
mod desktop;
mod fps;
mod ipc;
mod metrics;
mod overlay;
mod render;
mod ui;
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("run") {
        "toggle" | "show" | "hide" | "quit" => ipc::connect(&args[1]),
        "run" => overlay::run(args.iter().any(|a| a == "--show")),
        "enable-fps" => {
            let helper = std::env::current_exe()?.with_file_name("gamebar-fps-helper");
            anyhow::ensure!(
                helper.is_file(),
                "Build/install gamebar-fps-helper beside gamebar first"
            );
            let uid = unsafe { libc::getuid() };
            let status = std::process::Command::new("pkexec")
                .arg(helper)
                .arg("--uid")
                .arg(uid.to_string())
                .spawn()?;
            println!("FPS helper authorization started (PID {})", status.id());
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!(
                "Linux Game Bar\n\n  gamebar run [--show]   Start background panel (KDE: Win+G)\n  gamebar toggle         Toggle panel (Hyprland/Sway key binding)\n  gamebar show|hide|quit  Control running panel\n  gamebar enable-fps     Authorize FPS hot-attach helper with pkexec"
            );
            Ok(())
        }
        other => anyhow::bail!("Unknown command {other}; see gamebar --help"),
    }
}
