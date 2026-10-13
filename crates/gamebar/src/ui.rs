use crossbeam_channel::Sender;
use egui::{Color32, RichText, Ui};
use gamebar_core::{ActiveWindow, Command, Direction, ProcessId, Snapshot, Sort, Theme};
use silver_ui_kit::{buttons::icon_button, icons::Glyph, theme::Tokens};
use std::collections::{HashMap, VecDeque};

pub struct Dashboard {
    pub snapshot: Snapshot,
    pub selected: Option<ProcessId>,
    pub active: ActiveWindow,
    theme: Theme,
    pub zoom: f32,
    tab: usize,
    sort: Sort,
    filter: String,
    history: VecDeque<Option<f64>>,
    pub hide: bool,
    commands: Sender<Command>,
}
impl Dashboard {
    pub fn new(commands: Sender<Command>) -> Self {
        Self {
            snapshot: Snapshot::default(),
            selected: None,
            active: ActiveWindow::default(),
            theme: Theme::default(),
            zoom: 1.,
            tab: 0,
            sort: Sort::Cpu,
            filter: String::new(),
            history: VecDeque::new(),
            hide: false,
            commands,
        }
    }
    fn send(&self, command: Command) {
        if let Err(e) = self.commands.try_send(command) {
            eprintln!("backend command: {e}");
        }
    }
    pub fn select_active(&mut self, active: ActiveWindow) {
        if let Some(id) = active.pid.and_then(crate::metrics::process_id) {
            if self.selected != Some(id) {
                self.history.clear();
            }
            self.selected = Some(id);
            self.send(Command::SelectProcess(Some(id)));
        }
        self.active = active;
    }
    pub fn update(&mut self, snapshot: Snapshot) {
        if snapshot.revision != self.snapshot.revision {
            self.history
                .push_back(if snapshot.fps.process == self.selected {
                    snapshot.fps.fps
                } else {
                    None
                });
            if self.history.len() > 90 {
                self.history.pop_front();
            }
            self.snapshot = snapshot;
        }
    }
    pub fn show(&mut self, root: &mut Ui) {
        let ctx = root.ctx().clone();
        let tokens = match self.theme {
            Theme::ProDark => Tokens::pro_dark(),
            Theme::StudioLight => Tokens::studio_light(),
            Theme::ModernDark => Tokens::modern_dark(),
        };
        tokens.apply(&ctx);
        let rect = root.max_rect();
        root.painter()
            .rect_filled(rect, 0., Color32::from_black_alpha(115));
        let width = (rect.width() - 40.).clamp(320., 1140.);
        let height = (rect.height() - 120.).max(260.);
        egui::Window::new("Linux Game Bar")
            .id(egui::Id::new("dashboard"))
            .title_bar(true)
            .collapsible(false)
            .resizable(true)
            .movable(true)
            .scroll([false, true])
            .default_pos(egui::pos2((rect.width() - width) / 2., 50.))
            .default_size(egui::vec2(width, height.min(800.)))
            .min_size(egui::vec2(620., 380.))
            .max_size(egui::vec2(
                (rect.width() - 20.).max(620.),
                (rect.height() - 40.).max(380.),
            ))
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("GAME BAR");
                    ui.label(RichText::new("Linux · Wayland").color(tokens.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, &tokens, Glyph::Close, false, "Скрыть · Esc").clicked()
                        {
                            self.hide = true;
                        }
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(tokens.theme_name)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.theme, Theme::ProDark, "Pro Dark");
                                ui.selectable_value(
                                    &mut self.theme,
                                    Theme::StudioLight,
                                    "Studio Light",
                                );
                                ui.selectable_value(
                                    &mut self.theme,
                                    Theme::ModernDark,
                                    "Modern Dark",
                                );
                            });
                        ui.add(
                            egui::Slider::new(&mut self.zoom, 0.75..=1.5)
                                .step_by(0.05)
                                .text("Масштаб"),
                        );
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    for (i, label) in ["Производительность", "Аудиомикшер", "Ресурсы"]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut self.tab, i, *label);
                    }
                });
                ui.add_space(8.);
                match self.tab {
                    0 => self.performance(ui, &tokens),
                    1 => self.mixer(ui, &tokens),
                    _ => self.resources(ui, &tokens),
                }
                if let Some(error) = self.snapshot.errors.last() {
                    ui.separator();
                    ui.label(RichText::new(error).color(tokens.warning));
                }
            });
    }
    fn process_picker(&mut self, ui: &mut Ui) {
        let previous = self.selected;
        let selected = self.selected.and_then(|id| {
            self.snapshot
                .applications
                .iter()
                .flat_map(|a| &a.processes)
                .find(|p| p.id == id)
        });
        let text = selected
            .map(|p| format!("{} · PID {}", p.name, p.id.pid))
            .unwrap_or_else(|| "Выберите приложение".into());
        egui::ComboBox::from_id_salt("process")
            .width(360.)
            .selected_text(text)
            .show_ui(ui, |ui| {
                for app in &self.snapshot.applications {
                    for p in &app.processes {
                        if ui
                            .selectable_value(
                                &mut self.selected,
                                Some(p.id),
                                format!("{} · PID {}", p.name, p.id.pid),
                            )
                            .changed()
                        {
                            self.send(Command::SelectProcess(self.selected));
                        }
                    }
                }
            });
        if previous != self.selected {
            self.history.clear();
        }
    }
    fn performance(&mut self, ui: &mut Ui, t: &Tokens) {
        let helper_ready = crate::fps::socket_path().exists();
        ui.horizontal(|ui| {
            self.process_picker(ui);
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new(if helper_ready {
                        "Подключить FPS"
                    } else {
                        "Включить FPS"
                    }),
                )
                .clicked()
            {
                if let Some(id) = self.selected {
                    self.send(if helper_ready {
                        Command::TraceFps(id)
                    } else {
                        Command::AuthorizeFps(id)
                    });
                    // Release exclusive layer keyboard focus for the system authorization dialog.
                    if !helper_ready {
                        self.hide = true;
                    }
                }
            }
            if ui.button("Отключить").clicked() {
                self.send(Command::StopFps);
            }
        });
        if !helper_ready {
            ui.label(RichText::new("FPS требует отдельного helper: «Включить FPS» откроет системную авторизацию и подключится к выбранной игре.").color(t.warning));
        }
        let app = self.selected.and_then(|id| {
            self.snapshot
                .applications
                .iter()
                .find(|a| a.processes.iter().any(|p| p.id == id))
        });
        ui.add_space(12.);
        ui.columns(4, |columns| {
            metric(
                &mut columns[0],
                "PRESENT FPS",
                number(
                    if self.snapshot.fps.process == self.selected {
                        self.snapshot.fps.fps
                    } else {
                        None
                    },
                    "",
                ),
                t,
            );
            metric(
                &mut columns[1],
                "CPU приложения",
                number(app.and_then(|a| a.cpu_percent), "%"),
                t,
            );
            metric(
                &mut columns[2],
                "RAM приложения",
                app.map(|a| bytes(a.memory_bytes))
                    .unwrap_or_else(|| "—".into()),
                t,
            );
            metric(
                &mut columns[3],
                "GPU приложения",
                number(app.and_then(|a| a.gpu_percent), "%"),
                t,
            );
        });
        ui.add_space(10.);
        ui.label(format!(
            "{}  {}  {}",
            self.snapshot
                .fps
                .process
                .map(|p| format!("Trace PID {} ·", p.pid))
                .unwrap_or_default(),
            self.snapshot.fps.source,
            self.snapshot.fps.status
        ));
        ui.horizontal(|ui| {
            ui.label(format!(
                "Frame interval: {}",
                number(self.snapshot.fps.frame_ms, " ms")
            ));
            ui.label(format!("p95: {}", number(self.snapshot.fps.p95_ms, " ms")));
            ui.label(format!("Lost: {}", self.snapshot.fps.lost));
        });
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 125.), egui::Sense::hover());
        ui.painter().rect_filled(rect, t.radius, t.bg);
        let max = self.history.iter().flatten().copied().fold(60., f64::max);
        let mut previous = None;
        for (i, value) in self.history.iter().enumerate() {
            if let Some(value) = value {
                let point = egui::pos2(
                    rect.left() + i as f32 / 89. * rect.width(),
                    rect.bottom() - (*value / max) as f32 * (rect.height() - 12.) - 6.,
                );
                if let Some(prev) = previous {
                    ui.painter()
                        .line_segment([prev, point], egui::Stroke::new(2., t.accent));
                }
                ui.painter().circle_filled(point, 1.5, t.accent);
                previous = Some(point);
            } else {
                previous = None;
            }
        }
        ui.add_space(12.);
        ui.heading("Система");
        let s = &self.snapshot.system;
        ui.columns(4, |columns| {
            metric(
                &mut columns[0],
                "CPU · все ядра",
                number(s.cpu_percent, "%"),
                t,
            );
            metric(
                &mut columns[1],
                "RAM",
                format!("{} / {}", bytes(s.memory_used), bytes(s.memory_total)),
                t,
            );
            metric(
                &mut columns[2],
                "GPU · устройство",
                number(s.gpu_percent, "%"),
                t,
            );
            metric(
                &mut columns[3],
                "GPU температура",
                number(s.gpu_temperature, " °C"),
                t,
            );
        });
        ui.label(format!(
            "{} · VRAM {} / {} · CPU {}",
            s.gpu_name.as_deref().unwrap_or("GPU недоступен"),
            s.vram_used.map(bytes).unwrap_or_else(|| "—".into()),
            s.vram_total.map(bytes).unwrap_or_else(|| "—".into()),
            number(s.cpu_temperature, " °C")
        ));
        ui.label(RichText::new("FPS — успешные вызовы present выбранной swapchain; не частота монитора и не число реально показанных кадров. «—» означает отсутствие измерения.").small().color(t.muted));
    }
    fn mixer(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.label(&self.snapshot.audio_status);
        ui.label(RichText::new("Звук и микрофон отключаются отдельно для приложения. Настройки применяются также к его новым потокам.").color(t.muted));
        let mut groups: HashMap<String, Vec<_>> = HashMap::new();
        for stream in &self.snapshot.streams {
            groups
                .entry(stream.app_key.clone())
                .or_default()
                .push(stream.clone());
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by(|a, b| a.1[0].name.cmp(&b.1[0].name));
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 35.)
            .show(ui, |ui| {
                if groups.is_empty() {
                    ui.label("Нет активных аудиопотоков");
                }
                for (key, streams) in groups {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label(RichText::new(&streams[0].name).strong());
                        for (direction, label) in [
                            (Direction::Playback, "Звук"),
                            (Direction::Capture, "Микрофон"),
                        ] {
                            let relevant: Vec<_> = streams
                                .iter()
                                .filter(|s| s.direction == direction)
                                .collect();
                            if relevant.is_empty() {
                                continue;
                            }
                            let muted = relevant.iter().all(|s| s.muted);
                            let enabled = relevant.iter().all(|s| s.controllable);
                            let mut volume = relevant.iter().map(|s| s.volume).fold(0., f32::max);
                            ui.horizontal(|ui| {
                                ui.label(label);
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::Button::new(if muted {
                                            "Включить"
                                        } else {
                                            "Отключить"
                                        }),
                                    )
                                    .clicked()
                                {
                                    self.send(Command::AudioMute {
                                        app: key.clone(),
                                        direction,
                                        muted: !muted,
                                    });
                                }
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::Slider::new(&mut volume, 0.0..=1.5).show_value(false),
                                    )
                                    .changed()
                                {
                                    self.send(Command::AudioVolume {
                                        app: key.clone(),
                                        direction,
                                        volume,
                                    });
                                }
                                ui.label(format!(
                                    "{:.0}% · {} поток(а)",
                                    volume * 100.,
                                    relevant.len()
                                ));
                                if !enabled {
                                    ui.label(RichText::new("нет writable Props").color(t.warning));
                                }
                            });
                        }
                        if ui.small_button("Забыть политику").clicked() {
                            self.send(Command::ForgetAudioPolicy { app: key });
                        }
                    });
                }
            });
    }
    fn resources(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.horizontal(|ui| {
            ui.label("Поиск");
            ui.text_edit_singleline(&mut self.filter);
            for (sort, label) in [
                (Sort::Cpu, "CPU"),
                (Sort::Memory, "RAM"),
                (Sort::Gpu, "GPU"),
                (Sort::Io, "I/O"),
            ] {
                ui.selectable_value(&mut self.sort, sort, label);
            }
            if ui
                .add_enabled(
                    self.active.window_id.is_some(),
                    egui::Button::new("Закрыть исходное окно"),
                )
                .clicked()
            {
                self.send(Command::CloseWindow(self.active.clone()));
            }
        });
        let mut apps: Vec<_> = self
            .snapshot
            .applications
            .iter()
            .filter(|a| a.name.to_lowercase().contains(&self.filter.to_lowercase()))
            .cloned()
            .collect();
        let value = |a: &gamebar_core::Application| match self.sort {
            Sort::Cpu => a.cpu_percent.unwrap_or(-1.),
            Sort::Memory => a.memory_bytes as f64,
            Sort::Gpu => a.gpu_percent.unwrap_or(-1.),
            Sort::Io => a.io_bytes_per_sec.unwrap_or(-1.),
        };
        apps.sort_by(|a, b| value(b).total_cmp(&value(a)));
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("resources")
                .striped(true)
                .min_col_width(80.)
                .show(ui, |ui| {
                    for text in ["Приложение", "CPU", "RAM", "GPU", "VRAM", "I/O", "Действия"]
                    {
                        ui.strong(text);
                    }
                    ui.end_row();
                    for app in apps {
                        if ui
                            .selectable_label(
                                self.selected
                                    .is_some_and(|id| app.processes.iter().any(|p| p.id == id)),
                                &app.name,
                            )
                            .on_hover_text(
                                app.processes
                                    .iter()
                                    .map(|p| format!("PID {} · {}", p.id.pid, p.executable))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            )
                            .clicked()
                        {
                            self.selected = app.processes.first().map(|p| p.id);
                            self.send(Command::SelectProcess(self.selected));
                        }
                        ui.label(number(app.cpu_percent, "%"));
                        ui.label(bytes(app.memory_bytes));
                        ui.label(number(app.gpu_percent, "%"));
                        ui.label(app.vram_bytes.map(bytes).unwrap_or_else(|| "—".into()));
                        ui.label(
                            app.io_bytes_per_sec
                                .map(|v| format!("{}/s", bytes(v as u64)))
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.add_enabled_ui(app.processes.iter().all(|p| p.owned), |ui| {
                            ui.menu_button("Завершить…", |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} процессов",
                                        app.name,
                                        app.processes.len()
                                    ))
                                    .color(t.warning),
                                );
                                if ui.button("SIGTERM · завершить").clicked() {
                                    self.send(Command::Terminate {
                                        processes: app.processes.iter().map(|p| p.id).collect(),
                                        force: false,
                                    });
                                    ui.close();
                                }
                                if ui.button("SIGKILL · принудительно").clicked() {
                                    self.send(Command::Terminate {
                                        processes: app.processes.iter().map(|p| p.id).collect(),
                                        force: true,
                                    });
                                    ui.close();
                                }
                            });
                        });
                        ui.end_row();
                    }
                });
        });
    }
}
fn metric(ui: &mut Ui, label: &str, value: String, t: &Tokens) {
    egui::Frame::group(ui.style())
        .fill(t.surface_raised)
        .show(ui, |ui| {
            ui.label(RichText::new(label).small().color(t.muted));
            ui.label(RichText::new(value).size(25.).strong());
        });
}
fn number(value: Option<f64>, unit: &str) -> String {
    value
        .map(|v| format!("{v:.1}{unit}"))
        .unwrap_or_else(|| "—".into())
}
fn bytes(value: u64) -> String {
    let v = value as f64;
    if v >= 1073741824. {
        format!("{:.1} GiB", v / 1073741824.)
    } else if v >= 1048576. {
        format!("{:.0} MiB", v / 1048576.)
    } else {
        format!("{:.0} KiB", v / 1024.)
    }
}
