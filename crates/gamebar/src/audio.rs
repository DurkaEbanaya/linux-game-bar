//! Native PipeWire registry + subscribed SPA Props. Never touches a physical source/sink mute.
use crate::metrics;
use crossbeam_channel::{Receiver, Sender};
use gamebar_core::{AudioStream, Command, Direction};
use pipewire::{
    self as pw,
    spa::{
        self,
        param::ParamType,
        pod::{
            Object, Pod, Property, Value, ValueArray, deserialize::PodDeserializer,
            serialize::PodSerializer,
        },
    },
};
use std::{cell::RefCell, collections::HashMap, io::Cursor, rc::Rc, thread, time::Duration};

#[derive(Clone, Default)]
struct Policy {
    volume: Option<f32>,
    muted: Option<bool>,
}
struct Entry {
    node: pw::node::Node,
    _listener: pw::node::NodeListener,
    stream: AudioStream,
    channels: Vec<f32>,
    pending_policy: bool,
    props_ready: bool,
}
#[derive(Default)]
struct State {
    nodes: HashMap<u32, Entry>,
    policies: HashMap<(String, Direction), Policy>,
    dirty: bool,
    errors: Vec<String>,
}
pub struct AudioUpdate {
    pub streams: Vec<AudioStream>,
    pub status: String,
    pub errors: Vec<String>,
}

