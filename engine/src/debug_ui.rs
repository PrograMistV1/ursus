#[derive(Debug, Clone)]
pub struct DebugUiState {
    pub show_overlay: bool,
    pub show_settings: bool,
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
            vsync: false,
            exposure: 0.5,
            fxaa_enabled: true,
            swapchain_dirty: false,
        }
    }
}

pub fn draw(ctx: &egui::Context, state: &mut DebugUiState, fps: f32, entity_count: u32) {
    if state.show_overlay {
        egui::Window::new("##overlay")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
            .frame(egui::Frame::window(&ctx.style()).multiply_with_opacity(0.6))
            .show(ctx, |ui| {
                ui.label(format!("FPS  {fps:.0}"));
                ui.label(format!("ms   {:.2}", 1000.0 / fps.max(0.001)));
                ui.label(format!("ents {entity_count}"));
                ui.separator();
                if ui.small_button("⚙ Settings").clicked() {
                    state.show_settings = !state.show_settings;
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
}