//! Privileged service: only same-user processes, only files actually mapped by that process.
#![allow(clippy::collapsible_if)]
use anyhow::{Context, Result};
use gamebar_core::{FpsRequest, FpsSnapshot, Probe, ProbeApi, ProcessId};
use libbpf_rs::{
    Link, MapCore, MapFlags, Object, ObjectBuilder, RingBuffer, RingBufferBuilder, UprobeOpts,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ffi::CString,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Trace {
    links: Vec<Link>,
    ring: RingBuffer<'static>,
    object: Object,
    process: ProcessId,
    events: Rc<RefCell<HashMap<u64, VecDeque<u64>>>>,
    source: String,
    started: Instant,
    last_lost: u64,
    loss_until: u64,
}
impl Trace {
    fn attach(uid: u32, process: ProcessId, probes: Vec<Probe>) -> Result<Self> {
        validate_process(uid, process)?;
        anyhow::ensure!(
            !probes.is_empty() && probes.len() <= 8,
            "Invalid number of probes"
        );
        let maps = fs::read_to_string(format!("/proc/{}/maps", process.pid))?;
        for (index, probe) in probes.iter().enumerate() {
            anyhow::ensure!(
                !probes[..index]
                    .iter()
                    .any(|p| p.path == probe.path && p.offset == probe.offset),
                "Duplicate probe would double-count frames"
            );
            validate_probe(probe, &maps)?;
        }
        let object = ObjectBuilder::default()
            .open_memory(include_bytes!(concat!(env!("OUT_DIR"), "/present.bpf.o")))?
            .load()?;
        let enter = object
            .progs_mut()
            .find(|p| p.name() == "enter")
            .context("Missing entry program")?;
        let leave = object
            .progs_mut()
            .find(|p| p.name() == "leave")
            .context("Missing return program")?;
        let mut links = vec![];
        for probe in &probes {
            let cookie = match probe.api {
                ProbeApi::Vulkan => 1,
                ProbeApi::Egl => 2,
                ProbeApi::Glx => 3,
            };
            links.push(enter.attach_uprobe_with_opts(
                process.pid as i32,
                &probe.path,
                probe.offset as usize,
                UprobeOpts {
                    cookie,
                    ..Default::default()
                },
            )?);
            links.push(leave.attach_uprobe_with_opts(
                process.pid as i32,
                &probe.path,
                probe.offset as usize,
                UprobeOpts {
                    cookie,
                    retprobe: true,
                    ..Default::default()
                },
            )?);
        }
        validate_process(uid, process)?;
        let events = Rc::new(RefCell::new(HashMap::<u64, VecDeque<u64>>::new()));
        let captured = events.clone();
        let map = object
            .maps()
            .find(|m| m.name() == "events")
            .context("Missing ring buffer")?;
        let mut builder = RingBufferBuilder::new();
        builder.add(&map, move |data| {
            if data.len() != 24 {
                return -libc::EINVAL;
            }
            let time = u64::from_ne_bytes(data[0..8].try_into().unwrap());
            let chain = u64::from_ne_bytes(data[8..16].try_into().unwrap());
            let mut events = captured.borrow_mut();
            // Bound both number of swapchains and history; stale entries are removed at sampling.
            if events.len() >= 64 && !events.contains_key(&chain) {
                return -libc::ENOSPC;
            }
            let list = events.entry(chain).or_default();
            list.push_back(time);
            while list.len() > 100000 {
                list.pop_front();
            }
            0
        })?;
        let ring = builder.build()?;
        let source = probes
            .iter()
            .map(|p| p.label.clone())
            .collect::<Vec<_>>()
            .join(" / ");
        Ok(Self {
            links,
            ring,
            object,
            process,
            events,
            source,
            started: Instant::now(),
            last_lost: 0,
            loss_until: 0,
        })
    }
    fn snapshot(&mut self) -> Result<FpsSnapshot> {
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        let now = ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64;
        let window = 2_000_000_000u64;
        let lost = self
            .object
            .maps()
            .find(|m| m.name() == "dropped")
            .context("Missing loss counter")?
            .lookup(&0u32.to_ne_bytes(), MapFlags::ANY)?
            .map(|v| u64::from_ne_bytes(v.try_into().unwrap()))
            .unwrap_or(0);
        if lost != self.last_lost {
            self.loss_until = now + window;
        }
        let invalid = now < self.loss_until;
        self.last_lost = lost;
        let mut events = self.events.borrow_mut();
        for list in events.values_mut() {
            while list
                .front()
                .is_some_and(|t| *t < now.saturating_sub(window))
            {
                list.pop_front();
            }
        }
        events.retain(|_, list| !list.is_empty());
        let best = events
            .iter()
            .filter(|(_, list)| {
                list.back()
                    .is_some_and(|time| now.saturating_sub(*time) < 500_000_000)
            })
            .max_by_key(|(_, list)| list.len());
        let mut snapshot = FpsSnapshot {
            process: Some(self.process),
            source: self.source.clone(),
            lost,
            status: "Waiting for successful presents…".into(),
            ..Default::default()
        };
        if let Some((chain, list)) = best {
            snapshot.swapchain = Some(*chain);
            snapshot.frames = list.len() as u64;
            if list.len() >= 2 && !invalid && self.started.elapsed() >= Duration::from_secs(2) {
                // Cadence from actual timestamps; not callback delivery times, not monitor Hz.
                let mut times: Vec<_> = list.iter().copied().collect();
                times.sort_unstable();
                let elapsed = times.last().unwrap() - times[0];
                if elapsed > 0 {
                    snapshot.fps = Some((times.len() - 1) as f64 * 1e9 / elapsed as f64);
                    snapshot.frame_ms = Some(elapsed as f64 / (times.len() - 1) as f64 / 1e6);
                }
                let mut intervals: Vec<_> = times
                    .windows(2)
                    .map(|t| (t[1] - t[0]) as f64 / 1e6)
                    .collect();
                intervals.sort_by(f64::total_cmp);
                snapshot.p95_ms = intervals
                    .get((intervals.len() * 95 / 100).min(intervals.len() - 1))
                    .copied();
                snapshot.status = format!("Attached · swapchain 0x{chain:x} · 2 s window");
            }
        }
        if invalid {
            snapshot.status = "Events lost; interval invalid".into();
        }
        Ok(snapshot)
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        self.links.clear();
    }
}
fn validate_process(uid: u32, id: ProcessId) -> Result<()> {
    anyhow::ensure!(
        id.pid > 1 && id.pid != std::process::id(),
        "Invalid target PID"
    );
    let path = format!("/proc/{}", id.pid);
    anyhow::ensure!(
        fs::metadata(&path)?.uid() == uid,
        "Target belongs to another user"
    );
    let stat = fs::read_to_string(format!("{path}/stat"))?;
    let tail = stat.rsplit_once(") ").context("Invalid process stat")?.1;
    let ticks = tail
        .split_whitespace()
        .nth(19)
        .context("Missing process starttime")?
        .parse::<u64>()?;
    anyhow::ensure!(ticks == id.start_ticks, "Target exited or PID was reused");
    Ok(())
}
fn validate_probe(probe: &Probe, maps: &str) -> Result<()> {
    let path = Path::new(&probe.path);
    anyhow::ensure!(
        path.is_absolute() && !probe.path.contains('\n'),
        "Invalid probe path"
    );
    let meta = fs::metadata(path)?;
    anyhow::ensure!(
        meta.is_file() && probe.offset < meta.len(),
        "Invalid probe offset"
    );
    let matches = maps.lines().any(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 || !fields[1].contains('x') || fields[5] != probe.path {
            return false;
        }
        let Some((start, end)) = fields[0].split_once('-') else {
            return false;
        };
        let (Ok(start), Ok(end), Ok(offset), Ok(inode)) = (
            u64::from_str_radix(start, 16),
            u64::from_str_radix(end, 16),
            u64::from_str_radix(fields[2], 16),
            fields[4].parse::<u64>(),
        ) else {
            return false;
        };
        let Some((major, minor)) = fields[3].split_once(':') else {
            return false;
        };
        let (Ok(major), Ok(minor)) = (
            u64::from_str_radix(major, 16),
            u64::from_str_radix(minor, 16),
        ) else {
            return false;
        };
        inode == meta.ino()
            && libc::major(meta.dev()) == major as u32
            && libc::minor(meta.dev()) == minor as u32
            && probe.offset >= offset
            && probe.offset < offset + end - start
    });
    anyhow::ensure!(
        matches,
        "Probe file/offset is not an executable mapping of the target"
    );
    Ok(())
}
fn peer_uid(stream: &UnixStream) -> Result<u32> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of_val(&cred) as libc::socklen_t;
    anyhow::ensure!(
        unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut size,
            )
        } == 0,
        "Cannot authenticate client"
    );
    Ok(cred.uid)
}
fn main() -> Result<()> {
    anyhow::ensure!(
        unsafe { libc::geteuid() } == 0,
        "Start through gamebar enable-fps / pkexec"
    );
    anyhow::ensure!(
        std::env::consts::ARCH == "x86_64",
        "FPS probe ABI currently supports x86_64 only"
    );
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3 && args[1] == "--uid",
        "Usage: gamebar-fps-helper --uid UID"
    );
    let uid = args[2].parse::<u32>()?;
    anyhow::ensure!(uid > 0, "Refusing root GUI user");
    if let Ok(pk_uid) = std::env::var("PKEXEC_UID") {
        anyhow::ensure!(pk_uid.parse::<u32>()? == uid, "Authorization user mismatch");
    }
    let dir = PathBuf::from("/run/linux-game-bar");
    if !dir.exists() {
        fs::create_dir(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))?;
    }
    let meta = fs::symlink_metadata(&dir)?;
    anyhow::ensure!(
        meta.is_dir() && meta.uid() == 0 && meta.mode() & 0o022 == 0,
        "Invalid root-owned runtime directory"
    );
    let path = dir.join(format!("{uid}.sock"));
    if UnixStream::connect(&path).is_ok() {
        anyhow::bail!("Helper already running")
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    let cpath = CString::new(path.as_os_str().as_encoded_bytes())?;
    anyhow::ensure!(
        unsafe { libc::chown(cpath.as_ptr(), uid, u32::MAX) } == 0,
        "Socket ownership failed"
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let (tx, rx) = mpsc::sync_channel(8);
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            if peer_uid(&stream).ok() != Some(uid) {
                continue;
            }
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            let request = BufReader::new(&mut stream)
                .take(65536)
                .read_line(&mut line)
                .ok()
                .and_then(|_| serde_json::from_str::<FpsRequest>(&line).ok());
            if let Some(request) = request {
                if tx.send((request, stream)).is_err() {
                    break;
                }
            }
        }
    });
    let mut trace: Option<Trace> = None;
    let mut idle = Instant::now();
    let mut identity_check = Instant::now();
    loop {
        if let Some(t) = trace.as_mut() {
            let identity = if identity_check.elapsed() >= Duration::from_secs(1) {
                identity_check = Instant::now();
                validate_process(uid, t.process)
            } else {
                Ok(())
            };
            if let Err(e) = identity.and_then(|_| t.ring.poll(Duration::ZERO).map_err(Into::into)) {
                eprintln!("Detach: {e}");
                trace = None;
            }
        }
        match rx.recv_timeout(if trace.is_some() {
            Duration::from_millis(10)
        } else {
            Duration::from_millis(250)
        }) {
            Ok((request, mut stream)) => {
                idle = Instant::now();
                let result = match request {
                    FpsRequest::Attach { process, probes } => {
                        trace = None;
                        Trace::attach(uid, process, probes).and_then(|mut t| {
                            let s = t.snapshot()?;
                            trace = Some(t);
                            Ok(s)
                        })
                    }
                    FpsRequest::Detach => {
                        trace = None;
                        Ok(FpsSnapshot {
                            status: "Detached".into(),
                            ..Default::default()
                        })
                    }
                    FpsRequest::Status => {
                        trace.as_mut().map(Trace::snapshot).unwrap_or_else(|| {
                            Ok(FpsSnapshot {
                                status: "Idle".into(),
                                ..Default::default()
                            })
                        })
                    }
                };
                let snapshot = result.unwrap_or_else(|e| FpsSnapshot {
                    status: format!("FPS attach error: {e:#}"),
                    ..Default::default()
                });
                let _ = serde_json::to_writer(&mut stream, &snapshot);
                let _ = stream.write_all(b"\n");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if idle.elapsed() > Duration::from_secs(600) {
            break;
        }
    }
    drop(trace);
    let _ = fs::remove_file(path);
    Ok(())
}
