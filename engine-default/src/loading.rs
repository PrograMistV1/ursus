use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::frame_pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use engine_core::render::graph::{pass, RenderGraph};
use engine_core::vulkan::gfx_pipeline::builder::PipelineBuilder;
use crate::passes::ui::UiPass;
use engine_core::vulkan::{GpuTexture, VulkanContext};
use glam::Vec2;
use std::sync::Arc;

#[repr(C)]
struct LoadingPC {
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
}

struct PendingState {
    bg_pipeline: vk::Pipeline,
    bg_layout: vk::PipelineLayout,
    device: ash::Device,
    logo_texture: GpuTexture,
}

thread_local! {
    static PENDING: std::cell::RefCell<Option<PendingState>> =
        std::cell::RefCell::new(None);
}

pub struct LoadingPipeline {
    bg_pipeline: vk::Pipeline,
    bg_layout: vk::PipelineLayout,
    device: ash::Device,
    _logo_texture: GpuTexture,
}

impl Default for LoadingPipeline {
    fn default() -> Self {
        let s =
            PENDING.with(|c| c.borrow_mut().take()).expect("LoadingPipeline::default() called without prior build()");
        Self { bg_pipeline: s.bg_pipeline, bg_layout: s.bg_layout, device: s.device, _logo_texture: s.logo_texture }
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

        let start_time = std::time::Instant::now();
        pass("loading_background")
            .write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, _rw, gpu| {
                let sc = pool.image(h_swapchain);
                let extent = sc.extent;

                let attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(sc.view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.05, 0.05, 0.08, 1.0] } });

                unsafe {
                    gpu.device().cmd_begin_rendering(
                        cmd,
                        &vk::RenderingInfo::default()
                            .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
                            .layer_count(1)
                            .color_attachments(std::slice::from_ref(&attachment)),
                    );
                    gpu.device().cmd_set_viewport(
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
                    gpu.device().cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);
                    gpu.device().cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, bg_pipeline);

                    let pc = LoadingPC {
                        time: start_time.elapsed().as_secs_f32(),
                        progress: 1.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                    };
                    let pc_bytes =
                        std::slice::from_raw_parts(&pc as *const LoadingPC as *const u8, size_of::<LoadingPC>());
                    gpu.device().cmd_push_constants(cmd, bg_layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);
                    gpu.device().cmd_draw(cmd, 3, 1, 0, 0);
                    gpu.device().cmd_end_rendering(cmd);
                }
                Ok(())
            })
            .build(graph);

        let ui_pass =
            Arc::new(UiPass::new(device, swapchain.format, gpu_assets.bindless.layout, &mut gpu_assets.shaders)?);
        let ui_pass_cap = Arc::clone(&ui_pass);

        pass("loading_logo")
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, _rw, gpu| {
                let sc = pool.image(h_swapchain);
                let screen = [sc.extent.width as f32, sc.extent.height as f32];

                let logo_h = screen[1].min(screen[0]) * 0.55;
                let logo_w = logo_h * logo_aspect;
                let logo_x = (screen[0] - logo_w) * 0.5;
                let logo_y = (screen[1] - logo_h) * 0.5 - screen[1] * 0.04;

                ui_pass_cap.begin(gpu.device(), cmd, sc.view, sc.extent, gpu.bindless.set);
                ui_pass_cap.draw_textured_rect(
                    gpu.device(),
                    cmd,
                    screen,
                    Vec2::new(logo_x, logo_y),
                    Vec2::new(logo_w, logo_h),
                    [1.0, 1.0, 1.0, 1.0],
                    logo_slot,
                );
                ui_pass_cap.end(gpu.device(), cmd);
                Ok(())
            })
            .build(graph);

        PENDING.with(|c| {
            *c.borrow_mut() = Some(PendingState { bg_pipeline, bg_layout, device: device.clone(), logo_texture });
        });

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
}