pub fn spawn(commands: Receiver<Command>, updates: Sender<AudioUpdate>) {
    thread::Builder::new()
        .name("pipewire".into())
        .spawn(move || {
            pw::init();
            let state = Rc::new(RefCell::new(State::default()));
            loop {
                match run(&commands, &updates, state.clone()) {
                    Ok(true) => break,
                    result => {
                        let status = match result {
                            Err(e) => format!("PipeWire reconnect: {e}"),
                            _ => "PipeWire disconnected; reconnecting".into(),
                        };
                        let _ = updates.try_send(AudioUpdate {
                            streams: vec![],
                            status,
                            errors: vec![],
                        });
                        state.borrow_mut().nodes.clear();
                        thread::sleep(Duration::from_secs(2));
                    }
                }
            }
        })
        .expect("spawn PipeWire thread");
}
fn run(
    commands: &Receiver<Command>,
    updates: &Sender<AudioUpdate>,
    state: Rc<RefCell<State>>,
) -> anyhow::Result<bool> {
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core=context.connect_rc(Some(pw::properties::properties! {"application.name"=>"Linux Game Bar", "application.id"=>"io.github.DurkaEbanaya.GameBar"}))?;
    let weak = main_loop.downgrade();
    let errors = Rc::downgrade(&state);
    let _core_listener = core
        .add_listener_local()
        .error(move |id, _, _, message| {
            if let Some(state) = errors.upgrade() {
                let mut s = state.borrow_mut();
                s.errors.push(message.to_owned());
                s.dirty = true;
            }
            if id == 0 {
                if let Some(loop_) = weak.upgrade() {
                    loop_.quit();
                }
            }
        })
        .register();
    let registry = core.get_registry_rc()?;
    let weak_registry = registry.downgrade();
    let weak_state = Rc::downgrade(&state);
    let removed_state = Rc::downgrade(&state);
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }
            let Some(props) = global.props.as_ref() else {
                return;
            };
            let direction = match props.get("media.class") {
                Some("Stream/Output/Audio") => Direction::Playback,
                Some("Stream/Input/Audio") => Direction::Capture,
                _ => return,
            };
            let (Some(registry), Some(state)) = (weak_registry.upgrade(), weak_state.upgrade())
            else {
                return;
            };
            let Ok(node) = registry.bind::<pw::node::Node, _>(global) else {
                return;
            };
            let pid = props
                .get("application.process.id")
                .and_then(|p| p.parse().ok());
            let process = pid.and_then(metrics::process_id);
            let (key, process_name, _) = pid.map(metrics::identity).unwrap_or_default();
            let name = props
                .get("application.name")
                .or_else(|| props.get("node.description"))
                .or_else(|| props.get("node.name"))
                .unwrap_or(&process_name)
                .to_owned();
            let key = if key.is_empty() {
                format!(
                    "audio:{}:{}",
                    props.get("application.id").unwrap_or(&name),
                    props.get("client.id").unwrap_or("unknown")
                )
            } else {
                key
            };
            let id = global.id;
            let info_state = Rc::downgrade(&state);
            let param_state = Rc::downgrade(&state);
            let listener = node
                .add_listener_local()
                .info(move |info| {
                    if let Some(state) = info_state.upgrade() {
                        let mut state = state.borrow_mut();
                        if let Some(entry) = state.nodes.get_mut(&id) {
                            entry.stream.controllable = info.params().iter().any(|p| {
                                p.id() == ParamType::Props
                                    && p.flags().contains(spa::param::ParamInfoFlags::WRITE)
                            });
                            if entry.stream.controllable {
                                entry
                                    .node
                                    .enum_params(0, Some(ParamType::Props), 0, u32::MAX);
                            }
                        }
                        state.dirty = true;
                    }
                })
                .param(move |_, param, _, _, pod| {
                    if param != ParamType::Props {
                        return;
                    }
                    let Some(pod) = pod else { return };
                    let Ok((_, Value::Object(object))) =
                        PodDeserializer::deserialize_any_from(pod.as_bytes())
                    else {
                        return;
                    };
                    if let Some(state) = param_state.upgrade() {
                        let mut state = state.borrow_mut();
                        if let Some(entry) = state.nodes.get_mut(&id) {
                            entry.props_ready = true;
                            for prop in object.properties {
                                match (prop.key, prop.value) {
                                    (spa::sys::SPA_PROP_mute, Value::Bool(value)) => {
                                        entry.stream.muted = value
                                    }
                                    (spa::sys::SPA_PROP_volume, Value::Float(value)) => {
                                        entry.stream.volume = value.max(0.).cbrt()
                                    }
                                    (
                                        spa::sys::SPA_PROP_channelVolumes,
                                        Value::ValueArray(ValueArray::Float(channels)),
                                    ) => {
                                        entry.stream.volume = channels
                                            .iter()
                                            .copied()
                                            .fold(0., f32::max)
                                            .max(0.)
                                            .cbrt();
                                        entry.channels = channels;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        state.dirty = true;
                    }
                })
                .register();
            node.subscribe_params(&[ParamType::Props]);
            node.enum_params(0, Some(ParamType::Props), 0, u32::MAX);
            let mut state = state.borrow_mut();
            state.nodes.insert(
                id,
                Entry {
                    node,
                    _listener: listener,
                    stream: AudioStream {
                        id,
                        serial: props
                            .get("object.serial")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(id as u64),
                        direction,
                        app_key: key,
                        name,
                        process,
                        volume: 1.,
                        muted: false,
                        controllable: false,
                    },
                    channels: vec![],
                    pending_policy: true,
                    props_ready: false,
                },
            );
            state.dirty = true;
        })
        .global_remove(move |id| {
            if let Some(state) = removed_state.upgrade() {
                let mut state = state.borrow_mut();
                state.nodes.remove(&id);
                state.dirty = true;
            }
        })
        .register();
    let shutdown = Rc::new(RefCell::new(false));
    let stop = shutdown.clone();
    let timer_loop = main_loop.downgrade();
    let timer_state = state.clone();
    let commands = commands.clone();
    let updates = updates.clone();
    let timer = main_loop.loop_().add_timer(move |_| {
        let mut state = timer_state.borrow_mut();
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Shutdown => {
                    *stop.borrow_mut() = true;
                    if let Some(loop_) = timer_loop.upgrade() {
                        loop_.quit();
                    }
                }
                Command::AudioVolume {
                    app,
                    direction,
                    volume,
                } => {
                    let volume = volume.clamp(0., 1.5);
                    state
                        .policies
                        .entry((app.clone(), direction))
                        .or_default()
                        .volume = Some(volume);
                    for entry in state
                        .nodes
                        .values_mut()
                        .filter(|e| e.stream.app_key == app && e.stream.direction == direction)
                    {
                        if let Err(e) = apply(
                            entry,
                            &Policy {
                                volume: Some(volume),
                                muted: None,
                            },
                        ) {
                            eprintln!("audio: {e}");
                        }
                    }
                }
                Command::AudioMute {
                    app,
                    direction,
                    muted,
                } => {
                    state
                        .policies
                        .entry((app.clone(), direction))
                        .or_default()
                        .muted = Some(muted);
                    for entry in state
                        .nodes
                        .values_mut()
                        .filter(|e| e.stream.app_key == app && e.stream.direction == direction)
                    {
                        if let Err(e) = apply(
                            entry,
                            &Policy {
                                volume: None,
                                muted: Some(muted),
                            },
                        ) {
                            eprintln!("audio: {e}");
                        }
                    }
                }
                Command::ForgetAudioPolicy { app } => {
                    state.policies.retain(|(key, _), _| key != &app);
                }
                _ => {}
            }
        }
        let policies = state.policies.clone();
        for entry in state.nodes.values_mut() {
            if entry.pending_policy && entry.stream.controllable && entry.props_ready {
                if let Some(policy) =
                    policies.get(&(entry.stream.app_key.clone(), entry.stream.direction))
                {
                    let _ = apply(entry, policy);
                }
                entry.pending_policy = false;
            }
        }
        if state.dirty {
            let update = AudioUpdate {
                streams: state.nodes.values().map(|e| e.stream.clone()).collect(),
                status: "PipeWire · subscribed".into(),
                errors: state.errors.clone(),
            };
            if updates.try_send(update).is_ok() {
                state.dirty = false;
                state.errors.clear();
            }
        }
    });
    timer
        .update_timer(
            Some(Duration::from_millis(10)),
            Some(Duration::from_millis(100)),
        )
        .into_result()?;
    state.borrow_mut().dirty = true;
    main_loop.run();
    Ok(*shutdown.borrow())
}
fn apply(entry: &mut Entry, policy: &Policy) -> anyhow::Result<()> {
    anyhow::ensure!(
        entry.stream.controllable,
        "{} has no writable stream Props",
        entry.stream.name
    );
    let mut properties = vec![];
    if let Some(volume) = policy.volume {
        // PipeWire volumes are linear; UI uses perceptual cubic scaling like PulseAudio/wpctl.
        let linear = volume.powi(3);
        if entry.channels.is_empty() {
            properties.push(Property::new(
                spa::sys::SPA_PROP_volume,
                Value::Float(linear),
            ));
        } else {
            let peak = entry.channels.iter().copied().fold(0., f32::max);
            let channels = entry
                .channels
                .iter()
                .map(|v| {
                    if peak > 0. {
                        *v / peak * linear
                    } else {
                        linear
                    }
                })
                .collect();
            properties.push(Property::new(
                spa::sys::SPA_PROP_channelVolumes,
                Value::ValueArray(ValueArray::Float(channels)),
            ));
        }
    }
    if let Some(muted) = policy.muted {
        properties.push(Property::new(spa::sys::SPA_PROP_mute, Value::Bool(muted)));
    }
    if properties.is_empty() {
        return Ok(());
    }
    let value = Value::Object(Object {
        type_: spa::utils::SpaTypes::ObjectParamProps.as_raw(),
        id: ParamType::Props.as_raw(),
        properties,
    });
    let (cursor, _) = PodSerializer::serialize(Cursor::new(Vec::new()), &value)?;
    let bytes = cursor.into_inner();
    let pod = Pod::from_bytes(&bytes).ok_or_else(|| anyhow::anyhow!("Invalid SPA Props"))?;
    entry.node.set_param(ParamType::Props, 0, pod);
    Ok(())
}
