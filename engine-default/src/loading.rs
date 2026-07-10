use crate::passes::ui::UiPass;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::frame_pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use engine_core::render::gfx::format::{Format, ImageLayout};
use engine_core::render::gfx::{CommandEncoder, PushConstantRange, ShaderStage};
use engine_core::render::graph::{pass, RenderGraph};
use engine_core::render::world::{PreparedUiDrawList, UiPrimitive};
use engine_core::vulkan::{GpuTexture, VulkanContext};
use glam::Vec2;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LoadingPC {
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
}

struct PendingState {
    logo_texture: GpuTexture,
}

thread_local! {
    static PENDING: std::cell::RefCell<Option<PendingState>> =
        std::cell::RefCell::new(None);
}

pub struct LoadingPipeline {
    _logo_texture: GpuTexture,
}

impl Default for LoadingPipeline {
    fn default() -> Self {
        let s =
            PENDING.with(|c| c.borrow_mut().take()).expect("LoadingPipeline::default() called without prior build()");
        Self { _logo_texture: s.logo_texture }
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

impl RenderPipeline for LoadingPipeline {
    fn build(
        ctx: &VulkanContext,
        gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized,
    {
        crate::builtin_shaders::register_builtin(&mut gpu_assets.shaders);

        let swapchain = ctx.swapchain.as_ref().unwrap();

        let h_swapchain = graph.pool.register_swapchain_external(swapchain.format);

        let push_range = PushConstantRange::of::<LoadingPC>(ShaderStage::Fragment);

        let handle = gpu_assets.shaders.by_name("loading").expect("shader 'loading' not registered");
        let (vert_spv, frag_spv) = gpu_assets.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'loading' must have frag").to_vec();

        let bg_pipeline = gpu_assets.create_fullscreen_pipeline(
            &vert_spv,
            &frag_spv,
            std::slice::from_ref(&swapchain.format),
            &[],
            std::slice::from_ref(&push_range),
            None,
        )?;

        let (logo_pixels, logo_w, logo_h) = load_svg_as_rgba("assets/ursus.svg").unwrap_or_else(|e| {
            log::warn!("Лого не загружено: {} — fallback 1x1", e);
            (vec![255u8, 255, 255, 255], 1, 1)
        });
        let logo_aspect = logo_w as f32 / logo_h as f32;

        let logo_texture = GpuTexture::upload_no_mip(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            gpu_assets.command_pool(),
            ctx.device.graphics_queue,
            &logo_pixels,
            logo_w,
            logo_h,
            Format::Rgba8Unorm,
            "ursus_logo",
        )?;
        let logo_slot = gpu_assets.bindless.alloc_slot(logo_texture.view);

        let start_time = std::time::Instant::now();
        pass("loading_background")
            .write(h_swapchain, ImageLayout::ColorAttachment)
            .record(move |enc: &mut CommandEncoder, _rw, _gpu| {
                enc.begin_rendering_clear(h_swapchain, [0.05, 0.05, 0.08, 1.0]);

                let extent = enc.extent_of(h_swapchain);

                enc.bind_pipeline(bg_pipeline);

                let pc = LoadingPC {
                    time: start_time.elapsed().as_secs_f32(),
                    progress: 1.0,
                    width: extent[0],
                    height: extent[1],
                };
                enc.push_constants(bg_pipeline, ShaderStage::Fragment, &pc);
                enc.draw(3);

                enc.end_rendering();
                Ok(())
            })
            .build(graph, &gpu_assets);

        let ui_pass = Arc::new(UiPass::new(gpu_assets, swapchain.format)?);
        let ui_pass_cap = Arc::clone(&ui_pass);

        pass("loading_logo")
            .read_write(h_swapchain, ImageLayout::ColorAttachment)
            .record(move |enc: &mut CommandEncoder, _rw, gpu| {
                let screen = enc.extent_of(h_swapchain);

                let logo_h = screen[1].min(screen[0]) * 0.55;
                let logo_w = logo_h * logo_aspect;
                let logo_x = (screen[0] - logo_w) * 0.5;
                let logo_y = (screen[1] - logo_h) * 0.5 - screen[1] * 0.04;

                let mut draw_list = PreparedUiDrawList::default();
                draw_list.primitives.push(UiPrimitive::TexturedRect {
                    pos: Vec2::new(logo_x, logo_y),
                    size: Vec2::new(logo_w, logo_h),
                    color: [1.0, 1.0, 1.0, 1.0],
                    bindless_slot: logo_slot,
                    uv: [0.0, 0.0, 1.0, 1.0],
                });

                ui_pass_cap.record_draw_list(enc, &draw_list, gpu, h_swapchain)
            })
            .build(graph, &gpu_assets);

        PENDING.with(|c| {
            *c.borrow_mut() = Some(PendingState { logo_texture });
        });

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
}
