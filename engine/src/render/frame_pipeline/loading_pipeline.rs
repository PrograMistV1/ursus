use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::ui::font_manager::FontId;
use crate::render::frame_pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render_graph::{pass, RenderGraph};
use crate::vulkan::gfx_pipeline::builder::PipelineBuilder;
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::VulkanContext;
use ash::vk;
use glam::Vec2;

#[repr(C)]
struct LoadingPC {
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
}

struct LoadingFrameData {
    device: *const ash::Device,
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
    bindless_set: vk::DescriptorSet,

    gpu_assets: *mut GpuAssetServer,
    ui_pass_ptr: *const UiPass,
}
unsafe impl Send for LoadingFrameData {}

struct PendingState {
    bg_pipeline: vk::Pipeline,
    bg_layout: vk::PipelineLayout,
    ui_pass: UiPass,
    device: ash::Device,
}

thread_local! {
    static PENDING: std::cell::RefCell<Option<PendingState>> =
        std::cell::RefCell::new(None);
}

pub struct LoadingPipeline {
    start_time: std::time::Instant,
    bg_pipeline: vk::Pipeline,
    bg_layout: vk::PipelineLayout,
    ui_pass: UiPass,
    device: ash::Device,
}

impl Default for LoadingPipeline {
    fn default() -> Self {
        let s =
            PENDING.with(|c| c.borrow_mut().take()).expect("LoadingPipeline::default() called without prior build()");
        Self {
            start_time: std::time::Instant::now(),
            bg_pipeline: s.bg_pipeline,
            bg_layout: s.bg_layout,
            ui_pass: s.ui_pass,
            device: s.device,
        }
    }
}

impl Drop for LoadingPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            self.device.destroy_pipeline(self.bg_pipeline, None);
            self.device.destroy_pipeline_layout(self.bg_layout, None);
        }
    }
}

impl RenderPipeline for LoadingPipeline {
    fn build(
        ctx: &VulkanContext,
        gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized,
    {
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let device = &ctx.device.handle;

        let h_swapchain = graph.pool.register_swapchain_external(swapchain.format);

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<LoadingPC>() as u32);

        let handle = gpu_assets.shaders.by_name("loading").expect("shader 'loading' not registered");
        let (vert_spv, frag_spv) = gpu_assets.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'loading' must have frag").to_vec();

        let (bg_pipeline, bg_layout) =
            PipelineBuilder::fullscreen(&vert_spv, &frag_spv, std::slice::from_ref(&swapchain.format))
                .push_constants(std::slice::from_ref(&push_range))
                .build(device)?;

        let ui_pass = UiPass::new(device, swapchain.format, gpu_assets.bindless.layout, &mut gpu_assets.shaders)?;

        PENDING.with(|c| {
            *c.borrow_mut() = Some(PendingState { bg_pipeline, bg_layout, ui_pass, device: device.clone() });
        });

        let device_bg = device.clone();
        pass("loading_background")
            .write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const LoadingFrameData);
                let sc = pool.image(h_swapchain);
                let extent = sc.extent;

                let color_attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(sc.view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.05, 0.05, 0.08, 1.0] } });

                device_bg.cmd_begin_rendering(
                    cmd,
                    &vk::RenderingInfo::default()
                        .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
                        .layer_count(1)
                        .color_attachments(std::slice::from_ref(&color_attachment)),
                );
                device_bg.cmd_set_viewport(
                    cmd,
                    0,
                    &[vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }],
                );
                device_bg.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);
                device_bg.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, bg_pipeline);

                let pc = LoadingPC { time: data.time, progress: data.progress, width: data.width, height: data.height };
                let pc_bytes = std::slice::from_raw_parts(&pc as *const LoadingPC as *const u8, size_of::<LoadingPC>());
                device_bg.cmd_push_constants(cmd, bg_layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);
                device_bg.cmd_draw(cmd, 3, 1, 0, 0);
                device_bg.cmd_end_rendering(cmd);
                Ok(())
            })
            .build(graph);

        pass("loading_text")
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const LoadingFrameData);
                let gpu = &mut *data.gpu_assets;
                let sc = pool.image(h_swapchain);
                let ui_pass = &*data.ui_pass_ptr;
                let screen = [data.width, data.height];
                let font = gpu.default_font;

                let font_size_big = 32.0f32;
                let font_size_sub = 14.0f32;
                let text = "ENGINE";
                let sub_text = "Loading...";

                let text_w = gpu.font_manager.measure(font, text, font_size_big);
                let sub_w = gpu.font_manager.measure(font, sub_text, font_size_sub);
                let line_h = gpu.font_manager.line_height(font_size_big);

                let cx = (data.width - text_w) * 0.5;
                let cy = (data.height - line_h) * 0.5 - 20.0;
                let sx = (data.width - sub_w) * 0.5;
                let sy = cy + line_h + 8.0;

                ui_pass.begin(&*data.device, cmd, sc.view, sc.extent, data.bindless_set);

                draw_text_line(
                    ui_pass,
                    &*data.device,
                    cmd,
                    screen,
                    font,
                    Vec2::new(cx, cy),
                    text,
                    font_size_big,
                    [1.0, 1.0, 1.0, 1.0],
                    gpu,
                );
                draw_text_line(
                    ui_pass,
                    &*data.device,
                    cmd,
                    screen,
                    font,
                    Vec2::new(sx, sy),
                    sub_text,
                    font_size_sub,
                    [0.6, 0.7, 0.9, 0.8],
                    gpu,
                );

                ui_pass.end(&*data.device, cmd);
                Ok(())
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()> {
        input.gpu_assets.flush_font_atlases()?;

        let (w, h) = input.output_resolution;

        graph.set_frame_data(Box::new(LoadingFrameData {
            device: input.device,
            time: self.start_time.elapsed().as_secs_f32(),
            progress: 1.0,
            width: w as f32,
            height: h as f32,
            bindless_set: input.gpu_assets.bindless.set,
            gpu_assets: input.gpu_assets as *mut GpuAssetServer,
            ui_pass_ptr: &self.ui_pass as *const UiPass,
        }));

        Ok(())
    }

    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
}

unsafe fn draw_text_line(
    ui_pass: &UiPass,
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    screen_size: [f32; 2],
    font: FontId,
    origin: Vec2,
    text: &str,
    px: f32,
    color: [f32; 4],
    gpu: &mut GpuAssetServer,
) {
    let ascent = px * 0.8;
    let baseline = Vec2::new(origin.x, origin.y + ascent);
    let mut cursor_x = baseline.x;

    for ch in text.chars() {
        if let Some(glyph) = gpu.font_manager.glyph(font, ch, px) {
            let slot = gpu.gpu_fonts.slot_for_glyph(&glyph);
            ui_pass.draw_sdf_glyph(device, cmd, screen_size, Vec2::new(cursor_x, baseline.y), &glyph, slot, color);
        }
        cursor_x += gpu.font_manager.advance(font, ch, px);
    }
}
