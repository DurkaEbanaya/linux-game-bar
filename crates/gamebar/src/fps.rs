//! Resolve real driver dispatch addresses in our process, then translate to mapped ELF offsets.
//! No injection, environment variables or launch wrapper in the target game.
use gamebar_core::{FpsRequest, FpsSnapshot, Probe, ProbeApi, ProcessId};
use object::{Object, ObjectSegment, ObjectSymbol};
use std::{
    collections::HashSet,
    ffi::{CStr, CString},
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

pub fn socket_path() -> PathBuf {
    PathBuf::from(format!("/run/linux-game-bar/{}.sock", unsafe {
        libc::getuid()
    }))
}
pub fn request(request: &FpsRequest) -> anyhow::Result<FpsSnapshot> {
    let mut stream = UnixStream::connect(socket_path())
        .map_err(|e| anyhow::anyhow!("FPS helper unavailable: {e}. Run gamebar enable-fps"))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
    serde_json::to_writer(&mut stream, request)?;
    stream.write_all(b"\n")?;
    let mut line = String::new();
    BufReader::new(stream).take(65536).read_line(&mut line)?;
    Ok(serde_json::from_str(&line)?)
}

pub fn authorize() -> anyhow::Result<std::process::Child> {
    let system = PathBuf::from("/usr/local/libexec/gamebar-fps-helper");
    let helper = if system.is_file() {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(&system)?;
        anyhow::ensure!(
            metadata.uid() == 0 && metadata.mode() & 0o022 == 0,
            "System FPS helper must be root-owned and not writable by group/others"
        );
        system
    } else {
        std::env::current_exe()?.with_file_name("gamebar-fps-helper")
    };
    anyhow::ensure!(
        helper.is_file(),
        "Build/install gamebar-fps-helper beside gamebar first"
    );
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::ipc::runtime_dir();
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let log = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(authorization_log())?;
    Ok(std::process::Command::new("pkexec")
        .arg(helper)
        .arg("--uid")
        .arg(unsafe { libc::getuid() }.to_string())
        .stderr(std::process::Stdio::from(log))
        .spawn()?)
}
pub fn authorization_log() -> PathBuf {
    crate::ipc::runtime_dir().join("fps-helper.log")
}
pub fn authorization_error(status: Option<std::process::ExitStatus>) -> String {
    let details = fs::File::open(authorization_log())
        .ok()
        .and_then(|file| {
            let mut text = String::new();
            file.take(16384).read_to_string(&mut text).ok()?;
            Some(text)
        })
        .unwrap_or_default();
    format!(
        "FPS-helper не запущен ({}). {}{}",
        status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "таймаут авторизации".into()),
        details.trim(),
        if details.contains("Permission denied") {
            " Установите системный helper: bash scripts/install.sh --system-helper (SELinux запрещает privileged entrypoint в домашнем каталоге)."
        } else {
            ""
        }
    )
}

