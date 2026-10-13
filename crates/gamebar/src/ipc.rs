use crossbeam_channel::Sender;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    thread,
};

#[derive(Clone, Debug)]
pub enum Control {
    Toggle,
    Show,
    Hide,
    Quit,
    ToggleHud,
}
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })))
        .join("linux-game-bar")
}
pub fn connect(command: &str) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(runtime_dir().join("control.sock"))?;
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}
pub fn listen(sender: Sender<Control>) -> anyhow::Result<UnixListener> {
    let dir = runtime_dir();
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let path = dir.join("control.sock");
    if UnixStream::connect(&path).is_ok() {
        anyhow::bail!("Game Bar is already running")
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    let accept = listener.try_clone()?;
    thread::Builder::new()
        .name("control".into())
        .spawn(move || {
            for stream in accept.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(1)));
                let mut line = String::new();
                if BufReader::new(stream)
                    .take(128)
                    .read_line(&mut line)
                    .is_err()
                {
                    continue;
                }
                let control = match line.trim() {
                    "toggle" => Control::Toggle,
                    "show" => Control::Show,
                    "hide" => Control::Hide,
                    "quit" => Control::Quit,
                    "hud-toggle" => Control::ToggleHud,
                    _ => continue,
                };
                if sender.send(control).is_err() {
                    break;
                }
            }
        })?;
    Ok(listener)
}
