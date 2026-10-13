use crossbeam_channel::Sender;
use egui::{Color32, RichText, Ui};
use gamebar_core::{
    ActiveWindow, Application, Command, Direction, ProcessId, Snapshot, Sort, Theme,
};
use serde::{Deserialize, Serialize};
use silver_ui_kit::{buttons::icon_button, icons::Glyph, theme::Tokens};
use std::{
    collections::{BTreeSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    AppDisk,
    SystemDisk,
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
            Self::AppDisk => "Диск приложения · чтение / запись",
            Self::SystemDisk => "Диски системы · чтение / запись",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct WidgetLayout {
    widget: Widget,
    visible: bool,
    position: [f32; 2],
    size: [f32; 2],
    #[serde(default)]
    opacity: Option<f32>,
    #[serde(default = "default_update_ms")]
    update_ms: u64,
}
fn default_update_ms() -> u64 {
    1000
}
#[derive(Default)]
struct WidgetReading {
    value: String,
    intervals: Vec<(f64, f64)>,
    frame_ms: Option<f64>,
    p95_ms: Option<f64>,
    graph_status: Option<String>,
}
struct MetricDrag {
    index: usize,
    /// Pointer-to-window offset is captured once, so grid rounding never accumulates.
    offset: egui::Vec2,
}
struct MetricResize {
    index: usize,
    pointer: egui::Pos2,
    size: egui::Vec2,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    theme: Theme,
    zoom: f32,
    hud: bool,
    opacity: f32,
    widgets: Vec<WidgetLayout>,
    hud_shortcut: String,
    grid_visible: bool,
    grid_snap: bool,
    grid_size: f32,
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
                opacity: None,
                update_ms: default_update_ms(),
            })
            .collect();
        layouts.push(WidgetLayout {
            widget: Widget::FrameTime,
            visible: true,
            position: [24., 235.],
            size: [740., 190.],
            opacity: None,
            update_ms: default_update_ms(),
        });
        for (widget, y, visible) in [
            (Widget::AppDisk, 455., true),
            (Widget::SystemDisk, 565., true),
        ] {
            layouts.push(WidgetLayout {
                widget,
                visible,
                position: [24., y],
                size: [330., 90.],
                opacity: None,
                update_ms: default_update_ms(),
            });
        }
        Self {
            theme: Theme::default(),
            zoom: 1.,
            hud: true,
            opacity: 0.45,
            widgets: layouts,
            hud_shortcut: "Meta+Shift+G".into(),
            grid_visible: true,
            grid_snap: true,
            grid_size: 16.,
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
    editing_widget: Option<usize>,
    readings: Vec<Option<(Instant, WidgetReading)>>,
    preview: bool,
    editing_global: bool,
    frame_history: VecDeque<(f64, f64)>,
    history_identity: Option<(ProcessId, u64, u64)>,
    window_drag: Option<usize>,
    metric_drag: Option<MetricDrag>,
    metric_resize: Option<MetricResize>,
    positions_pending: BTreeSet<usize>,
    layout_settled: bool,
    pub hud_suppressed: bool,
    zoom_updated: Instant,
    displayed_positions: Vec<Option<egui::Pos2>>,
    movement_updated: Instant,
}
impl Dashboard {
    pub fn new(commands: Sender<Command>) -> Self {
        let mut settings = std::fs::read(settings_path())
            .ok()
            .filter(|b| b.len() <= 65536)
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .filter(|s| {
                s.widgets.len() <= 32
                    && s.zoom.is_finite()
                    && (0.75..=1.5).contains(&s.zoom)
                    && s.grid_size.is_finite()
                    && (8.0..=64.0).contains(&s.grid_size)
                    && s.opacity.is_finite()
                    && (0.1..=0.9).contains(&s.opacity)
                    && s.widgets.iter().all(|w| {
                        (250..=10000).contains(&w.update_ms)
                            && w.opacity
                                .is_none_or(|v| v.is_finite() && (0.0..=1.0).contains(&v))
                            && w.position
                                .iter()
                                .chain(w.size.iter())
                                .all(|v| v.is_finite() && v.abs() < 20000.)
                    })
            })
            .unwrap_or_default();
        // Merge newly added widgets without overwriting the user's existing layout.
        let mut migrated = false;
        for layout in Settings::default().widgets {
            if !settings.widgets.iter().any(|w| w.widget == layout.widget) {
                settings.widgets.push(layout);
                migrated = true;
            }
        }
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
            settings_dirty: migrated.then(Instant::now),
            editing_widget: None,
            readings: Vec::new(),
            preview: false,
            editing_global: false,
            frame_history: VecDeque::new(),
            history_identity: None,
            window_drag: None,
            metric_drag: None,
            metric_resize: None,
            positions_pending: BTreeSet::new(),
            layout_settled: false,
            hud_suppressed: false,
            zoom_updated: Instant::now(),
            displayed_positions: Vec::new(),
            movement_updated: Instant::now(),
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
                self.readings.clear();
                self.frame_history.clear();
                self.send(Command::SelectProcess(Some(id)));
            }
        }
        self.active = active;
    }
    pub fn update(&mut self, snapshot: Snapshot) {
        let fps = &snapshot.fps;
        let identity = fps
            .process
            .zip(fps.swapchain)
            .map(|(p, s)| (p, s, fps.lost));
        if identity != self.history_identity || fps.fps.is_none() {
            self.frame_history.clear();
            self.history_identity = identity;
            self.readings.clear();
        }
        for (timestamp, ms) in fps.frame_timestamps_ns.iter().zip(&fps.frame_times_ms) {
            let time = *timestamp as f64 / 1e9;
            if self
                .frame_history
                .back()
                .is_none_or(|(last, _)| time > *last)
            {
                self.frame_history.push_back((time, *ms));
            }
        }
        let cutoff = monotonic_seconds() - 12.;
        while self.frame_history.front().is_some_and(|(t, _)| *t < cutoff)
            || self.frame_history.len() > 20000
        {
            self.frame_history.pop_front();
        }
        self.snapshot = snapshot;
    }
    pub fn hud_enabled(&self) -> bool {
        self.settings.hud && !self.hud_suppressed && self.settings.widgets.iter().any(|w| w.visible)
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
    pub fn animate_zoom(&mut self) -> bool {
        let now = Instant::now();
        let dt = now
            .duration_since(self.zoom_updated)
            .as_secs_f32()
            .min(0.05);
        self.zoom_updated = now;
        let difference = self.settings.zoom - self.zoom;
        if difference.abs() < 0.0002 {
            self.zoom = self.settings.zoom;
            return false;
        }
        self.zoom += difference * (1. - (-dt * 9.).exp());
        true
    }
    pub fn show(&mut self, root: &mut Ui, hud: bool) {
        let ctx = root.ctx().clone();
        let preview = self.preview && ctx.input(|i| i.pointer.primary_down());
        let t = self.tokens(hud || preview);
        t.apply(&ctx);
        let rect = root.max_rect();
        if hud {
            self.widgets(&ctx, rect, &t, true);
            return;
        }
        if preview {
            self.widgets(&ctx, rect, &t, true);
        } else {
            root.painter()
                .rect_filled(rect, 0., Color32::from_black_alpha(100));
            if self.settings.grid_visible {
                let step = self.settings.grid_size;
                let stroke = egui::Stroke::new(0.5, Color32::from_white_alpha(32));
                for column in 0..=(rect.width() / step) as usize {
                    let x = rect.left() + column as f32 * step;
                    root.painter().line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        stroke,
                    );
                }
                for row in 0..=(rect.height() / step) as usize {
                    let y = rect.top() + row as f32 * step;
                    root.painter().line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        stroke,
                    );
                }
            }
        }
        let before = serde_json::to_vec(&self.settings).unwrap_or_default();
        if !preview {
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
                     ui.label("Сочетание для скрытия/показа только метрик:");
                     ui.text_edit_singleline(&mut self.settings.hud_shortcut);
                     if ui.button("Применить сочетание").clicked() {
                         self.send(Command::SetHudShortcut(self.settings.hud_shortcut.clone()));
                     }
                     ui.small("Пример: Meta+Shift+G. Win+G открывает/закрывает меню.");
                     if ui.button("Общий фон HUD · предпросмотр…").clicked() { self.editing_global = true; self.editing_widget = None; }
                    ui.add(egui::Slider::new(&mut self.settings.zoom, 0.75..=1.5).max_decimals(2).text("Масштаб"));
                    ui.checkbox(&mut self.settings.grid_visible, "Показывать сетку в меню");
                    ui.checkbox(&mut self.settings.grid_snap, "Привязка метрик к сетке");
                    egui::ComboBox::from_id_salt("grid-size").selected_text(format!("Сетка {} px",self.settings.grid_size)).show_ui(ui, |ui| {
                        for size in [8.,16.,24.,32.,48.,64.] { ui.selectable_value(&mut self.settings.grid_size,size,format!("{size:.0} px")); }
                    });
                    if ui.button("Выровнять все метрики").clicked() { self.layout_settled = false; }
                    egui::ComboBox::from_id_salt("theme").selected_text(t.theme_name).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings.theme, Theme::ProDark, "Pro Dark");
                        ui.selectable_value(&mut self.settings.theme, Theme::StudioLight, "Studio Light");
                        ui.selectable_value(&mut self.settings.theme, Theme::ModernDark, "Modern Dark");
                    });
                    ui.separator();
                     for layout in &mut self.settings.widgets {
                         if ui.checkbox(&mut layout.visible, layout.widget.title()).changed() { self.layout_settled = false; }
                     }
                    if ui.button("Сбросить расположение").clicked() {
                         self.settings.widgets = Settings::default().widgets;
                         self.readings.clear();
                         self.positions_pending.extend(0..self.settings.widgets.len());
                         self.layout_settled = false;
                         ctx.memory_mut(|m| *m = egui::Memory::default());
                    }
                });
                ui.label(RichText::new("Перетаскивайте метрики за заголовок; × скрывает виджет. Esc / Win+G оставляет только HUD, без захвата ввода.").small().color(t.muted));
                ui.separator();
                if self.tab == 0 { self.performance_controls(ui, &t); } else { self.resources(ui, &t); }
                if let Some(error) = self.snapshot.errors.last() { ui.separator(); ui.colored_label(t.warning, error); }
            });
            self.widgets(&ctx, rect, &t, false);
        }
        self.preview = false;
        if let Some(index) = self.editing_widget {
            if let Some(layout) = self.settings.widgets.get_mut(index) {
                let mut open = true;
                egui::Window::new(format!("Настройка · {}", layout.widget.title()))
                    .id(egui::Id::new("widget-settings")).open(&mut open).collapsible(false)
                    .resizable(false).show(&ctx, |ui| {
                        let mut opacity = layout.opacity.unwrap_or(self.settings.opacity);
                        let slider = ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Непрозрачность в игре"));
                        self.preview = slider.is_pointer_button_down_on();
                        if slider.changed() { layout.opacity = Some(opacity); }
                        egui::ComboBox::from_id_salt("widget-refresh")
                            .selected_text(format!("{} ms", layout.update_ms)).show_ui(ui, |ui| {
                                for (ms, text) in [(250, "250 ms · 4 Hz"), (500, "500 ms · 2 Hz"), (1000, "1 s"), (2000, "2 s"), (5000, "5 s"), (10000, "10 s")] {
                                    ui.selectable_value(&mut layout.update_ms, ms, text);
                                }
                            });
                        ui.small("Обновляется только этот виджет. FPS измеряется каждые 250 ms, CPU/GPU/RAM — раз в секунду.");
                        ui.small("Удерживайте ползунок: меню скрывается, виден HUD поверх игры.");
                        if ui.button("Использовать общий фон HUD").clicked() { layout.opacity = None; }
                    });
                if !open {
                    self.editing_widget = None;
                }
            } else {
                self.editing_widget = None;
            }
        }
        if self.editing_global {
            let mut open = true;
            egui::Window::new("Фон HUD · предпросмотр")
                .id(egui::Id::new("global-hud-preview"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    let slider = ui.add(
                        egui::Slider::new(&mut self.settings.opacity, 0.1..=0.9)
                            .text("Непрозрачность в игре"),
                    );
                    self.preview = slider.is_pointer_button_down_on();
                    ui.small("Удерживайте ползунок для предпросмотра поверх игры.");
                    ui.small("Виджеты с индивидуальным фоном не наследуют это значение.");
                });
            self.editing_global = open;
        }
        if self.preview {
            ctx.request_repaint();
        }
        if (self.settings.zoom - self.zoom).abs() > 0.0002 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if before != serde_json::to_vec(&self.settings).unwrap_or_default() {
            self.settings_dirty = Some(Instant::now());
        }
    }
    fn widgets(&mut self, ctx: &egui::Context, screen: egui::Rect, t: &Tokens, hud: bool) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.movement_updated)
            .as_secs_f32()
            .min(0.05);
        self.movement_updated = now;
        self.displayed_positions
            .resize(self.settings.widgets.len(), None);
        // Outer geometry, including the header and borders, always spans whole cells.
        if self.settings.grid_snap {
            for layout in &mut self.settings.widgets {
                let size = snap_metric_size(
                    egui::vec2(layout.size[0], layout.size[1]),
                    layout.widget,
                    self.settings.grid_size,
                    true,
                );
                if layout.size != [size.x, size.y] {
                    layout.size = [size.x, size.y];
                    self.layout_settled = false;
                    self.settings_dirty = Some(now);
                }
            }
        }
        if hud {
            self.metric_drag = None;
            self.metric_resize = None;
            self.window_drag = None;
        }
        if !hud && let Some(resize) = &self.metric_resize {
            let index = resize.index;
            if let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) {
                let layout = &mut self.settings.widgets[index];
                let old = layout.size;
                let size = snap_metric_size(
                    resize.size + (pointer - resize.pointer),
                    layout.widget,
                    if self.settings.grid_snap {
                        self.settings.grid_size
                    } else {
                        1.
                    },
                    false,
                );
                layout.size = [size.x, size.y];
                if old != layout.size {
                    if !self.push_windows(index, screen, ctx, pointer - resize.pointer) {
                        self.settings.widgets[index].size = old;
                    }
                    self.settings_dirty = Some(now);
                }
            }
            if !ctx.input(|i| i.pointer.primary_down()) {
                self.metric_resize = None;
            }
        }
        if !hud && let Some(drag) = &self.metric_drag {
            let index = drag.index;
            if let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) {
                let layout = &mut self.settings.widgets[index];
                let previous = egui::pos2(layout.position[0], layout.position[1]);
                let mut position = pointer - drag.offset;
                let size = egui::vec2(layout.size[0], layout.size[1]);
                let maximum = screen.max - size;
                if self.settings.grid_snap {
                    let step = self.settings.grid_size;
                    position.x = (position.x / step).round() * step;
                    position.y = (position.y / step).round() * step;
                    position.x = position.x.clamp(
                        (screen.left() / step).ceil() * step,
                        (maximum.x / step)
                            .floor()
                            .max((screen.left() / step).ceil())
                            * step,
                    );
                    position.y = position.y.clamp(
                        (screen.top() / step).ceil() * step,
                        (maximum.y / step).floor().max((screen.top() / step).ceil()) * step,
                    );
                } else {
                    position.x = position
                        .x
                        .clamp(screen.left(), maximum.x.max(screen.left()));
                    position.y = position.y.clamp(screen.top(), maximum.y.max(screen.top()));
                }
                layout.position = [position.x, position.y];
                if (position - previous).length_sq() > 0.01 {
                    if !self.push_windows(index, screen, ctx, position - previous) {
                        self.settings.widgets[index].position = [previous.x, previous.y];
                    }
                    self.settings_dirty = Some(Instant::now());
                }
            }
            if !ctx.input(|i| i.pointer.primary_down()) {
                self.metric_drag = None;
                // Already settled transactionally while dragging; do not send the dragged window back.
            }
        }
        self.readings
            .resize_with(self.settings.widgets.len(), || None);
        for index in 0..self.settings.widgets.len() {
            let layout = self.settings.widgets[index].clone();
            if !layout.visible {
                continue;
            }
            let id = egui::Id::new(("metric-widget", index));
            let target = egui::pos2(layout.position[0], layout.position[1]);
            let displayed = self.displayed_positions[index].get_or_insert(target);
            let difference = target - *displayed;
            if difference.length() > 0.15 {
                *displayed += difference * (1. - (-dt * 22.).exp());
                ctx.request_repaint_after(Duration::from_millis(16));
            } else {
                *displayed = target;
            }
            let pos = *displayed;
            let now = Instant::now();
            let interval = Duration::from_millis(layout.update_ms);
            if self.readings[index]
                .as_ref()
                .is_none_or(|(time, _)| time.elapsed() >= interval)
            {
                self.readings[index] = Some((now, self.widget_reading(layout.widget)));
            }
            // The line moves continuously; its sample ingestion is independent of label refresh.
            if matches!(layout.widget, Widget::FrameTime) {
                self.readings[index].as_mut().unwrap().1.intervals =
                    self.frame_history.iter().copied().collect();
            }
            let (updated, reading) = self.readings[index].as_ref().unwrap();
            ctx.request_repaint_after(interval.saturating_sub(updated.elapsed()));
            if hud {
                egui::Area::new(id.with("hud"))
                    .fixed_pos(pos)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_black_alpha(
                                (layout.opacity.unwrap_or(self.settings.opacity) * 255.) as u8,
                            ))
                            .corner_radius(0)
                            .inner_margin(8.)
                            .show(ui, |ui| {
                                ui.set_min_size(egui::vec2(
                                    layout.size[0] - 16.,
                                    layout.size[1] - 16.,
                                ));
                                ui.set_max_size(egui::vec2(
                                    layout.size[0] - 16.,
                                    layout.size[1] - 16.,
                                ));
                                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                                ui.label(
                                    RichText::new(layout.widget.title())
                                        .small()
                                        .color(Color32::from_gray(200)),
                                );
                                widget_content(ui, layout.widget, reading, t, true, layout.size[1]);
                            });
                    });
            } else {
                let mut open = true;
                let frame = egui::Frame::window(&ctx.global_style());
                let mut window = egui::Window::new(layout.widget.title())
                    .frame(frame)
                    .id(id)
                    .title_bar(false)
                    .movable(false)
                    .collapsible(false)
                    .default_pos(pos)
                    .fixed_size(egui::vec2(layout.size[0], layout.size[1]))
                    .scroll([false, true])
                    .constrain_to(screen);
                self.positions_pending.remove(&index);
                window = window.current_pos(pos);
                let response = window.show(ctx, |ui| {
                    // Own the drag instead of correcting egui's free-moving title bar after paint.
                    ui.horizontal(|ui| {
                        let title = ui
                            .add_sized(
                                egui::vec2(
                                    (ui.available_width()
                                        - 2. * t.compact_height
                                        - 2. * ui.spacing().item_spacing.x)
                                        .max(65.),
                                    t.compact_height,
                                ),
                                egui::Label::new(RichText::new(layout.widget.title()).strong())
                                    .truncate()
                                    .sense(egui::Sense::drag()),
                            )
                            .on_hover_cursor(egui::CursorIcon::Grab);
                        if title.drag_started_by(egui::PointerButton::Primary)
                            && let Some(start) = ui.input(|i| i.pointer.press_origin())
                        {
                            self.metric_drag = Some(MetricDrag {
                                index,
                                offset: start - pos,
                            });
                            self.window_drag = None;
                            self.metric_resize = None;
                        }
                        if title.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        if icon_button(
                            ui,
                            t,
                            Glyph::Pencil,
                            self.editing_widget == Some(index),
                            "Настроить прозрачность и обновление",
                        )
                        .clicked()
                        {
                            self.editing_widget = Some(index);
                        }
                        if icon_button(ui, t, Glyph::Close, false, "Скрыть метрику").clicked()
                        {
                            open = false;
                        }
                    });
                    ui.separator();
                    widget_content(ui, layout.widget, reading, t, false, layout.size[1]);
                    let corner = pos + egui::vec2(layout.size[0] - 12., layout.size[1] - 12.);
                    let grip_rect = egui::Rect::from_min_size(
                        corner - egui::vec2(12., 12.),
                        egui::vec2(18., 18.),
                    );
                    // The resize handle belongs to the outer window, not its scrolled content.
                    ui.set_clip_rect(ui.clip_rect().union(grip_rect));
                    let grip = ui
                        .interact(grip_rect, id.with("size-grip"), egui::Sense::drag())
                        .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                    ui.painter().line_segment(
                        [grip_rect.left_bottom(), grip_rect.right_top()],
                        egui::Stroke::new(2., t.muted),
                    );
                    if grip.drag_started_by(egui::PointerButton::Primary)
                        && let Some(pointer) = ui.input(|i| i.pointer.press_origin())
                    {
                        self.metric_resize = Some(MetricResize {
                            index,
                            pointer,
                            size: egui::vec2(layout.size[0], layout.size[1]),
                        });
                        self.metric_drag = None;
                    }
                });
                let saved = &mut self.settings.widgets[index];
                saved.visible = open;
                let _ = response;
            }
        }
        if !hud && self.metric_drag.is_none() && self.metric_resize.is_none() {
            if !self.layout_settled {
                for index in 0..self.settings.widgets.len() {
                    if self.settings.widgets[index].visible {
                        self.arrange_window(index, screen, true, ctx);
                    }
                }
                self.layout_settled = true;
            }
            if let Some(index) = self.window_drag {
                let released = !ctx.input(|i| i.pointer.primary_down());
                if !self.push_windows(index, screen, ctx, egui::Vec2::ZERO) {
                    self.arrange_window(index, screen, released, ctx);
                }
                if released {
                    self.window_drag = None;
                }
            }
        }
    }
    fn push_windows(
        &mut self,
        index: usize,
        screen: egui::Rect,
        ctx: &egui::Context,
        motion: egui::Vec2,
    ) -> bool {
        let rects: Vec<_> = self
            .settings
            .widgets
            .iter()
            .map(|w| {
                w.visible.then(|| {
                    egui::Rect::from_min_size(
                        egui::pos2(w.position[0], w.position[1]),
                        egui::vec2(w.size[0], w.size[1]),
                    )
                })
            })
            .collect();
        let obstacle = ctx.memory(|m| m.area_rect(egui::Id::new("dashboard")));
        if rects[index].is_some_and(|r| !screen.contains_rect(r)) {
            return false;
        }
        if obstacle.is_some_and(|o| rects[index].is_some_and(|r| o.expand(3.).intersects(r))) {
            return false;
        }
        let grid = self.settings.grid_snap.then_some(self.settings.grid_size);
        let Some(placed) =
            crate::layout::push_neighbours(&rects, index, screen, obstacle, grid, motion)
        else {
            return false;
        };
        for (layout, rect) in self.settings.widgets.iter_mut().zip(placed) {
            if let Some(rect) = rect {
                layout.position = [rect.left(), rect.top()];
            }
        }
        true
    }
    fn arrange_window(
        &mut self,
        index: usize,
        screen: egui::Rect,
        settle: bool,
        ctx: &egui::Context,
    ) {
        let layout = &self.settings.widgets[index];
        let mut rect = egui::Rect::from_min_size(
            egui::pos2(layout.position[0], layout.position[1]),
            egui::vec2(layout.size[0], layout.size[1]),
        );
        let mut others: Vec<_> = self
            .settings
            .widgets
            .iter()
            .enumerate()
            .filter(|(i, w)| *i != index && w.visible)
            .map(|(_, w)| {
                egui::Rect::from_min_size(
                    egui::pos2(w.position[0], w.position[1]),
                    egui::vec2(w.size[0], w.size[1]),
                )
            })
            .collect();
        if let Some(dashboard) = ctx.memory(|m| m.area_rect(egui::Id::new("dashboard"))) {
            others.push(dashboard);
        }
        let snap = 12.;
        let mut dx: f32 = snap + 1.;
        let mut dy: f32 = snap + 1.;
        let mut consider = |x: f32, y: f32| {
            if x.abs() < dx.abs() {
                dx = x;
            }
            if y.abs() < dy.abs() {
                dy = y;
            }
        };
        consider(
            screen.left() + 8. - rect.left(),
            screen.top() + 8. - rect.top(),
        );
        consider(
            screen.right() - 8. - rect.right(),
            screen.bottom() - 8. - rect.bottom(),
        );
        for other in &others {
            for x in [
                other.left() - rect.left(),
                other.right() - rect.right(),
                other.right() + 8. - rect.left(),
                other.left() - 8. - rect.right(),
            ] {
                if rect.bottom() >= other.top() - snap && rect.top() <= other.bottom() + snap {
                    consider(x, snap + 1.);
                }
            }
            for y in [
                other.top() - rect.top(),
                other.bottom() - rect.bottom(),
                other.bottom() + 8. - rect.top(),
                other.top() - 8. - rect.bottom(),
            ] {
                if rect.right() >= other.left() - snap && rect.left() <= other.right() + snap {
                    consider(snap + 1., y);
                }
            }
        }
        rect = rect.translate(egui::vec2(
            if dx.abs() <= snap { dx } else { 0. },
            if dy.abs() <= snap { dy } else { 0. },
        ));
        if self.settings.grid_snap {
            let step = self.settings.grid_size;
            let position = egui::pos2(
                (rect.left() / step).round() * step,
                (rect.top() / step).round() * step,
            );
            rect = egui::Rect::from_min_size(position, rect.size());
        }
        if settle {
            // Choose the nearest free position from screen edges and neighbours' edges.
            let mut xs = vec![
                rect.left(),
                screen.left() + 8.,
                screen.right() - rect.width() - 8.,
            ];
            let mut ys = vec![
                rect.top(),
                screen.top() + 8.,
                screen.bottom() - rect.height() - 8.,
            ];
            for other in &others {
                xs.extend([other.left() - rect.width() - 8., other.right() + 8.]);
                ys.extend([other.top() - rect.height() - 8., other.bottom() + 8.]);
            }
            if self.settings.grid_snap {
                let step = self.settings.grid_size;
                // Neighbour edges can fall between grid lines due to text-driven widths.
                // Consider both sides, retaining the closest available grid-aligned candidate.
                xs = xs
                    .into_iter()
                    .flat_map(|x| [(x / step).floor() * step, (x / step).ceil() * step])
                    .collect();
                ys = ys
                    .into_iter()
                    .flat_map(|y| [(y / step).floor() * step, (y / step).ceil() * step])
                    .collect();
            }
            let mut best = None;
            for x in xs {
                for y in &ys {
                    let candidate = egui::Rect::from_min_size(egui::pos2(x, *y), rect.size());
                    if screen.contains_rect(candidate)
                        && others.iter().all(|o| !o.expand(3.).intersects(candidate))
                    {
                        let distance = (candidate.min - rect.min).length_sq();
                        if best.is_none_or(|(_, d)| distance < d) {
                            best = Some((candidate, distance));
                        }
                    }
                }
            }
            if let Some((candidate, _)) = best {
                rect = candidate;
            }
        }
        let saved = &mut self.settings.widgets[index];
        if (egui::pos2(saved.position[0], saved.position[1]) - rect.min).length() > 0.1 {
            saved.position = [rect.left(), rect.top()];
            self.positions_pending.insert(index);
            self.settings_dirty = Some(Instant::now());
        }
    }
    fn widget_reading(&self, widget: Widget) -> WidgetReading {
        let fps = (self.snapshot.fps.process == self.selected).then_some(&self.snapshot.fps);
        let app = self.selected.and_then(|id| {
            self.snapshot
                .applications
                .iter()
                .find(|a| a.processes.iter().any(|p| p.id == id))
        });
        let s = &self.snapshot.system;
        if matches!(widget, Widget::FrameTime) {
            return WidgetReading {
                intervals: self.frame_history.iter().copied().collect(),
                frame_ms: fps.and_then(|f| f.frame_ms),
                p95_ms: fps.and_then(|f| f.p95_ms),
                graph_status: fps.and_then(|f| {
                    if !f.frame_times_ms.is_empty()
                        && f.frame_timestamps_ns.len() != f.frame_times_ms.len()
                    {
                        Some(
                            "Запущен старый FPS-helper: перезапустите helper после обновления"
                                .into(),
                        )
                    } else if f.fps.is_none() {
                        Some(f.status.clone())
                    } else {
                        None
                    }
                }),
                ..Default::default()
            };
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
            Widget::AppDisk => format!(
                "Чтение {}\nЗапись {}",
                rate(app.and_then(|a| a.disk_read_bytes_per_sec)),
                rate(app.and_then(|a| a.disk_write_bytes_per_sec))
            ),
            Widget::SystemDisk => format!(
                "Чтение {}\nЗапись {}",
                rate(s.disk_read_bytes_per_sec),
                rate(s.disk_write_bytes_per_sec)
            ),
            Widget::FrameTime => unreachable!(),
        };
        WidgetReading {
            value,
            ..Default::default()
        }
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
        // Keep the game's controls immediately visible rather than below the metrics explanation.
        if let Some(app) = self.selected.and_then(|id| {
            self.snapshot
                .applications
                .iter()
                .find(|a| a.processes.iter().any(|p| p.id == id))
        }) {
            ui.push_id("active-game-audio", |ui| {
                self.audio_controls(ui, &self.audio_keys(app));
            });
            ui.separator();
        }
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
                        ui.label(number(app.cpu_percent, "%"))
                            .on_hover_text("100% = все логические CPU системы");
                        ui.label(bytes(app.memory_bytes));
                        ui.label(number(app.gpu_percent, "%")).on_hover_text(
                            "Самый занятый DRM engine приложения, шкала 0–100%; оценка по fdinfo",
                        );
                        ui.label(app.vram_bytes.map(bytes).unwrap_or_else(|| "—".into()));
                        ui.label(
                            app.io_bytes_per_sec
                                .map(|v| format!("{}/s", bytes(v as u64)))
                                .unwrap_or_else(|| "—".into()),
                        )
                        .on_hover_text(format!(
                            "Чтение {}\nЗапись {}",
                            rate(app.disk_read_bytes_per_sec),
                            rate(app.disk_write_bytes_per_sec)
                        ));
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
fn snap_metric_size(size: egui::Vec2, widget: Widget, step: f32, grow: bool) -> egui::Vec2 {
    let minimum = match widget {
        Widget::FrameTime => egui::vec2(320., 192.),
        Widget::AppDisk | Widget::SystemDisk => egui::vec2(320., 144.),
        _ => egui::vec2(192., 112.),
    };
    let quantize = |value: f32, min: f32| {
        let units = value / step;
        (if grow { units.ceil() } else { units.round() }).max((min / step).ceil()) * step
    };
    egui::vec2(quantize(size.x, minimum.x), quantize(size.y, minimum.y))
}
pub fn saved_hud_shortcut() -> String {
    std::fs::read(settings_path())
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .map(|s| s.hud_shortcut)
        .unwrap_or_else(|| Settings::default().hud_shortcut)
}
fn monotonic_seconds() -> f64 {
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
}
fn widget_content(
    ui: &mut Ui,
    widget: Widget,
    reading: &WidgetReading,
    t: &Tokens,
    hud: bool,
    height: f32,
) {
    if matches!(widget, Widget::FrameTime) {
        ui.label(format!(
            "Среднее {} · p95 {}",
            number(reading.frame_ms, " ms"),
            number(reading.p95_ms, " ms")
        ));
        if let Some(status) = &reading.graph_status {
            ui.colored_label(t.warning, status);
        }
        let height = if hud {
            height - 60.
        } else {
            ui.available_height()
        };
        frame_plot(ui, &reading.intervals, height.clamp(70., 400.), t);
    } else {
        ui.label(
            RichText::new(&reading.value)
                .size(if hud { 22. } else { 25. })
                .strong(),
        );
    }
}
fn sort_value(a: &Application, sort: Sort) -> f64 {
    match sort {
        Sort::Cpu => a.cpu_percent.unwrap_or(-1.),
        Sort::Gpu => a.gpu_percent.unwrap_or(-1.),
        Sort::Memory => a.memory_bytes as f64,
        Sort::Io => a.io_bytes_per_sec.unwrap_or(-1.),
    }
}
fn frame_plot(ui: &mut Ui, values: &[(f64, f64)], height: f32, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(120.), height),
        egui::Sense::hover(),
    );
    let plot = rect.shrink2(egui::vec2(6., 15.));
    // Fixed time axis + delayed playhead absorbs snapshot transport latency without jumps.
    let end = monotonic_seconds() - 0.65;
    let start = end - 10.;
    let visible: Vec<_> = values
        .iter()
        .filter(|(time, _)| *time >= start && *time <= end)
        .copied()
        .collect();
    let desired = visible.iter().map(|(_, ms)| *ms).fold(16.67, f64::max) * 1.1;
    let scale_id = ui.id().with("frametime-scale");
    let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
    let max = ui.ctx().data_mut(|d| {
        let previous = d.get_temp::<f64>(scale_id).unwrap_or(desired);
        let next = previous + (desired - previous) * (1. - (-dt * 2.).exp());
        d.insert_temp(scale_id, next);
        next
    });
    let painter = ui.painter().with_clip_rect(plot.intersect(ui.clip_rect()));
    ui.ctx().request_repaint_after(Duration::from_millis(33));
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
    if visible.is_empty() {
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
    let bins = (plot.width() as usize).clamp(1, 1600);
    let mut envelope = vec![None::<(f64, f64, f64)>; bins];
    for (time, ms) in &visible {
        let index = (((time - start) / 10.) * bins as f64).floor() as usize;
        if let Some(bin) = envelope.get_mut(index.min(bins - 1)) {
            *bin = Some(match *bin {
                Some((low, high, _)) => (low.min(*ms), high.max(*ms), *ms),
                None => (*ms, *ms, *ms),
            });
        }
    }
    let mut previous = None;
    for (i, sample) in envelope.into_iter().enumerate() {
        let Some((low, high, last)) = sample else {
            continue;
        };
        let x = plot.left() + i as f32 / bins.saturating_sub(1).max(1) as f32 * plot.width();
        let point =
            |value: f64| egui::pos2(x, plot.bottom() - (value / max) as f32 * plot.height());
        painter.line_segment([point(low), point(high)], egui::Stroke::new(1., t.accent));
        let last = point(last);
        if let Some(prev) = previous {
            painter.line_segment([prev, last], egui::Stroke::new(1., t.accent));
        }
        previous = Some(last);
    }
    ui.painter().text(
        rect.left_bottom(),
        egui::Align2::LEFT_BOTTOM,
        "10 секунд · каждый present · непрерывная шкала",
        egui::FontId::proportional(10.),
        t.muted,
    );
}
fn number(value: Option<f64>, unit: &str) -> String {
    value
        .map(|v| format!("{v:.1}{unit}"))
        .unwrap_or_else(|| "—".into())
}
fn rate(value: Option<f64>) -> String {
    value
        .map(|v| {
            if v >= 1048576. {
                format!("{:.1} MiB/s", v / 1048576.)
            } else if v >= 1024. {
                format!("{:.1} KiB/s", v / 1024.)
            } else {
                format!("{v:.0} B/s")
            }
        })
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
