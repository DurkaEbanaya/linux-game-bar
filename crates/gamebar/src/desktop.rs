use crate::ipc::Control;
use crossbeam_channel::{Receiver, Sender, bounded};
use gamebar_core::ActiveWindow;
use std::{fs, process::Command as ProcessCommand, thread};

struct Bridge {
    active: Sender<ActiveWindow>,
    control: Sender<Control>,
}
#[zbus::interface(name = "io.github.DurkaEbanaya.GameBar")]
impl Bridge {
    fn active_window(&self, payload: &str) {
        if payload.len() <= 16384
            && let Ok(active) = serde_json::from_str::<ActiveWindow>(payload)
        {
            let _ = self.active.try_send(active);
        }
    }
    fn toggle(&self) {
        let _ = self.control.try_send(Control::Toggle);
    }
}
pub fn start(control: Sender<Control>) -> Receiver<ActiveWindow> {
    let (tx, rx) = bounded(32);
    thread::Builder::new()
        .name("desktop-bridge".into())
        .spawn(move || {
            let bridge = Bridge {
                active: tx,
                control,
            };
            let connection = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name("io.github.DurkaEbanaya.GameBar"))
                .and_then(|b| b.serve_at("/io/github/DurkaEbanaya/GameBar", bridge))
                .and_then(|b| b.build());
            let Ok(connection) = connection else {
                eprintln!("Desktop D-Bus bridge unavailable");
                return;
            };
            if std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .contains("KDE")
            {
                if let Err(e) = load_kwin(&connection) {
                    eprintln!("KWin integration: {e}");
                }
            }
            loop {
                thread::park();
            }
        })
        .expect("spawn desktop bridge");
    rx
}
fn load_kwin(connection: &zbus::blocking::Connection) -> anyhow::Result<()> {
    let dir = crate::ipc::runtime_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("kwin.js");
    fs::write(&path, include_str!("../../../integration/kwin/gamebar.js"))?;
    let proxy = zbus::blocking::Proxy::new(
        connection,
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    )?;
    let _: bool = proxy
        .call("unloadScript", &("linux-game-bar",))
        .unwrap_or(false);
    // KWin uses deleteLater for unload; wait before loading the same plugin name.
    for _ in 0..20 {
        let loaded: bool = proxy.call("isScriptLoaded", &("linux-game-bar",))?;
        if !loaded {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    let id: i32 = proxy.call(
        "loadScript",
        &(path.to_string_lossy().as_ref(), "linux-game-bar"),
    )?;
    anyhow::ensure!(id >= 0, "KWin refused script");
    let script = zbus::blocking::Proxy::new(
        connection,
        "org.kde.KWin",
        format!("/Scripting/Script{id}"),
        "org.kde.kwin.Script",
    )?;
    let _: () = script.call("run", &())?;
    Ok(())
}
pub fn query_active(fallback: &ActiveWindow) -> ActiveWindow {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        if let Ok(output) = ProcessCommand::new("hyprctl")
            .args(["-j", "activewindow"])
            .output()
        {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                let monitor = ProcessCommand::new("hyprctl")
                    .args(["-j", "monitors"])
                    .output()
                    .ok()
                    .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
                    .and_then(|m| {
                        m.as_array()?
                            .iter()
                            .find(|m| m["id"] == v["monitor"])?
                            .get("name")?
                            .as_str()
                            .map(str::to_owned)
                    });
                return ActiveWindow {
                    pid: v["pid"].as_u64().map(|p| p as u32),
                    title: v["title"].as_str().unwrap_or("").into(),
                    output: monitor,
                    window_id: v["address"].as_str().map(str::to_owned),
                };
            }
        }
    }
    if std::env::var_os("SWAYSOCK").is_some() {
        if let Ok(output) = ProcessCommand::new("swaymsg")
            .args(["-t", "get_tree", "-r"])
            .output()
        {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(active) = find_sway(&v, None) {
                    return active;
                }
            }
        }
    }
    fallback.clone()
}
fn find_sway(v: &serde_json::Value, output: Option<String>) -> Option<ActiveWindow> {
    let output = if v["type"] == "output" {
        v["name"].as_str().map(str::to_owned)
    } else {
        output
    };
    if v["focused"] == true && v["pid"].is_number() {
        return Some(ActiveWindow {
            pid: v["pid"].as_u64().map(|v| v as u32),
            title: v["name"].as_str().unwrap_or("").into(),
            output,
            window_id: v["id"].as_u64().map(|v| v.to_string()),
        });
    }
    for field in ["nodes", "floating_nodes"] {
        if let Some(nodes) = v[field].as_array() {
            for node in nodes {
                if let Some(active) = find_sway(node, output.clone()) {
                    return Some(active);
                }
            }
        }
    }
    None
}
pub fn close_window(window: &ActiveWindow) -> anyhow::Result<()> {
    let id = window
        .window_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No window identity"))?;
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        let out = ProcessCommand::new("hyprctl")
            .args(["dispatch", "closewindow", &format!("address:{id}")])
            .output()?;
        anyhow::ensure!(
            out.status.success(),
            "hyprctl: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return Ok(());
    }
    if std::env::var_os("SWAYSOCK").is_some() {
        let _: u64 = id.parse()?;
        let out = ProcessCommand::new("swaymsg")
            .arg(format!("[con_id={id}] kill"))
            .output()?;
        anyhow::ensure!(out.status.success(), "swaymsg close failed");
        return Ok(());
    }
    let conn = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", "/KWin", "org.kde.KWin")?;
    // KWin has no public close(uuid); use a short-lived native script matching internalId.
    let _ = proxy;
    anyhow::ensure!(
        id.chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, '-' | '{' | '}')),
        "Invalid KWin window ID"
    );
    let path = crate::ipc::runtime_dir().join("close.js");
    fs::write(
        &path,
        format!(
            "var windows=workspace.windowList();for(var i=0;i<windows.length;i++){{if(String(windows[i].internalId)==='{id}')windows[i].closeWindow();}}"
        ),
    )?;
    let scripting = zbus::blocking::Proxy::new(
        &conn,
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    )?;
    let _: bool = scripting
        .call("unloadScript", &("linux-game-bar-close",))
        .unwrap_or(false);
    for _ in 0..20 {
        let loaded: bool = scripting.call("isScriptLoaded", &("linux-game-bar-close",))?;
        if !loaded {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(25));
    }
    let number: i32 = scripting.call(
        "loadScript",
        &(path.to_string_lossy().as_ref(), "linux-game-bar-close"),
    )?;
    anyhow::ensure!(number >= 0, "KWin refused close-window script");
    let script = zbus::blocking::Proxy::new(
        &conn,
        "org.kde.KWin",
        format!("/Scripting/Script{number}"),
        "org.kde.kwin.Script",
    )?;
    let _: () = script.call("run", &())?;
    Ok(())
}
