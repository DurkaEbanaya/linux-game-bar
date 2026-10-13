use crate::{backend::Backend, ipc::Control, render::Gpu, ui::Dashboard};
use crossbeam_channel::{Receiver, bounded};
use gamebar_core::ActiveWindow;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::{
    os::fd::AsRawFd,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};

struct Overlay {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shell: LayerShell,
    gpu: Option<Gpu>,
    layer: Option<LayerSurface>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    ctx: egui::Context,
    input: egui::RawInput,
    dashboard: Dashboard,
    size: [u32; 2],
    scale: i32,
    configured: bool,
    dirty: bool,
    quit: bool,
    start: Instant,
    modifiers: egui::Modifiers,
}
pub fn run(show: bool) -> anyhow::Result<()> {
    let (control_tx, control_rx) = bounded(32);
    let _listener = crate::ipc::listen(control_tx.clone())?;
    let active_rx = crate::desktop::start(control_tx);
    let backend = Backend::start();
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init(&conn)?;
    let qh = queue.handle();
    let mut app = Overlay {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh)?,
        shell: LayerShell::bind(&globals, &qh)?,
        gpu: None,
        layer: None,
        keyboard: None,
        pointer: None,
        ctx: egui::Context::default(),
        input: egui::RawInput::default(),
        dashboard: Dashboard::new(backend.commands.clone()),
        size: [1280, 720],
        scale: 1,
        configured: false,
        dirty: false,
        quit: false,
        start: Instant::now(),
        modifiers: egui::Modifiers::default(),
    };
    queue.roundtrip(&mut app)?;
    if show {
        app.show(&conn, &qh, &active_rx)?;
    }
    let mut repaint_at = Instant::now() + Duration::from_secs(86400);
    while !app.quit {
        queue.dispatch_pending(&mut app)?;
        // Active-window publication precedes the KWin toggle message on the same D-Bus bridge.
        if app.layer.is_none() {
            while let Ok(active) = active_rx.try_recv() {
                app.dashboard.active = active;
            }
        }
        while let Ok(control) = control_rx.try_recv() {
            match control {
                Control::Quit => app.quit = true,
                Control::Hide => app.hide(),
                Control::Show if app.layer.is_none() => app.show(&conn, &qh, &active_rx)?,
                Control::Toggle => {
                    if app.layer.is_some() {
                        app.hide()
                    } else {
                        app.show(&conn, &qh, &active_rx)?
                    }
                }
                _ => {}
            }
        }
        while let Ok(snapshot) = backend.snapshots.try_recv() {
            if snapshot.revision != app.dashboard.snapshot.revision {
                app.dashboard.update(snapshot);
                app.dirty = true;
            }
        }
        if app.dashboard.hide {
            app.hide();
            app.dashboard.hide = false;
        }
        if app.layer.is_some() && app.configured && (app.dirty || Instant::now() >= repaint_at) {
            app.input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(app.size[0] as f32, app.size[1] as f32),
            ));
            app.input.time = Some(app.start.elapsed().as_secs_f64());
            app.input
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .native_pixels_per_point = Some(app.scale as f32);
            let output = app
                .ctx
                .run_ui(app.input.take(), |root| app.dashboard.show(root));
            let delay = output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|v| v.repaint_delay)
                .unwrap_or(Duration::MAX);
            repaint_at = Instant::now() + delay.min(Duration::from_secs(86400));
            if let Some(gpu) = &mut app.gpu {
                if let Err(e) = gpu.draw(
                    &app.ctx,
                    output,
                    [
                        app.size[0] * app.scale as u32,
                        app.size[1] * app.scale as u32,
                    ],
                ) {
                    eprintln!("Render: {e}");
                    app.hide();
                }
            }
            app.dirty = false;
        }
        conn.flush()?;
        if let Some(guard) = conn.prepare_read() {
            let mut fd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let timeout = if app.layer.is_some() {
                repaint_at
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(50) as i32
            } else {
                100
            };
            let ready = unsafe { libc::poll(&mut fd, 1, timeout) };
            if ready > 0 {
                guard.read()?;
            } else {
                drop(guard);
            }
        }
    }
    app.hide();
    backend.shutdown();
    let _ = std::fs::remove_file(crate::ipc::runtime_dir().join("control.sock"));
    Ok(())
}
impl Overlay {
    fn show(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        active_rx: &Receiver<ActiveWindow>,
    ) -> anyhow::Result<()> {
        while let Ok(active) = active_rx.try_recv() {
            self.dashboard.active = active;
        }
        self.dashboard
            .select_active(crate::desktop::query_active(&self.dashboard.active));
        let output = self.outputs.outputs().find(|output| {
            self.outputs
                .info(output)
                .is_some_and(|i| i.name.as_ref() == self.dashboard.active.output.as_ref())
        });
        let surface = self.compositor.create_surface(qh);
        let layer = self.shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("linux-game-bar"),
            output.as_ref(),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        self.ctx = egui::Context::default();
        silver_ui_kit::typography::install(&self.ctx);
        self.gpu = Some(Gpu::new(conn, layer.wl_surface())?);
        self.scale = 1;
        self.configured = false;
        self.dirty = true;
        self.input = egui::RawInput::default();
        layer.commit();
        self.layer = Some(layer);
        Ok(())
    }
    fn hide(&mut self) {
        self.gpu.take();
        self.layer.take();
        self.configured = false;
        self.input = egui::RawInput::default();
    }
    fn key(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        if self.layer.is_none() {
            return;
        }
        if pressed
            && (event.keysym == Keysym::Escape
                || (event.keysym == Keysym::g && self.modifiers.mac_cmd))
        {
            self.dashboard.hide = true;
            return;
        }
        let key = match event.keysym {
            Keysym::Tab => Some(egui::Key::Tab),
            Keysym::Return => Some(egui::Key::Enter),
            Keysym::BackSpace => Some(egui::Key::Backspace),
            Keysym::Delete => Some(egui::Key::Delete),
            Keysym::Left => Some(egui::Key::ArrowLeft),
            Keysym::Right => Some(egui::Key::ArrowRight),
            Keysym::Up => Some(egui::Key::ArrowUp),
            Keysym::Down => Some(egui::Key::ArrowDown),
            Keysym::Home => Some(egui::Key::Home),
            Keysym::End => Some(egui::Key::End),
            _ => None,
        };
        if let Some(key) = key {
            self.input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat,
                modifiers: self.modifiers,
            });
        }
        if pressed && !self.modifiers.ctrl && !self.modifiers.mac_cmd {
            if let Some(text) = event.utf8 {
                if !text.chars().any(char::is_control) {
                    self.input.events.push(egui::Event::Text(text));
                }
            }
        }
        self.dirty = true;
    }
}
impl CompositorHandler for Overlay {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        self.scale = factor.max(1);
        surface.set_buffer_scale(self.scale);
        self.dirty = true;
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.dirty = true;
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl LayerShellHandler for Overlay {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.dashboard.hide = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        c: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.size = [c.new_size.0.max(1), c.new_size.1.max(1)];
        self.configured = true;
        self.dirty = true;
    }
}
impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seats.get_keyboard(qh, &seat, None).ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if cap == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl KeyboardHandler for Overlay {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.input.focused = true;
        self.dirty = true;
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.input.focused = false;
        self.modifiers = Default::default();
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, true, false);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, true, true);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(event, false, false);
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        m: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = egui::Modifiers {
            alt: m.alt,
            ctrl: m.ctrl,
            shift: m.shift,
            mac_cmd: m.logo,
            command: m.ctrl,
        };
        self.input
            .events
            .push(egui::Event::ModifiersChanged(self.modifiers));
        self.dirty = true;
    }
}
impl PointerHandler for Overlay {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if !self
                .layer
                .as_ref()
                .is_some_and(|l| l.wl_surface() == &event.surface)
            {
                continue;
            }
            let pos = egui::pos2(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.input.events.push(egui::Event::PointerMoved(pos))
                }
                PointerEventKind::Leave { .. } => self.input.events.push(egui::Event::PointerGone),
                PointerEventKind::Press { button, .. }
                | PointerEventKind::Release { button, .. } => {
                    let button = match button {
                        0x110 => Some(egui::PointerButton::Primary),
                        0x111 => Some(egui::PointerButton::Secondary),
                        0x112 => Some(egui::PointerButton::Middle),
                        _ => None,
                    };
                    if let Some(button) = button {
                        self.input.events.push(egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: matches!(event.kind, PointerEventKind::Press { .. }),
                            modifiers: self.modifiers,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => self.input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-horizontal.absolute as f32, -vertical.absolute as f32),
                    modifiers: self.modifiers,
                    phase: egui::TouchPhase::Move,
                }),
            }
            self.dirty = true;
        }
    }
}
delegate_registry!(Overlay);
impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
smithay_client_toolkit::delegate_dispatch2!(Overlay);
