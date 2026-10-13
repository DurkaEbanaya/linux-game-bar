use gamebar_core::{Application, Process, ProcessId, SystemMetrics};
use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix::fs::MetadataExt,
    time::Instant,
};

pub fn process_id(pid: u32) -> Option<ProcessId> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    Some(ProcessId {
        pid,
        start_ticks: rest.split_whitespace().nth(19)?.parse().ok()?,
    })
}

/// Cgroups separate desktop application instances; Wine's Windows executable separates games
/// from their launcher instead of putting every wine64 process in one application.
pub fn identity(pid: u32) -> (String, String, String) {
    let exe = fs::read_link(format!("/proc/{pid}/exe"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let comm = fs::read_to_string(format!("/proc/{pid}/comm"))
        .unwrap_or_default()
        .trim()
        .to_owned();
    let cmd = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let args: Vec<_> = cmd
        .split(|c| *c == 0)
        .filter_map(|s| std::str::from_utf8(s).ok())
        .collect();
    let windows = args
        .iter()
        .find(|s| s.to_ascii_lowercase().ends_with(".exe"));
    let name = windows
        .map(|s| s.rsplit(['/', '\\']).next().unwrap_or(s).to_string())
        .unwrap_or_else(|| comm.clone());
    let cgroup = fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
    let scope = cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .unwrap_or("");
    let app_scope = scope
        .rsplit('/')
        .next()
        .filter(|s| s.starts_with("app-") && (s.ends_with(".scope") || s.ends_with(".service")));
    let key = if let Some(win) = windows {
        format!("wine:{}:{}", scope, win.to_ascii_lowercase())
    } else if let Some(scope) = app_scope {
        format!("cgroup:{scope}")
    } else if !exe.is_empty() {
        format!("exe:{exe}")
    } else {
        format!("pid:{pid}:{comm}")
    };
    (key, name, exe)
}

#[derive(Default, Clone)]
struct Counters {
    ticks: u64,
    io: Option<u64>,
}
#[derive(Default, Clone)]
struct DrmClient {
    engines: HashMap<String, (u64, u64)>,
    vram: Option<u64>,
}

pub struct Sampler {
    previous: HashMap<ProcessId, Counters>,
    previous_drm: HashMap<String, DrmClient>,
    time: Instant,
    total_cpu: Option<(u64, u64)>,
    ticks_per_sec: f64,
    page_size: u64,
    pub selected: Option<ProcessId>,
}
impl Sampler {
    pub fn new() -> Self {
        Self {
            previous: HashMap::new(),
            previous_drm: HashMap::new(),
            time: Instant::now(),
            total_cpu: None,
            ticks_per_sec: unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64,
            page_size: unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64,
            selected: None,
        }
    }
    pub fn sample(&mut self) -> (Vec<Application>, SystemMetrics) {
        let now = Instant::now();
        let dt = now.duration_since(self.time).as_secs_f64().max(0.001);
        let mut next = HashMap::new();
        let mut next_drm = HashMap::new();
        let mut groups: HashMap<String, Application> = HashMap::new();
        let mut group_clients: HashMap<String, HashSet<String>> = HashMap::new();
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                else {
                    continue;
                };
                let path = entry.path();
                let Ok(stat) = fs::read_to_string(path.join("stat")) else {
                    continue;
                };
                let Some(rest) = stat.rfind(')').and_then(|i| stat.get(i + 2..)) else {
                    continue;
                };
                let fields: Vec<_> = rest.split_whitespace().collect();
                if fields.len() < 22 {
                    continue;
                }
                let parse = |i: usize| fields[i].parse::<u64>().unwrap_or(0);
                let id = ProcessId {
                    pid,
                    start_ticks: parse(19),
                };
                let ticks = parse(11) + parse(12);
                let io: Option<u64> = fs::read_to_string(path.join("io")).ok().map(|s| {
                    s.lines()
                        .filter(|l| l.starts_with("read_bytes:") || l.starts_with("write_bytes:"))
                        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                        .sum()
                });
                let prev = self.previous.get(&id);
                let cpu = prev
                    .and_then(|p| ticks.checked_sub(p.ticks))
                    .map(|v| v as f64 / self.ticks_per_sec / dt * 100.);
                let io_rate = prev
                    .and_then(|p| io?.checked_sub(p.io?))
                    .map(|v| v as f64 / dt);
                next.insert(id, Counters { ticks, io });
                let (key, name, exe) = identity(pid);
                let clients = drm_clients(pid);
                let mut gpu = None;
                let mut vram = None;
                for (client, counters) in &clients {
                    if let Some(old) = self.previous_drm.get(client) {
                        let busy = counters
                            .engines
                            .iter()
                            .filter_map(|(engine, (value, capacity))| {
                                let old = old.engines.get(engine)?.0;
                                Some(
                                    value.checked_sub(old)? as f64
                                        / (dt * 1e9 * (*capacity).max(1) as f64)
                                        * 100.,
                                )
                            })
                            .fold(None, |acc: Option<f64>, value| {
                                Some(acc.unwrap_or(0.).max(value))
                            });
                        gpu = sum_option(gpu, busy);
                    }
                    vram = sum_option(vram, counters.vram);
                    group_clients
                        .entry(key.clone())
                        .or_default()
                        .insert(client.clone());
                    next_drm.insert(client.clone(), counters.clone());
                }
                let process = Process {
                    id,
                    parent: parse(1) as u32,
                    name: name.clone(),
                    executable: exe,
                    app_key: key.clone(),
                    cpu_percent: cpu,
                    memory_bytes: parse(21) * self.page_size,
                    gpu_percent: gpu,
                    vram_bytes: vram,
                    io_bytes_per_sec: io_rate,
                    owned: fs::metadata(&path)
                        .map(|m| m.uid() == unsafe { libc::getuid() })
                        .unwrap_or(false),
                };
                let group = groups.entry(key.clone()).or_insert_with(|| Application {
                    key,
                    name,
                    ..Default::default()
                });
                group.cpu_percent = sum_option(group.cpu_percent, cpu);
                group.memory_bytes += process.memory_bytes;
                group.io_bytes_per_sec = sum_option(group.io_bytes_per_sec, io_rate);
                group.processes.push(process);
            }
        }
        // Shared DRM clients may occur through duplicated/inherited file descriptors: count once per app.
        for (key, clients) in group_clients {
            if let Some(group) = groups.get_mut(&key) {
                for client in clients {
                    let Some(current) = next_drm.get(&client) else {
                        continue;
                    };
                    group.vram_bytes = sum_option(group.vram_bytes, current.vram);
                    if let Some(old) = self.previous_drm.get(&client) {
                        let busy = current
                            .engines
                            .iter()
                            .filter_map(|(e, (v, c))| {
                                Some(
                                    v.checked_sub(old.engines.get(e)?.0)? as f64
                                        / (dt * 1e9 * (*c).max(1) as f64)
                                        * 100.,
                                )
                            })
                            .fold(None, |a: Option<f64>, v| Some(a.unwrap_or(0.).max(v)));
                        group.gpu_percent = sum_option(group.gpu_percent, busy);
                    }
                }
            }
        }
        self.previous = next;
        self.previous_drm = next_drm;
        self.time = now;
        let mut apps: Vec<_> = groups.into_values().collect();
        apps.sort_by(|a, b| {
            b.cpu_percent
                .unwrap_or(0.)
                .total_cmp(&a.cpu_percent.unwrap_or(0.))
        });
        (apps, self.system())
    }
    fn system(&mut self) -> SystemMetrics {
        let mut out = SystemMetrics::default();
        if let Ok(stat) = fs::read_to_string("/proc/stat") {
            let vals: Vec<u64> = stat
                .lines()
                .next()
                .unwrap_or("")
                .split_whitespace()
                .skip(1)
                .take(8)
                .filter_map(|s| s.parse().ok())
                .collect();
            let total: u64 = vals.iter().sum();
            let idle = vals.get(3).copied().unwrap_or(0) + vals.get(4).copied().unwrap_or(0);
            if let Some((old_total, old_idle)) = self.total_cpu {
                if let (Some(t), Some(i)) =
                    (total.checked_sub(old_total), idle.checked_sub(old_idle))
                {
                    if t > 0 {
                        out.cpu_percent = Some((t.saturating_sub(i)) as f64 / t as f64 * 100.);
                    }
                }
            }
            self.total_cpu = Some((total, idle));
        }
        if let Ok(mem) = fs::read_to_string("/proc/meminfo") {
            let value = |name: &str| {
                mem.lines()
                    .find(|l| l.starts_with(name))
                    .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                    .unwrap_or(0)
                    * 1024
            };
            out.memory_total = value("MemTotal:");
            out.memory_used = out.memory_total.saturating_sub(value("MemAvailable:"));
        }
        if let Ok(cards) = fs::read_dir("/sys/class/drm") {
            for card in cards.flatten() {
                let name = card.file_name().to_string_lossy().to_string();
                if !name
                    .strip_prefix("card")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                {
                    continue;
                }
                let device = card.path().join("device");
                let busy = read_u64(device.join("gpu_busy_percent"));
                let vram = read_u64(device.join("mem_info_vram_total"));
                if busy.is_none() && vram.is_none() {
                    continue;
                }
                if out.gpu_percent.is_some() && vram.unwrap_or(0) < out.vram_total.unwrap_or(0) {
                    continue;
                }
                out.gpu_name = fs::read_to_string(device.join("product_name"))
                    .ok()
                    .map(|s| s.trim().to_owned())
                    .or(Some(name));
                out.gpu_percent = busy.map(|v| v as f64);
                out.vram_total = vram;
                out.vram_used = read_u64(device.join("mem_info_vram_used"));
                if let Ok(hwmons) = fs::read_dir(device.join("hwmon")) {
                    for h in hwmons.flatten() {
                        out.gpu_temperature =
                            read_u64(h.path().join("temp1_input")).map(|v| v as f64 / 1000.);
                    }
                }
            }
        }
        if let Ok(hwmons) = fs::read_dir("/sys/class/hwmon") {
            for h in hwmons.flatten() {
                let name = fs::read_to_string(h.path().join("name")).unwrap_or_default();
                if matches!(name.trim(), "coretemp" | "k10temp" | "zenpower") {
                    out.cpu_temperature =
                        read_u64(h.path().join("temp1_input")).map(|v| v as f64 / 1000.);
                }
            }
        }
        out
    }
}
fn read_u64(path: impl AsRef<std::path::Path>) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn sum_option<T: std::ops::Add<Output = T>>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + b),
        (a, None) => a,
        (None, b) => b,
    }
}
fn drm_clients(pid: u32) -> HashMap<String, DrmClient> {
    let mut clients = HashMap::new();
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fdinfo")) else {
        return clients;
    };
    for entry in entries.flatten() {
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if !text.contains("drm-client-id:") {
            continue;
        }
        let mut id = None;
        let mut device = None;
        let mut out = DrmClient::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let mut values = value.split_whitespace();
            let Some(first) = values.next() else { continue };
            match key {
                "drm-client-id" => id = Some(first.to_owned()),
                "drm-pdev" => device = Some(first.to_owned()),
                "drm-resident-vram" => {
                    out.vram = first.parse::<u64>().ok().map(|v| {
                        if values.next() == Some("KiB") {
                            v * 1024
                        } else {
                            v
                        }
                    })
                }
                _ => {
                    if let Some(engine) = key.strip_prefix("drm-engine-capacity-") {
                        if let Ok(v) = first.parse() {
                            out.engines.entry(engine.into()).or_insert((0, 1)).1 = v;
                        }
                    } else if let Some(engine) = key.strip_prefix("drm-engine-") {
                        if values.next() == Some("ns") {
                            if let Ok(v) = first.parse() {
                                out.engines.entry(engine.into()).or_insert((0, 1)).0 = v;
                            }
                        }
                    }
                }
            }
        }
        // On old kernels without drm-pdev use the DRM fd's device number, not the process/fd number.
        let device = device.or_else(|| {
            fs::metadata(format!(
                "/proc/{pid}/fd/{}",
                entry.file_name().to_string_lossy()
            ))
            .ok()
            .map(|m| m.rdev().to_string())
        });
        if let (Some(id), Some(device)) = (id, device) {
            clients.insert(format!("{device}:{id}"), out);
        }
    }
    clients
}

pub fn terminate(ids: &[ProcessId], force: bool) -> anyhow::Result<()> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    for id in ids {
        anyhow::ensure!(
            id.pid > 1 && id.pid != std::process::id(),
            "Refusing to terminate gamebar or PID 1"
        );
        // Open first, then check identity; pidfd_send_signal cannot hit a later reused PID.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, id.pid, 0) } as i32;
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        anyhow::ensure!(
            process_id(id.pid) == Some(*id),
            "Process {} exited or its PID was reused",
            id.pid
        );
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                if force { libc::SIGKILL } else { libc::SIGTERM },
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
