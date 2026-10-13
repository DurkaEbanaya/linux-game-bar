use crossbeam_channel::Sender;
use egui::{Color32, RichText, Ui};
use gamebar_core::{
    ActiveWindow, Application, Command, Direction, ProcessId, Snapshot, Sort, Theme,
};
use serde::{Deserialize, Serialize};
use silver_ui_kit::{buttons::icon_button, icons::Glyph, theme::Tokens};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Widget {
    Fps,
    Average,
    Low1,
    Low01,
    FrameTime,
    AppCpu,
    AppGpu,
    AppRam,
    SystemCpu,
    SystemGpu,
    SystemRam,
    Temperature,
    Vram,
}
impl Widget {
    fn title(self) -> &'static str {
        match self {
            Self::Fps => "PRESENT FPS",
            Self::Average => "AVG FPS · 30 s",
            Self::Low1 => "1% LOW",
            Self::Low01 => "0,1% LOW",
            Self::FrameTime => "FRAMETIME · present",
            Self::AppCpu => "CPU приложения",
            Self::AppGpu => "GPU приложения",
            Self::AppRam => "RAM приложения",
            Self::SystemCpu => "CPU системы",
            Self::SystemGpu => "GPU системы",
            Self::SystemRam => "RAM системы",
            Self::Temperature => "Температуры системы",
            Self::Vram => "VRAM системы",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct WidgetLayout {
    widget: Widget,
    visible: bool,
    position: [f32; 2],
    size: [f32; 2],
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    theme: Theme,
    zoom: f32,
    hud: bool,
    opacity: f32,
    widgets: Vec<WidgetLayout>,
}
impl Default for Settings {
    fn default() -> Self {
        let widgets = [
            Widget::Fps,
            Widget::Average,
            Widget::Low1,
            Widget::Low01,
            Widget::AppCpu,
            Widget::AppGpu,
            Widget::AppRam,
            Widget::SystemCpu,
            Widget::SystemGpu,
            Widget::SystemRam,
            Widget::Temperature,
            Widget::Vram,
        ];
        let mut layouts: Vec<_> = widgets
            .iter()
            .enumerate()
            .map(|(i, widget)| WidgetLayout {
                widget: *widget,
                visible: i < 7,
                position: [24. + (i % 4) as f32 * 195., 24. + (i / 4) as f32 * 100.],
                size: [175., 65.],
            })
            .collect();
        layouts.push(WidgetLayout {
            widget: Widget::FrameTime,
            visible: true,
            position: [24., 235.],
            size: [740., 190.],
        });
        Self {
            theme: Theme::default(),
            zoom: 1.,
            hud: true,
            opacity: 0.45,
            widgets: layouts,
        }
    }
}

pub struct Dashboard {
    pub snapshot: Snapshot,
    pub selected: Option<ProcessId>,
    pub active: ActiveWindow,
    pub zoom: f32,
    settings: Settings,
    tab: usize,
    sort: Sort,
    hot_sort: Sort,
    filter: String,
    pub hide: bool,
    pub suspend_hud: bool,
    commands: Sender<Command>,
    settings_dirty: Option<Instant>,
}
impl Dashboard {
    pub fn new(commands: Sender<Command>) -> Self {
        let settings = std::fs::read(settings_path())
            .ok()
            .filter(|b| b.len() <= 65536)
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .filter(|s| {
                s.widgets.len() <= 32
                    && s.zoom.is_finite()
                    && (0.75..=1.5).contains(&s.zoom)
                    && s.opacity.is_finite()
                    && (0.1..=0.9).contains(&s.opacity)
                    && s.widgets.iter().all(|w| {
                        w.position
                            .iter()
                            .chain(w.size.iter())
                            .all(|v| v.is_finite() && v.abs() < 20000.)
                    })
            })
            .unwrap_or_default();
        Self {
            snapshot: Snapshot::default(),
            selected: None,
            active: ActiveWindow::default(),
            zoom: settings.zoom,
            settings,
            tab: 0,
            sort: Sort::Cpu,
            hot_sort: Sort::Cpu,
            filter: String::new(),
            hide: false,
            suspend_hud: false,
            commands,
            settings_dirty: None,
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
                self.selected = Some(id);
                self.send(Command::SelectProcess(Some(id)));
            }
        }
        self.active = active;
    }
    pub fn update(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
    }
    pub fn hud_enabled(&self) -> bool {
        self.settings.hud && self.settings.widgets.iter().any(|w| w.visible)
    }
    pub fn save(&mut self, force: bool) {
        if self.settings_dirty.is_none()
            || (!force
                && self
                    .settings_dirty
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(1)))
        {
            return;
        }
        self.settings.zoom = self.zoom;
        let path = settings_path();
        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(path.parent().unwrap())?;
            let temporary = path.with_extension("json.tmp");
            std::fs::write(&temporary, serde_json::to_vec_pretty(&self.settings)?)?;
            std::fs::rename(temporary, path)?;
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("Settings: {e}");
        }
        self.settings_dirty = None;
    }
    fn tokens(&self, hud: bool) -> Tokens {
        if hud {
            return Tokens::pro_dark();
        }
        match self.settings.theme {
            Theme::ProDark => Tokens::pro_dark(),
            Theme::StudioLight => Tokens::studio_light(),
            Theme::ModernDark => Tokens::modern_dark(),
        }
    }
    pub fn show(&mut self, root: &mut Ui, hud: bool) {
        let ctx = root.ctx().clone();
        let t = self.tokens(hud);
        t.apply(&ctx);
        let rect = root.max_rect();
        if hud {
            self.widgets(&ctx, rect, &t, true);
            return;
        }
        root.painter()
            .rect_filled(rect, 0., Color32::from_black_alpha(100));
        let before = serde_json::to_vec(&self.settings).unwrap_or_default();
        let old_zoom = self.zoom;
        egui::Window::new("Linux Game Bar").id(egui::Id::new("dashboard"))
            .collapsible(false).default_pos(egui::pos2((rect.width() - 510.).max(20.), 25.))
            .default_size(egui::vec2(480., 640.)).min_width(430.)
            .scroll([false, true]).show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, title) in ["Производительность", "Ресурсы"].iter().enumerate() {
                        ui.selectable_value(&mut self.tab, i, *title);
                    }
                    if icon_button(ui, &t, Glyph::Close, false, "Вернуться в игру · Esc").clicked() { self.hide = true; }
                });
                ui.separator();
                ui.menu_button("Виджеты и HUD…", |ui| {
                    ui.checkbox(&mut self.settings.hud, "Оставлять HUD поверх игры");
                    ui.add(egui::Slider::new(&mut self.settings.opacity, 0.1..=0.9).text("Фон HUD"));
                    ui.add(egui::Slider::new(&mut self.zoom, 0.75..=1.5).step_by(0.05).text("Масштаб"));
                    egui::ComboBox::from_id_salt("theme").selected_text(t.theme_name).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings.theme, Theme::ProDark, "Pro Dark");
                        ui.selectable_value(&mut self.settings.theme, Theme::StudioLight, "Studio Light");
                        ui.selectable_value(&mut self.settings.theme, Theme::ModernDark, "Modern Dark");
                    });
                    ui.separator();
                    for layout in &mut self.settings.widgets { ui.checkbox(&mut layout.visible, layout.widget.title()); }
                    if ui.button("Сбросить расположение").clicked() {
                        self.settings.widgets = Settings::default().widgets;
                        ctx.memory_mut(|m| *m = egui::Memory::default());
                    }
                });
                ui.label(RichText::new("Перетаскивайте метрики за заголовок; × скрывает виджет. Esc / Win+G оставляет только HUD, без захвата ввода.").small().color(t.muted));
                ui.separator();
                if self.tab == 0 { self.performance_controls(ui, &t); } else { self.resources(ui, &t); }
                if let Some(error) = self.snapshot.errors.last() { ui.separator(); ui.colored_label(t.warning, error); }
            });
        if self.tab == 0 {
            self.widgets(&ctx, rect, &t, false);
        }
        if old_zoom != self.zoom || before != serde_json::to_vec(&self.settings).unwrap_or_default()
        {
            self.settings_dirty = Some(Instant::now());
        }
    }
    fn widgets(&mut self, ctx: &egui::Context, screen: egui::Rect, t: &Tokens, hud: bool) {
        for index in 0..self.settings.widgets.len() {
            let layout = self.settings.widgets[index].clone();
            if !layout.visible {
                continue;
            }
            let id = egui::Id::new(("metric-widget", index));
            let pos = egui::pos2(layout.position[0], layout.position[1]);
            if hud {
                egui::Area::new(id.with("hud"))
                    .fixed_pos(pos)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_black_alpha(
                                (self.settings.opacity * 255.) as u8,
                            ))
                            .corner_radius(5)
                            .inner_margin(8.)
                            .show(ui, |ui| {
                                ui.set_min_width(layout.size[0].max(140.) - 16.);
                                ui.set_max_width(layout.size[0].max(140.) - 16.);
                                ui.label(
                                    RichText::new(layout.widget.title())
                                        .small()
                                        .color(Color32::from_gray(200)),
                                );
                                self.widget_content(ui, layout.widget, t, true, layout.size[1]);
                            });
                    });
            } else {
                let mut open = true;
                let response = egui::Window::new(layout.widget.title())
                    .id(id)
                    .open(&mut open)
                    .collapsible(false)
                    .default_pos(pos)
                    .default_size(egui::vec2(layout.size[0], layout.size[1]))
                    .min_size(egui::vec2(140., 55.))
                    .constrain_to(screen)
                    .show(ctx, |ui| {
                        self.widget_content(ui, layout.widget, t, false, layout.size[1])
                    });
                let saved = &mut self.settings.widgets[index];
                saved.visible = open;
                if let Some(response) = response {
                    saved.position = [response.response.rect.min.x, response.response.rect.min.y];
                    saved.size = [
                        response.response.rect.width(),
                        response.response.rect.height().max(55.),
                    ];
                }
            }
        }
    }
    fn widget_content(&self, ui: &mut Ui, widget: Widget, t: &Tokens, hud: bool, height: f32) {
        let fps = (self.snapshot.fps.process == self.selected).then_some(&self.snapshot.fps);
        let app = self.selected.and_then(|id| {
            self.snapshot
                .applications
                .iter()
                .find(|a| a.processes.iter().any(|p| p.id == id))
        });
        let s = &self.snapshot.system;
        if matches!(widget, Widget::FrameTime) {
            ui.label(format!(
                "Среднее {} · p95 {}",
                number(fps.and_then(|f| f.frame_ms), " ms"),
                number(fps.and_then(|f| f.p95_ms), " ms")
            ));
            let graph_height = if hud {
                height - 60.
            } else {
                ui.available_height()
            };
            frame_plot(
                ui,
                fps.map(|f| f.frame_times_ms.as_slice()).unwrap_or(&[]),
                graph_height.clamp(70., 400.),
                t,
            );
            return;
        }
        let value = match widget {
            Widget::Fps => number(fps.and_then(|f| f.fps), ""),
            Widget::Average => number(fps.and_then(|f| f.avg_fps), ""),
            Widget::Low1 => number(fps.and_then(|f| f.low_1_fps), ""),
            Widget::Low01 => number(fps.and_then(|f| f.low_01_fps), ""),
            Widget::AppCpu => number(app.and_then(|a| a.cpu_percent), "%"),
            Widget::AppGpu => number(app.and_then(|a| a.gpu_percent), "%"),
            Widget::AppRam => app
                .map(|a| bytes(a.memory_bytes))
                .unwrap_or_else(|| "—".into()),
            Widget::SystemCpu => number(s.cpu_percent, "%"),
            Widget::SystemGpu => number(s.gpu_percent, "%"),
            Widget::SystemRam => format!("{} / {}", bytes(s.memory_used), bytes(s.memory_total)),
            Widget::Temperature => format!(
                "CPU {} · GPU {}",
                number(s.cpu_temperature, "°C"),
                number(s.gpu_temperature, "°C")
            ),
            Widget::Vram => format!(
                "{} / {}",
                s.vram_used.map(bytes).unwrap_or_else(|| "—".into()),
                s.vram_total.map(bytes).unwrap_or_else(|| "—".into())
            ),
            Widget::FrameTime => unreachable!(),
        };
        ui.label(
            RichText::new(value)
                .size(if hud { 22. } else { 25. })
                .strong(),
        );
    }
    fn performance_controls(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.label(
            RichText::new(if self.active.title.is_empty() {
                "Откройте Win+G из игры"
            } else {
                &self.active.title
            })
            .strong(),
        );
        let helper = crate::fps::socket_path().exists();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new(if helper {
                        "Переподключить FPS"
                    } else {
                        "Включить FPS"
                    }),
                )
                .clicked()
            {
                if let Some(id) = self.selected {
                    self.send(if helper {
                        Command::TraceFps(id)
                    } else {
                        Command::AuthorizeFps(id)
                    });
                    if !helper {
                        self.suspend_hud = true;
                        self.hide = true;
                    }
                }
            }
            if ui.button("Отключить FPS").clicked() {
                self.send(Command::StopFps);
            }
        });
        ui.label(
            RichText::new(&self.snapshot.fps.status)
                .small()
                .color(t.muted),
        );
        ui.label(
            RichText::new(format!(
                "Статистика {:.1} s · {} интервалов · lost {}",
                self.snapshot.fps.statistics_seconds,
                self.snapshot.fps.statistics_frames,
                self.snapshot.fps.lost
            ))
            .small(),
        );
        ui.label(RichText::new("AVG / lows: последние 30 s. 1% и 0,1% low = 1000 / среднее самых медленных интервалов. Нужны ≥100 / ≥1000 кадров. Frametime — расстояние между успешными present, не GPU render time.").small().color(t.muted));
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Интенсивные приложения");
            ui.selectable_value(&mut self.hot_sort, Sort::Cpu, "CPU");
            ui.selectable_value(&mut self.hot_sort, Sort::Gpu, "GPU");
        });
        ui.label(
            RichText::new(&self.snapshot.audio_status)
                .small()
                .color(t.muted),
        );
        let mut apps = self.snapshot.applications.clone();
        apps.sort_by(|a, b| sort_value(b, self.hot_sort).total_cmp(&sort_value(a, self.hot_sort)));
        let hot: BTreeSet<_> = apps.iter().take(8).map(|a| a.key.clone()).collect();
        apps.retain(|a| {
            hot.contains(&a.key)
                || self.snapshot.streams.iter().any(|s| {
                    s.app_key == a.key
                        || s.process
                            .is_some_and(|id| a.processes.iter().any(|p| p.id == id))
                })
        });
        let mut covered = BTreeSet::new();
        for app in apps.iter().take(24) {
            ui.push_id(&app.key, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(&app.name);
                    ui.label(format!(
                        "CPU {} · GPU {}",
                        number(app.cpu_percent, "%"),
                        number(app.gpu_percent, "%")
                    ));
                });
                let keys = self.audio_keys(app);
                covered.extend(keys.iter().cloned());
                self.audio_controls(ui, &keys);
                ui.separator();
            });
        }
        // Include streams whose process is outside the current /proc snapshot.
        let remaining: BTreeSet<_> = self
            .snapshot
            .streams
            .iter()
            .map(|s| s.app_key.clone())
            .filter(|key| !covered.contains(key))
            .collect();
        for key in remaining {
            ui.push_id(&key, |ui| {
                if let Some(stream) = self.snapshot.streams.iter().find(|s| s.app_key == key) {
                    ui.strong(&stream.name);
                }
                self.audio_controls(ui, &[key.clone()].into_iter().collect());
            });
        }
    }
    fn audio_keys(&self, app: &Application) -> BTreeSet<String> {
        self.snapshot
            .streams
            .iter()
            .filter(|s| {
                s.app_key == app.key
                    || s.process
                        .is_some_and(|id| app.processes.iter().any(|p| p.id == id))
            })
            .map(|s| s.app_key.clone())
            .collect()
    }
    fn audio_controls(&self, ui: &mut Ui, keys: &BTreeSet<String>) {
        let mut any = false;
        for (direction, label) in [
            (Direction::Playback, "Звук"),
            (Direction::Capture, "Микрофон"),
        ] {
            let streams: Vec<_> = self
                .snapshot
                .streams
                .iter()
                .filter(|s| keys.contains(&s.app_key) && s.direction == direction)
                .collect();
            if streams.is_empty() {
                continue;
            }
            any = true;
            let enabled = streams.iter().all(|s| s.controllable);
            let muted = streams.iter().all(|s| s.muted);
            let mut volume = streams.iter().map(|s| s.volume).fold(0., f32::max);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(format!(
                            "{label}: {}",
                            if muted { "MUTE" } else { "ON" }
                        )),
                    )
                    .on_hover_text("Отключить/включить только потоки этого приложения")
                    .clicked()
                {
                    for key in streams
                        .iter()
                        .map(|s| s.app_key.clone())
                        .collect::<BTreeSet<_>>()
                    {
                        self.send(Command::AudioMute {
                            app: key,
                            direction,
                            muted: !muted,
                        });
                    }
                }
                ui.scope(|ui| {
                    ui.spacing_mut().slider_width = 105.;
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Slider::new(&mut volume, 0.0..=1.5).show_value(false),
                        )
                        .changed()
                    {
                        for key in streams
                            .iter()
                            .map(|s| s.app_key.clone())
                            .collect::<BTreeSet<_>>()
                        {
                            self.send(Command::AudioVolume {
                                app: key,
                                direction,
                                volume,
                            });
                        }
                    }
                });
                ui.label(format!("{:.0}%", volume * 100.));
                if !enabled {
                    ui.small("нет writable Props");
                }
            });
        }
        if !any {
            ui.label(RichText::new("Нет активного аудиопотока").small().weak());
        }
    }
    fn resources(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.set_min_width(1000.);
        ui.horizontal_wrapped(|ui| {
            ui.label("Поиск");
            ui.text_edit_singleline(&mut self.filter);
            for (sort, title) in [
                (Sort::Cpu, "CPU"),
                (Sort::Memory, "RAM"),
                (Sort::Gpu, "GPU"),
                (Sort::Io, "I/O"),
            ] {
                ui.selectable_value(&mut self.sort, sort, title);
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
        apps.sort_by(|a, b| sort_value(b, self.sort).total_cmp(&sort_value(a, self.sort)));
        egui::ScrollArea::horizontal().show(ui, |ui| {
            egui::Grid::new("resources")
                .striped(true)
                .min_col_width(65.)
                .show(ui, |ui| {
                    for title in [
                        "Приложение",
                        "CPU",
                        "RAM",
                        "GPU",
                        "VRAM",
                        "I/O",
                        "Звук / микрофон",
                        "Действия",
                    ] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for app in apps {
                        ui.label(&app.name).on_hover_text(
                            app.processes
                                .iter()
                                .map(|p| format!("PID {} · {}", p.id.pid, p.executable))
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );
                        ui.label(number(app.cpu_percent, "%"));
                        ui.label(bytes(app.memory_bytes));
                        ui.label(number(app.gpu_percent, "%"));
                        ui.label(app.vram_bytes.map(bytes).unwrap_or_else(|| "—".into()));
                        ui.label(
                            app.io_bytes_per_sec
                                .map(|v| format!("{}/s", bytes(v as u64)))
                                .unwrap_or_else(|| "—".into()),
                        );
                        ui.push_id((&app.key, "audio"), |ui| {
                            ui.vertical(|ui| self.audio_controls(ui, &self.audio_keys(&app)));
                        });
                        ui.add_enabled_ui(app.processes.iter().all(|p| p.owned), |ui| {
                            ui.menu_button("Завершить…", |ui| {
                                ui.colored_label(
                                    t.warning,
                                    format!("{} · {} процессов", app.name, app.processes.len()),
                                );
                                for (force, title) in [
                                    (false, "SIGTERM · завершить"),
                                    (true, "SIGKILL · принудительно"),
                                ] {
                                    if ui.button(title).clicked() {
                                        self.send(Command::Terminate {
                                            processes: app.processes.iter().map(|p| p.id).collect(),
                                            force,
                                        });
                                        ui.close();
                                    }
                                }
                            });
                        });
                        ui.end_row();
                    }
                });
        });
    }
}
fn settings_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("linux-game-bar/layout.json")
}
fn sort_value(a: &Application, sort: Sort) -> f64 {
    match sort {
        Sort::Cpu => a.cpu_percent.unwrap_or(-1.),
        Sort::Gpu => a.gpu_percent.unwrap_or(-1.),
        Sort::Memory => a.memory_bytes as f64,
        Sort::Io => a.io_bytes_per_sec.unwrap_or(-1.),
    }
}
fn frame_plot(ui: &mut Ui, values: &[f64], height: f32, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(120.), height),
        egui::Sense::hover(),
    );
    let plot = rect.shrink2(egui::vec2(6., 15.));
    let max = values.iter().copied().fold(16.67, f64::max) * 1.1;
    for interval in [8.33, 16.67, 33.33] {
        if interval >= max {
            continue;
        }
        let y = plot.bottom() - (interval / max) as f32 * plot.height();
        ui.painter().line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(0.5, t.muted),
        );
        ui.painter().text(
            egui::pos2(plot.left() + 2., y),
            egui::Align2::LEFT_BOTTOM,
            format!("{interval:.1} ms"),
            egui::FontId::proportional(10.),
            t.muted,
        );
    }
    if values.is_empty() {
        ui.painter().text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "Нет present-интервалов",
            egui::FontId::proportional(12.),
            t.muted,
        );
        return;
    }
    // Preserve spikes with a per-pixel min/max envelope rather than averaging samples away.
    let bins = (plot.width() as usize).max(1).min(values.len());
    let mut previous = None;
    for i in 0..bins {
        let slice = &values[i * values.len() / bins..(i + 1) * values.len() / bins];
        let low = slice.iter().copied().fold(f64::INFINITY, f64::min);
        let high = slice.iter().copied().fold(0., f64::max);
        let x = plot.left() + i as f32 / bins.saturating_sub(1).max(1) as f32 * plot.width();
        let point =
            |value: f64| egui::pos2(x, plot.bottom() - (value / max) as f32 * plot.height());
        ui.painter()
            .line_segment([point(low), point(high)], egui::Stroke::new(1., t.accent));
        let last = point(*slice.last().unwrap());
        if let Some(prev) = previous {
            ui.painter()
                .line_segment([prev, last], egui::Stroke::new(1., t.accent));
        }
        previous = Some(last);
    }
    ui.painter().text(
        rect.left_bottom(),
        egui::Align2::LEFT_BOTTOM,
        "старые → новые · каждый present",
        egui::FontId::proportional(10.),
        t.muted,
    );
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
