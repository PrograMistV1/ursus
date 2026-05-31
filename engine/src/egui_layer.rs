use ash::vk;
use egui_ash_renderer::{DynamicRendering, Options, Renderer};
use egui_winit::State as EguiWinit;
use winit::window::Window;

pub struct EguiLayer {
    pub ctx: egui::Context,
    state: EguiWinit,
    renderer: Renderer,
}

impl EguiLayer {
    pub fn new(
        window: &Window,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        device: ash::Device,
        swapchain_format: vk::Format,
    ) -> anyhow::Result<Self> {
        let ctx = egui::Context::default();

        let state = EguiWinit::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let renderer = Renderer::with_default_allocator(
            instance,
            physical_device,
            device,
            DynamicRendering {
                color_attachment_format: swapchain_format,
                depth_attachment_format: None,
            },
            Options {
                in_flight_frames: 2,
                srgb_framebuffer: swapchain_format == vk::Format::B8G8R8A8_SRGB
                    || swapchain_format == vk::Format::R8G8B8A8_SRGB,
                enable_depth_test: false,
                enable_depth_write: false,
            },
        )?;

        Ok(Self {
            ctx,
            state,
            renderer,
        })
    }

    pub fn handle_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        let resp = self.state.on_window_event(window, event);
        resp.consumed
    }

    pub fn begin_frame(&mut self, window: &Window) -> egui::RawInput {
        self.state.take_egui_input(window)
    }

    pub fn end_frame_and_draw(
        &mut self,
        window: &Window,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        cmd: vk::CommandBuffer,
        extent: vk::Extent2D,
        output: egui::FullOutput,
    ) -> anyhow::Result<()> {
        self.state
            .handle_platform_output(window, output.platform_output.clone());
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        if !output.textures_delta.set.is_empty() {
            self.renderer.set_textures(
                queue,
                command_pool,
                output.textures_delta.set.as_slice(),
            )?;
        }

        self.renderer
            .cmd_draw(cmd, extent, output.pixels_per_point, &primitives)?;

        if !output.textures_delta.free.is_empty() {
            self.renderer
                .free_textures(output.textures_delta.free.as_slice())?;
        }
        Ok(())
    }
}