pub fn discover(process: ProcessId) -> anyhow::Result<Vec<Probe>> {
    anyhow::ensure!(
        crate::metrics::process_id(process.pid) == Some(process),
        "Target process exited"
    );
    let maps = fs::read_to_string(format!("/proc/{}/maps", process.pid))?;
    let paths: HashSet<String> = maps
        .lines()
        .filter_map(mapping_path)
        .filter(|p| p.starts_with('/'))
        .collect();
    let vulkan = paths.iter().any(|p| p.contains("libvulkan"));
    if vulkan {
        // Mesa retains local ELF symbols in many custom/debug builds. Resolve the target's
        // own ICD, not the panel's system ICD (Steam runtime/custom Mesa may differ).
        for path in paths
            .iter()
            .filter(|p| p.contains("libvulkan_") || p.contains("libvulkan.so"))
        {
            if let Ok(offset) = symbol_offset(path, "wsi_QueuePresentKHR") {
                return Ok(vec![Probe {
                    path: path.clone(),
                    offset,
                    api: ProbeApi::Vulkan,
                    label: "Mesa WSI QueuePresentKHR · target ICD".into(),
                }]);
            }
        }
        let candidates = resolve_vulkan().unwrap_or_default();
        let selected: Vec<_> = candidates
            .into_iter()
            .filter_map(|mut probe| {
                // Namespace path names can differ; only accept the exact same device+inode.
                use std::os::unix::fs::MetadataExt;
                let source = fs::metadata(&probe.path).ok()?;
                let target = paths.iter().find(|path| {
                    fs::metadata(path)
                        .is_ok_and(|m| m.dev() == source.dev() && m.ino() == source.ino())
                })?;
                probe.path = target.clone();
                Some(probe)
            })
            .collect();
        if !selected.is_empty() {
            return Ok(selected);
        }
        anyhow::bail!(
            "Vulkan ICD выбранного процесса не совпадает с драйвером панели, символ WSI не найден. PID {}. Не переключаюсь на GLX: игра использует Vulkan.",
            process.pid
        );
    }
    for (pattern, symbol, api) in [
        ("libEGL", "eglSwapBuffers", ProbeApi::Egl),
        ("libGLX", "glXSwapBuffers", ProbeApi::Glx),
        ("libGL.so", "glXSwapBuffers", ProbeApi::Glx),
    ] {
        for path in paths.iter().filter(|p| p.contains(pattern)) {
            if let Ok(offset) = symbol_offset(path, symbol) {
                return Ok(vec![Probe {
                    path: path.clone(),
                    offset,
                    api,
                    label: symbol.into(),
                }]);
            }
        }
    }
    anyhow::bail!(
        "No supported present dispatch found in PID {} (Vulkan driver mapping / EGL / GLX)",
        process.pid
    )
}
fn mapping_path(line: &str) -> Option<String> {
    let mut rest = line;
    for _ in 0..5 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        rest = &rest[end..];
    }
    let path = rest.trim_start();
    (!path.is_empty()).then(|| path.to_owned())
}
fn symbol_offset(path: &str, symbol: &str) -> anyhow::Result<u64> {
    let data = fs::read(path)?;
    let object = object::File::parse(data.as_slice())?;
    let address = object
        .dynamic_symbols()
        .chain(object.symbols())
        .find(|s| s.name() == Ok(symbol) && s.is_definition())
        .ok_or_else(|| anyhow::anyhow!("{symbol} not found"))?
        .address();
    for segment in object.segments() {
        let (offset, size) = segment.file_range();
        if address >= segment.address() && address < segment.address() + size {
            return Ok(offset + address - segment.address());
        }
    }
    anyhow::bail!("No ELF segment for symbol")
}
fn address_probe(address: *const libc::c_void, label: &str) -> Option<Probe> {
    // dladdr identifies the actual ICD, including stripped Mesa drivers; executable mapping
    // supplies the on-disk uprobe offset, independent of the game's ASLR.
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    if unsafe { libc::dladdr(address, &mut info) } == 0 || info.dli_fname.is_null() {
        return None;
    }
    let path = unsafe { CStr::from_ptr(info.dli_fname) }
        .to_string_lossy()
        .into_owned();
    let path = fs::canonicalize(path).ok()?.to_string_lossy().into_owned();
    let maps = fs::read_to_string("/proc/self/maps").ok()?;
    for line in maps.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let (start, end) = fields[0].split_once('-')?;
        let (start, end) = (
            u64::from_str_radix(start, 16).ok()?,
            u64::from_str_radix(end, 16).ok()?,
        );
        let addr = address as usize as u64;
        if addr >= start && addr < end {
            return Some(Probe {
                path,
                offset: u64::from_str_radix(fields[2], 16).ok()? + addr - start,
                api: ProbeApi::Vulkan,
                label: label.into(),
            });
        }
    }
    None
}
fn resolve_vulkan() -> anyhow::Result<Vec<Probe>> {
    use ash::vk;
    let entry = unsafe { ash::Entry::load() }?;
    let name = CString::new("Linux Game Bar dispatch resolver")?;
    let app = vk::ApplicationInfo::default()
        .application_name(&name)
        .api_version(vk::API_VERSION_1_1);
    let info = vk::InstanceCreateInfo::default().application_info(&app);
    let instance = unsafe { entry.create_instance(&info, None) }?;
    let result = (|| -> anyhow::Result<Vec<Probe>> {
        let mut probes = vec![];
        for physical in unsafe { instance.enumerate_physical_devices() }? {
            let queues = unsafe { instance.get_physical_device_queue_family_properties(physical) };
            let Some(index) = queues.iter().position(|q| {
                q.queue_count > 0 && q.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            }) else {
                continue;
            };
            let queue = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(index as u32)
                .queue_priorities(&[1.]);
            let extensions = [ash::khr::swapchain::NAME.as_ptr()];
            let info = vk::DeviceCreateInfo::default()
                .queue_create_infos(std::slice::from_ref(&queue))
                .enabled_extension_names(&extensions);
            let Ok(device) = (unsafe { instance.create_device(physical, &info, None) }) else {
                continue;
            };
            if let Some(function) = unsafe {
                instance.get_device_proc_addr(device.handle(), c"vkQueuePresentKHR".as_ptr())
            } {
                if let Some(probe) = address_probe(
                    function as *const libc::c_void,
                    "Vulkan ICD QueuePresentKHR",
                ) {
                    if !probes
                        .iter()
                        .any(|p: &Probe| p.path == probe.path && p.offset == probe.offset)
                    {
                        probes.push(probe);
                    }
                }
            }
            unsafe { device.destroy_device(None) };
        }
        Ok(probes)
    })();
    unsafe { instance.destroy_instance(None) };
    result
}
