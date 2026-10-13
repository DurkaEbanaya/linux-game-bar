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
            let status = fps::authorize()?;
            println!(
                "FPS helper authorization started (PID {}); diagnostics: {}",
                status.id(),
                fps::authorization_log().display()
            );
            Ok(())
        }
        "fps-probes" | "trace-fps" => {
            let pid = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("Specify PID"))?
                .parse()?;
            let process =
                metrics::process_id(pid).ok_or_else(|| anyhow::anyhow!("Process not found"))?;
            let probes = fps::discover(process)?;
            if args[1] == "fps-probes" {
                println!("{}", serde_json::to_string_pretty(&probes)?);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&fps::request(
                        &gamebar_core::FpsRequest::Attach { process, probes }
                    )?)?
                );
            }
            Ok(())
        }
        "fps-status" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&fps::request(&gamebar_core::FpsRequest::Status)?)?
            );
            Ok(())
        }
        "audio-list" => {
            let (commands, command_rx) = crossbeam_channel::bounded(8);
            let (updates, update_rx) = crossbeam_channel::bounded(8);
            audio::spawn(command_rx, updates);
            let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let mut latest = None;
            while std::time::Instant::now() < until {
                if let Ok(update) = update_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    latest = Some(update);
                }
            }
            let _ = commands.send(gamebar_core::Command::Shutdown);
            let latest = latest.ok_or_else(|| anyhow::anyhow!("No PipeWire snapshot received"))?;
            println!(
                "{}\n{}",
                latest.status,
                serde_json::to_string_pretty(&latest.streams)?
            );
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!(
                "Linux Game Bar\n\n  gamebar run [--show]   Start background panel (KDE: Win+G)\n  gamebar toggle         Toggle panel (Hyprland/Sway key binding)\n  gamebar show|hide|quit  Control running panel\n  gamebar enable-fps     Authorize FPS hot-attach helper with pkexec\n  gamebar audio-list     Diagnose live per-app PipeWire streams"
            );
            Ok(())
        }
        other => anyhow::bail!("Unknown command {other}; see gamebar --help"),
    }
}
