use crate::assets::gpu_server::GpuAssetServer;
use crate::pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render_graph::{pass, RenderGraph};
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::pipeline::builder::PipelineBuilder;
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
    font_atlas_tex: u32,

    font_atlas_ptr: *const crate::assets::ui::FontAtlas,
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
        let s = PENDING
            .with(|c| c.borrow_mut().take())
            .expect("LoadingPipeline::default() вызван без предшествующего build()");
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

        let handle = gpu_assets.shaders.by_name("loading").expect("шейдер 'loading' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu_assets.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'loading' должен иметь frag").to_vec();

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
                let sc = pool.image(h_swapchain);

                let font_atlas = data.font_atlas_ptr.as_ref();
                let ui_pass = &*data.ui_pass_ptr;

                let w = data.width;
                let h = data.height;

                let font_size = 32.0f32;
                let text = "ENGINE";

                let text_width = if let Some(atlas) = font_atlas {
                    atlas.measure_text(text, font_size as u32)
                } else {
                    font_size * text.len() as f32 * 0.6
                };
                let line_height = if let Some(atlas) = font_atlas {
                    atlas.line_height(font_size as u32)
                } else {
                    font_size * 1.2
                };

                let center_x = (w - text_width) * 0.5;
                let center_y = (h - line_height) * 0.5 - 20.0;

                let sub_font_size = 14.0f32;
                let sub_text = "Loading...";
                let sub_width = if let Some(atlas) = font_atlas {
                    atlas.measure_text(sub_text, sub_font_size as u32)
                } else {
                    sub_font_size * sub_text.len() as f32 * 0.6
                };
                let sub_x = (w - sub_width) * 0.5;
                let sub_y = center_y + line_height + 8.0;

                let texts = vec![
                    (Vec2::new(center_x, center_y), text.to_string(), font_size, [1.0f32, 1.0, 1.0, 1.0]),
                    (Vec2::new(sub_x, sub_y), sub_text.to_string(), sub_font_size, [0.6f32, 0.7, 0.9, 0.8]),
                ];

                ui_pass.record(
                    &*data.device,
                    cmd,
                    sc.view,
                    sc.extent,
                    data.bindless_set,
                    &[],
                    &texts,
                    font_atlas,
                    data.font_atlas_tex,
                )?;

                Ok(())
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()> {
        let (w, h) = input.output_resolution;

        let font_atlas_tex = input.gpu_assets.font_atlas_texture.map(|h| h.0).unwrap_or(0);
        let bindless_set = input.gpu_assets.bindless.set;
        let font_atlas_ptr = input.gpu_assets.font_atlas.as_ref().map(|a| a as *const _).unwrap_or(std::ptr::null());

        graph.set_frame_data(Box::new(LoadingFrameData {
            device: input.device,
            time: self.start_time.elapsed().as_secs_f32(),
            progress: 0.0,
            width: w as f32,
            height: h as f32,
            bindless_set,
            font_atlas_tex,
            font_atlas_ptr,
            ui_pass_ptr: &self.ui_pass as *const UiPass,
        }));

        Ok(())
    }

    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
}
