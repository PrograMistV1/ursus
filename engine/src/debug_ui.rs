use crate::vulkan::timestamps::{GpuFrameTimes, GpuStage};
use std::collections::VecDeque;

/// История CPU frame time (rolling window)
pub struct CpuFrameHistory {
    pub samples: VecDeque<f32>, // ms
    pub capacity: usize,
    pub max_ms: f32, // адаптивный максимум для графика
}

impl CpuFrameHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            max_ms: 33.3,
        }
    }

    pub fn push(&mut self, frame_ms: f32) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(frame_ms);

        // Пересчитываем max с небольшим сглаживанием
        let actual_max = self.samples.iter().cloned().fold(0.0f32, f32::max);
        // Плавно тянемся к actual_max * 1.2, но не падаем ниже 16ms
        self.max_ms = (actual_max * 1.2).max(16.0);
    }

    pub fn avg_ms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f32>() / self.samples.len() as f32
    }

    pub fn as_slice(&self) -> Vec<f32> {
        self.samples.iter().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct DebugUiState {
    pub show_overlay: bool,
    pub show_settings: bool,
    pub show_profiler: bool,
    pub show_perf: bool,
    pub vsync: bool,
    pub exposure: f32,
    pub fxaa_enabled: bool,
    pub swapchain_dirty: bool,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            show_overlay: true,
            show_settings: false,
            show_profiler: false,
            show_perf: false,
            vsync: false,
            exposure: 0.5,
            fxaa_enabled: true,
            swapchain_dirty: false,
        }
    }
}

pub fn draw(
    ctx: &egui::Context,
    state: &mut DebugUiState,
    fps: f32,
    entity_count: u32,
    cpu_history: &CpuFrameHistory,
    gpu_times: &GpuFrameTimes,
) {
    if state.show_overlay {
        egui::Window::new("##overlay")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
            .frame(egui::Frame::window(&ctx.style()).multiply_with_opacity(0.6))
            .show(ctx, |ui| {
                ui.label(format!("FPS  {fps:.0}"));
                ui.label(format!("CPU  {:.2} ms", cpu_history.avg_ms()));
                ui.label(format!("GPU  {:.2} ms", gpu_times.total_ms));
                ui.label(format!("ents {entity_count}"));
                ui.separator();
                if ui.small_button("Settings").clicked() {
                    state.show_settings = !state.show_settings;
                }
                if ui.small_button("Profiler").clicked() {
                    state.show_profiler = !state.show_profiler;
                }
                if ui.small_button("Perf").clicked() {
                    state.show_perf = !state.show_perf;
                }
            });
    }

    if state.show_settings {
        egui::Window::new("Settings")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Display");
                ui.checkbox(&mut state.show_overlay, "Show overlay");

                let prev_vsync = state.vsync;
                ui.checkbox(&mut state.vsync, "VSync (FIFO)");
                if state.vsync != prev_vsync {
                    state.swapchain_dirty = true;
                }

                ui.separator();
                ui.heading("Post-process");
                ui.add(
                    egui::Slider::new(&mut state.exposure, 0.05..=4.0)
                        .text("Exposure")
                        .logarithmic(false),
                );
                ui.checkbox(&mut state.fxaa_enabled, "FXAA");

                ui.separator();
                if ui.button("Close").clicked() {
                    state.show_settings = false;
                }
            });
    }

    if state.show_profiler {
        puffin_egui::profiler_window(ctx);
    }

    if state.show_perf {
        draw_perf_window(ctx, state, cpu_history, gpu_times);
    }
}

