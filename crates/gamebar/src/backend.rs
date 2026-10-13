use crate::{audio, fps, metrics};
use crossbeam_channel::{Receiver, Sender, bounded};
use gamebar_core::{Command, FpsRequest, Snapshot};
use std::{
    thread,
    time::{Duration, Instant},
};

pub struct Backend {
    pub commands: Sender<Command>,
    pub snapshots: Receiver<Snapshot>,
    worker: thread::JoinHandle<()>,
}
impl Backend {
    pub fn start() -> Self {
        let (tx, rx) = bounded(64);
        let (snap_tx, snap_rx) = bounded(1);
        let (audio_tx, audio_rx) = bounded(64);
        let (updates_tx, updates_rx) = bounded(1);
        audio::spawn(audio_rx, updates_tx);
        let worker = thread::Builder::new()
            .name("metrics".into())
            .spawn(move || {
                let mut sampler = metrics::Sampler::new();
                let mut snapshot = Snapshot {
                    audio_status: "Connecting to PipeWire…".into(),
                    ..Default::default()
                };
                let mut next_sample = Instant::now();
                let mut tracing = false;
                let mut sent_revision = None;
                let mut authorization:Option<(std::process::Child,gamebar_core::ProcessId,Instant)>=None;
                snapshot.fps.status="FPS не подключён. Выберите игру и нажмите «Включить FPS»; потребуется авторизация helper.".into();
                loop {
                    if let Ok(command) = rx.recv_timeout(Duration::from_millis(100)) {
                        let result: anyhow::Result<()> = match command {
                            Command::Shutdown => {
                                let _ = audio_tx.send(Command::Shutdown);
                                let _ = fps::request(&FpsRequest::Detach);
                                break;
                            }
                            Command::AudioVolume { .. }
                            | Command::AudioMute { .. }
                            | Command::ForgetAudioPolicy { .. } => {
                                audio_tx.send(command).map_err(Into::into)
                            }
                            Command::SelectProcess(id) => {
                                sampler.selected = id;
                                if let Some(process)=id && snapshot.fps.process!=Some(process) && fps::socket_path().exists() {
                                    let _=fps::request(&FpsRequest::Detach);tracing=false;
                                    match fps::discover(process).and_then(|probes|fps::request(&FpsRequest::Attach {process,probes})) {
                                        Ok(value)=>{tracing=value.process.is_some();snapshot.fps=value;}
                                        Err(error)=>{snapshot.fps=gamebar_core::FpsSnapshot {status:error.to_string(),..Default::default()};}
                                    }
                                }
                                Ok(())
                            }
                            Command::TraceFps(process) => {
                                tracing = false;
                                // Failed discovery must not leave an older target tracing in the helper.
                                let _ = fps::request(&FpsRequest::Detach);
                                let result = fps::discover(process).and_then(|probes| {
                                    fps::request(&FpsRequest::Attach { process, probes })
                                });
                                match result {
                                    Ok(value) => {
                                        tracing = value.process.is_some();
                                        snapshot.fps = value;
                                        Ok(())
                                    }
                                    Err(e) => {
                                        snapshot.fps = gamebar_core::FpsSnapshot {
                                            status: e.to_string(),
                                            ..Default::default()
                                        };
                                        Err(e)
                                    }
                                }
                            }
                            Command::AuthorizeFps(process)=>{
                                if authorization.is_some() {Ok(())}else {
                                    fps::authorize().map(|child| {
                                        authorization=Some((child,process,Instant::now()));
                                        snapshot.fps.status="Ожидание авторизации FPS-helper…".into();
                                    })
                                }
                            }
                            Command::StopFps => {
                                if let Some((mut child,_,_))=authorization.take() {let _=child.kill();let _=child.wait();}
                                tracing = false;
                                fps::request(&FpsRequest::Detach).map(|v| snapshot.fps = v)
                            }
                            Command::Terminate { processes, force } => {
                                metrics::terminate(&processes, force)
                            }
                            Command::CloseWindow(window) => crate::desktop::close_window(&window),
                        };
                        if let Err(error) = result {
                            snapshot.errors.push(error.to_string());
                        }
                        snapshot.revision += 1;
                    }
                    if let Some((child,process,started))=authorization.as_mut() {
                        let ready=std::os::unix::net::UnixStream::connect(fps::socket_path()).is_ok();
                        if ready {
                            let target=*process;
                            authorization=None;
                            match fps::discover(target).and_then(|probes|fps::request(&FpsRequest::Attach {process:target,probes})) {
                                Ok(value)=>{tracing=value.process.is_some();snapshot.fps=value;}
                                Err(error)=>{snapshot.fps=gamebar_core::FpsSnapshot {status:error.to_string(),..Default::default()};}
                            }
                            snapshot.revision+=1;
                        }else {
                            let status=child.try_wait().ok().flatten();
                            if started.elapsed()>Duration::from_secs(120) || status.is_some() {
                                snapshot.fps.status=fps::authorization_error(status);
                                snapshot.errors.push(snapshot.fps.status.clone());
                                authorization=None;snapshot.revision+=1;
                            }
                        }
                    }
                    if let Ok(update) = updates_rx.try_recv() {
                        snapshot.streams = update.streams;
                        snapshot.audio_status = update.status;
                        snapshot.errors.extend(update.errors);
                        snapshot.revision += 1;
                    }
                    if Instant::now() >= next_sample {
                        let (applications, system) = sampler.sample();
                        snapshot.applications = applications;
                        snapshot.system = system;
                        if tracing {
                            match fps::request(&FpsRequest::Status) {
                                Ok(value) => {
                                    tracing = value.process.is_some();
                                    snapshot.fps = value
                                }
                                Err(e) => {
                                    snapshot.fps.fps = None;
                                    snapshot.fps.frame_ms = None;
                                    snapshot.fps.p95_ms = None;
                                    snapshot.fps.status = e.to_string();
                                }
                            }
                        }
                        snapshot.revision += 1;
                        next_sample = Instant::now() + Duration::from_secs(1);
                    }
                    if snapshot.errors.len() > 8 {
                        snapshot.errors.drain(..snapshot.errors.len() - 8);
                    }
                    // Single bounded slot: no queue backlog, worker never blocks the UI.
                    if sent_revision != Some(snapshot.revision)
                        && snap_tx.try_send(snapshot.clone()).is_ok()
                    {
                        sent_revision = Some(snapshot.revision);
                    }
                }
            })
            .expect("spawn metrics worker");
        Self {
            commands: tx,
            snapshots: snap_rx,
            worker,
        }
    }
    pub fn shutdown(self) {
        let _ = self.commands.send(Command::Shutdown);
        let _ = self.worker.join();
    }
}
