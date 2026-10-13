//! GUI-independent snapshots, identities and commands. Missing measurements stay missing.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessId {
    pub pid: u32,
    /// Linux starttime (clock ticks since boot), protects against PID reuse.
    pub start_ticks: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Process {
    pub id: ProcessId,
    pub parent: u32,
    pub name: String,
    pub executable: String,
    pub app_key: String,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: u64,
    pub gpu_percent: Option<f64>,
    pub vram_bytes: Option<u64>,
    pub io_bytes_per_sec: Option<f64>,
    pub owned: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Application {
    pub key: String,
    pub name: String,
    pub processes: Vec<Process>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: u64,
    pub gpu_percent: Option<f64>,
    pub vram_bytes: Option<u64>,
    pub io_bytes_per_sec: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Playback,
    Capture,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioStream {
    pub id: u32,
    pub serial: u64,
    pub direction: Direction,
    pub app_key: String,
    pub name: String,
    pub process: Option<ProcessId>,
    pub volume: f32,
    pub muted: bool,
    pub controllable: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_percent: Option<f64>,
    pub memory_used: u64,
    pub memory_total: u64,
    pub gpu_percent: Option<f64>,
    pub gpu_temperature: Option<f64>,
    pub gpu_name: Option<String>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub cpu_temperature: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActiveWindow {
    pub pid: Option<u32>,
    pub title: String,
    pub output: Option<String>,
    pub window_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FpsSnapshot {
    pub process: Option<ProcessId>,
    pub fps: Option<f64>,
    pub frame_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub source: String,
    pub status: String,
    pub frames: u64,
    pub lost: u64,
    pub swapchain: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: u64,
    pub applications: Vec<Application>,
    pub streams: Vec<AudioStream>,
    pub system: SystemMetrics,
    pub fps: FpsSnapshot,
    pub audio_status: String,
    pub errors: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    ProDark,
    StudioLight,
    ModernDark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Cpu,
    Memory,
    Gpu,
    Io,
}

#[derive(Clone, Debug)]
pub enum Command {
    AudioVolume {
        app: String,
        direction: Direction,
        volume: f32,
    },
    AudioMute {
        app: String,
        direction: Direction,
        muted: bool,
    },
    ForgetAudioPolicy {
        app: String,
    },
    SelectProcess(Option<ProcessId>),
    TraceFps(ProcessId),
    StopFps,
    Terminate {
        processes: Vec<ProcessId>,
        force: bool,
    },
    CloseWindow(ActiveWindow),
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum FpsRequest {
    Attach {
        process: ProcessId,
        probes: Vec<Probe>,
    },
    Detach,
    Status,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProbeApi {
    Vulkan,
    Egl,
    Glx,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub path: String,
    pub offset: u64,
    pub api: ProbeApi,
    pub label: String,
}