fn draw_perf_window(
    ctx: &egui::Context,
    state: &mut DebugUiState,
    cpu_history: &CpuFrameHistory,
    gpu_times: &GpuFrameTimes,
) {
    egui::Window::new("Performance")
        .resizable(true)
        .default_width(380.0)
        .min_width(300.0)
        .show(ctx, |ui| {
            // ── CPU Frame Time Graph ──────────────────────────────────────────
            ui.heading("CPU Frame Time");

            let samples = cpu_history.as_slice();
            let avg = cpu_history.avg_ms();
            let peak = samples.iter().cloned().fold(0.0f32, f32::max);
            let max_ms = cpu_history.max_ms;

            ui.horizontal(|ui| {
                ui.label(format!("avg {avg:.2} ms  peak {peak:.2} ms"));
                ui.label(format!(
                    "({:.0} fps)",
                    if avg > 0.0 { 1000.0 / avg } else { 0.0 }
                ));
            });

            // Рисуем график как bar chart через egui painter
            let desired = egui::vec2(ui.available_width(), 60.0);
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(120));

                // Линия 16.6ms (60fps)
                draw_threshold_line(
                    &painter,
                    rect,
                    16.667,
                    max_ms,
                    egui::Color32::from_rgb(80, 200, 80),
                );
                // Линия 33.3ms (30fps)
                draw_threshold_line(
                    &painter,
                    rect,
                    33.333,
                    max_ms,
                    egui::Color32::from_rgb(200, 120, 40),
                );

                if !samples.is_empty() {
                    let bar_w = rect.width() / samples.len() as f32;
                    for (i, &ms) in samples.iter().enumerate() {
                        let t = (ms / max_ms).clamp(0.0, 1.0);
                        let bar_h = t * rect.height();
                        let x = rect.left() + i as f32 * bar_w;
                        let bar_rect = egui::Rect::from_min_size(
                            egui::pos2(x, rect.bottom() - bar_h),
                            egui::vec2(bar_w.max(1.0) - 0.5, bar_h),
                        );
                        let color = frame_time_color(ms);
                        painter.rect_filled(bar_rect, 0.0, color);
                    }
                }

                // Метки
                painter.text(
                    egui::pos2(rect.left() + 2.0, rect.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    format!("{max_ms:.0} ms"),
                    egui::FontId::monospace(9.0),
                    egui::Color32::from_gray(160),
                );
            }

            ui.add_space(4.0);

            // ── GPU Pass Breakdown ────────────────────────────────────────────
            ui.separator();
            ui.heading("GPU Pass Breakdown");

            ui.horizontal(|ui| {
                ui.label(format!("total {:.2} ms", gpu_times.total_ms));
            });

            // Stacked bar
            let bar_h = 18.0;
            let desired = egui::vec2(ui.available_width(), bar_h);
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());

            if ui.is_rect_visible(rect) && gpu_times.total_ms > 0.0 {
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(120));

                let mut x = rect.left();
                for (i, stage) in GpuStage::ALL.iter().enumerate() {
                    let ms = gpu_times.pass_ms[i];
                    if ms <= 0.0 {
                        continue;
                    }
                    let frac = ms / gpu_times.total_ms;
                    let w = frac * rect.width();
                    let seg =
                        egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, bar_h));
                    painter.rect_filled(seg, 0.0, stage_color(i));
                    // Лейбл если есть место
                    if w > 28.0 {
                        painter.text(
                            seg.center(),
                            egui::Align2::CENTER_CENTER,
                            stage.name(),
                            egui::FontId::monospace(9.0),
                            egui::Color32::WHITE,
                        );
                    }
                    x += w;
                }
            }

            ui.add_space(6.0);

            // Таблица по pass-ам
            egui::Grid::new("gpu_pass_table")
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Pass").strong());
                    ui.label(egui::RichText::new("ms").strong());
                    ui.label(egui::RichText::new("%").strong());
                    ui.end_row();

                    for (i, stage) in GpuStage::ALL.iter().enumerate() {
                        let ms = gpu_times.pass_ms[i];
                        let pct = if gpu_times.total_ms > 0.0 {
                            ms / gpu_times.total_ms * 100.0
                        } else {
                            0.0
                        };

                        let color = stage_color(i);
                        ui.colored_label(color, stage.name());
                        ui.label(format!("{ms:.3}"));
                        ui.label(format!("{pct:.1}"));
                        ui.end_row();
                    }
                });

            ui.separator();
            if ui.button("Close").clicked() {
                state.show_perf = false;
            }
        });
}

fn draw_threshold_line(
    painter: &egui::Painter,
    rect: egui::Rect,
    threshold_ms: f32,
    max_ms: f32,
    color: egui::Color32,
) {
    if threshold_ms >= max_ms {
        return;
    }
    let t = threshold_ms / max_ms;
    let y = rect.bottom() - t * rect.height();
    painter.hline(rect.left()..=rect.right(), y, egui::Stroke::new(1.0, color));
    painter.text(
        egui::pos2(rect.right() - 2.0, y - 1.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{threshold_ms:.0}"),
        egui::FontId::monospace(8.0),
        color,
    );
}

fn frame_time_color(ms: f32) -> egui::Color32 {
    if ms < 16.7 {
        egui::Color32::from_rgb(80, 200, 80) // зелёный — 60fps+
    } else if ms < 33.4 {
        egui::Color32::from_rgb(220, 180, 40) // жёлтый — 30-60fps
    } else {
        egui::Color32::from_rgb(220, 60, 60) // красный — <30fps
    }
}

const STAGE_COLORS: [(u8, u8, u8); 7] = [
    (100, 140, 200), // Shadow       — синеватый
    (80, 180, 100),  // Geometry     — зелёный
    (200, 160, 60),  // Lighting     — жёлтый
    (180, 80, 180),  // PostProcess  — фиолетовый
    (60, 180, 200),  // FSR EASU     — голубой
    (40, 140, 160),  // FSR RCAS     — тёмно-голубой
    (200, 100, 80),  // UI           — оранжево-красный
];

fn stage_color(i: usize) -> egui::Color32 {
    let (r, g, b) = STAGE_COLORS[i % STAGE_COLORS.len()];
    egui::Color32::from_rgb(r, g, b)
}
