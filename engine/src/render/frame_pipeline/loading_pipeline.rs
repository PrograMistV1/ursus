use crate::assets::gpu_server::GpuAssetServer;
use crate::render::frame_pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render::graph::{pass, RenderGraph};
use crate::vulkan::gfx_pipeline::builder::PipelineBuilder;
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::{GpuTexture, VulkanContext};
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

    ui_pass_ptr: *const UiPass,
    logo_slot: u32,
    logo_aspect: f32,
}
unsafe impl Send for LoadingFrameData {}

struct PendingState {
    bg_pipeline: vk::Pipeline,
    bg_layout: vk::PipelineLayout,
    ui_pass: UiPass,
    device: ash::Device,
    logo_texture: GpuTexture,
    logo_slot: u32,
    logo_aspect: f32,
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
    _logo_texture: GpuTexture,
    logo_slot: u32,
    logo_aspect: f32,
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
            _logo_texture: s.logo_texture,
            logo_slot: s.logo_slot,
            logo_aspect: s.logo_aspect,
        }
    }
}

fn load_svg_as_rgba(path: &str) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    use resvg::{tiny_skia, usvg};

    let svg_data = std::fs::read(path).map_err(|e| anyhow::anyhow!("Не удалось прочитать {}: {}", path, e))?;

    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&svg_data, &options).map_err(|e| anyhow::anyhow!("Ошибка парсинга SVG: {}", e))?;

    let svg_size = tree.size();
    let max_dim = 512u32;
    let scale = (max_dim as f32 / svg_size.width().max(svg_size.height())).min(4.0);
    let w = (svg_size.width() * scale) as u32;
    let h = (svg_size.height() * scale) as u32;

    let mut pixmap =
        tiny_skia::Pixmap::new(w, h).ok_or_else(|| anyhow::anyhow!("Не удалось создать pixmap {}x{}", w, h))?;

    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    let mut pixels = pixmap.data().to_vec();
    for chunk in pixels.chunks_mut(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.001 {
            chunk[0] = (chunk[0] as f32 / a).min(255.0) as u8;
            chunk[1] = (chunk[1] as f32 / a).min(255.0) as u8;
            chunk[2] = (chunk[2] as f32 / a).min(255.0) as u8;
        }
    }

    log::info!("SVG лого загружено: {}x{} (scale={})", w, h, scale);
    Ok((pixels, w, h))
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

        let (logo_pixels, logo_w, logo_h) = load_svg_as_rgba("assets/ursus.svg").unwrap_or_else(|e| {
            log::warn!("Лого не загружено: {} — fallback 1x1", e);
            (vec![255u8, 255, 255, 255], 1, 1)
        });
        let logo_aspect = logo_w as f32 / logo_h as f32;

        let logo_texture = GpuTexture::upload_no_mip(
            device,
            ctx.device.physical,
            &ctx.instance.handle,
            gpu_assets.command_pool(),
            ctx.device.graphics_queue,
            &logo_pixels,
            logo_w,
            logo_h,
            vk::Format::R8G8B8A8_UNORM,
            "ursus_logo",
        )?;
        let logo_slot = gpu_assets.bindless.alloc_slot(logo_texture.view);

        let device_bg = device.clone();
        pass("loading_background")
            .write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const LoadingFrameData);
                let sc = pool.image(h_swapchain);
                let extent = sc.extent;

                let attachment = vk::RenderingAttachmentInfo::default()
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
                        .color_attachments(std::slice::from_ref(&attachment)),
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

        pass("loading_logo")
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const LoadingFrameData);
                let sc = pool.image(h_swapchain);
                let ui = &*data.ui_pass_ptr;
                let screen = [data.width, data.height];

                let logo_h = data.height.min(data.width) * 0.55;
                let logo_w = logo_h * data.logo_aspect;
                let logo_x = (data.width - logo_w) * 0.5;
                let logo_y = (data.height - logo_h) * 0.5 - data.height * 0.04;

                ui.begin(&*data.device, cmd, sc.view, sc.extent, data.bindless_set);
                ui.draw_textured_rect(
                    &*data.device,
                    cmd,
                    screen,
                    Vec2::new(logo_x, logo_y),
                    Vec2::new(logo_w, logo_h),
                    [1.0, 1.0, 1.0, 1.0],
                    data.logo_slot,
                );
                ui.end(&*data.device, cmd);
                Ok(())
            })
            .build(graph);

        PENDING.with(|c| {
            *c.borrow_mut() = Some(PendingState {
                bg_pipeline,
                bg_layout,
                ui_pass,
                device: device.clone(),
                logo_texture,
                logo_slot,
                logo_aspect,
            });
        });

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()> {
        let (w, h) = input.output_resolution;

        graph.set_frame_data(Box::new(LoadingFrameData {
            device: input.device,
            time: self.start_time.elapsed().as_secs_f32(),
            progress: 1.0,
            width: w as f32,
            height: h as f32,
            bindless_set: input.gpu_assets.bindless.set,
            ui_pass_ptr: &self.ui_pass as *const UiPass,
            logo_slot: self.logo_slot,
            logo_aspect: self.logo_aspect,
        }));

        Ok(())
    }

    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
}
