use crate::vulkan::timestamps::GpuFrameTimes;
use std::collections::VecDeque;

pub struct CpuFrameHistory {
    pub samples: VecDeque<f32>,
    pub capacity: usize,
    pub max_ms: f32,
}

impl CpuFrameHistory {
    pub fn new(capacity: usize) -> Self {
        Self { samples: VecDeque::with_capacity(capacity), capacity, max_ms: 33.3 }
    }

    pub fn push(&mut self, frame_ms: f32) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(frame_ms);
        let actual_max = self.samples.iter().cloned().fold(0.0f32, f32::max);
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
    pub vsync: bool,
    pub exposure: f32,
    pub swapchain_dirty: bool,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            show_overlay: true,
            show_settings: false,
            show_profiler: false,
            vsync: false,
            exposure: 0.5,
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
    gpu_times: Option<&GpuFrameTimes>,
) {
    if state.show_overlay {
        draw_overlay(ctx, state, fps, entity_count, cpu_history, gpu_times);
    }

    if state.show_settings {
        draw_settings(ctx, state);
    }

    if state.show_profiler {
        puffin_egui::profiler_window(ctx);
    }
}

fn draw_overlay(
    ctx: &egui::Context,
    state: &mut DebugUiState,
    fps: f32,
    entity_count: u32,
    cpu_history: &CpuFrameHistory,
    gpu_times: Option<&GpuFrameTimes>,
) {
    egui::Window::new("##overlay")
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .frame(egui::Frame::window(&ctx.style()).multiply_with_opacity(0.6))
        .show(ctx, |ui| {
            let avg_cpu = cpu_history.avg_ms();
            ui.label(format!("FPS  {fps:.0}  |  CPU {avg_cpu:.2} ms  |  ents {entity_count}"));

            if let Some(gpu) = gpu_times {
                ui.separator();
                ui.label(format!("GPU total  {:.2} ms", gpu.total_ms));

                if !gpu.passes.is_empty() {
                    ui.add_space(2.0);

                    let bar_h = 14.0;
                    let desired = egui::vec2(ui.available_width(), bar_h);
                    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());

                    if ui.is_rect_visible(rect) && gpu.total_ms > 0.0 {
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(120));

                        let mut x = rect.left();
                        for (i, (name, ms)) in gpu.passes.iter().enumerate() {
                            if *ms <= 0.0 {
                                continue;
                            }
                            let frac = ms / gpu.total_ms;
                            let w = frac * rect.width();
                            let seg = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, bar_h));
                            painter.rect_filled(seg, 0.0, pass_color(i));
                            if w > 24.0 {
                                painter.text(
                                    seg.center(),
                                    egui::Align2::CENTER_CENTER,
                                    name,
                                    egui::FontId::monospace(8.0),
                                    egui::Color32::WHITE,
                                );
                            }
                            x += w;
                        }
                    }

                    ui.add_space(2.0);
                    egui::Grid::new("gpu_passes").num_columns(3).spacing([12.0, 1.0]).show(ui, |ui| {
                        for (i, (name, ms)) in gpu.passes.iter().enumerate() {
                            let pct = if gpu.total_ms > 0.0 {
                                ms / gpu.total_ms * 100.0
                            } else {
                                0.0
                            };
                            ui.colored_label(pass_color(i), name);
                            ui.label(format!("{ms:.3} ms"));
                            ui.label(format!("{pct:.1}%"));
                            ui.end_row();
                        }
                    });
                }
            }

            ui.separator();
            let samples = cpu_history.as_slice();
            let max_ms = cpu_history.max_ms;
            let desired = egui::vec2(ui.available_width(), 40.0);
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(120));

                draw_threshold_line(&painter, rect, 16.667, max_ms, egui::Color32::from_rgb(80, 200, 80));
                draw_threshold_line(&painter, rect, 33.333, max_ms, egui::Color32::from_rgb(200, 120, 40));

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
                        painter.rect_filled(bar_rect, 0.0, frame_time_color(ms));
                    }
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.small_button("Settings").clicked() {
                    state.show_settings = !state.show_settings;
                }
                if ui.small_button("Profiler").clicked() {
                    state.show_profiler = !state.show_profiler;
                }
            });
        });
}

fn draw_settings(ctx: &egui::Context, state: &mut DebugUiState) {
    egui::Window::new("Settings").resizable(true).default_width(280.0).show(ctx, |ui| {
        ui.heading("Display");
        ui.checkbox(&mut state.show_overlay, "Show overlay");

        let prev_vsync = state.vsync;
        ui.checkbox(&mut state.vsync, "VSync (FIFO)");
        if state.vsync != prev_vsync {
            state.swapchain_dirty = true;
        }

        ui.separator();
        ui.heading("Post-process");
        ui.add(egui::Slider::new(&mut state.exposure, 0.05..=4.0).text("Exposure").logarithmic(false));

        ui.separator();
        if ui.button("Close").clicked() {
            state.show_settings = false;
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
        egui::Color32::from_rgb(80, 200, 80)
    } else if ms < 33.4 {
        egui::Color32::from_rgb(220, 180, 40)
    } else {
        egui::Color32::from_rgb(220, 60, 60)
    }
}

const PASS_COLORS: [(u8, u8, u8); 8] = [
    (100, 140, 200),
    (80, 180, 100),
    (200, 160, 60),
    (180, 80, 180),
    (60, 180, 200),
    (40, 140, 160),
    (200, 100, 80),
    (160, 160, 60),
];

fn pass_color(i: usize) -> egui::Color32 {
    let (r, g, b) = PASS_COLORS[i % PASS_COLORS.len()];
    egui::Color32::from_rgb(r, g, b)
}
