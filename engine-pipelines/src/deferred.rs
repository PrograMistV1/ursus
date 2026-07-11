use crate::passes::depth_prepass::DepthPrepass;
use crate::passes::fsr::FsrPass;
use crate::passes::geometry::GeometryPass;
use crate::passes::lighting::LightingPass;
use crate::passes::material_buffer::{resolve_material, MaterialBuffer, MaterialData};
use crate::passes::post_process::PostProcessPass;
use crate::passes::shadow::ShadowPass;
use crate::passes::ui::UiPass;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::frame_pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use engine_core::render::gfx::{Format, ImageLayout, ImageUsage};
use engine_core::render::graph::{pass, RenderGraph};
use engine_core::render::resource::{ResourceDesc, ResourceExtent};
use engine_core::vulkan::resources::gbuffer::GBuffer;
use engine_core::vulkan::resources::shadow_map::SHADOW_MAP_SIZE;
use engine_core::vulkan::VulkanContext;
use std::sync::Arc;

const LDR_FORMAT: Format = Format::Rgba8Unorm;

pub struct DefaultPipeline;

impl Default for DefaultPipeline {
    fn default() -> Self {
        Self
    }
}

impl RenderPipeline for DefaultPipeline {
    fn build(
        ctx: &VulkanContext,
        gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles> {
        crate::builtin_shaders::register_builtin(&mut gpu_assets.shaders);

        let swapchain = ctx.swapchain.as_ref().unwrap();

        let h_shadow_map = graph.pool.register(ResourceDesc::depth(
            "shadow_map",
            Format::Depth32Float,
            ResourceExtent::Absolute(SHADOW_MAP_SIZE, SHADOW_MAP_SIZE),
        ));
        let h_gbuffer_albedo = graph.pool.register(ResourceDesc::color(
            "gbuffer_albedo",
            GBuffer::ALBEDO_FORMAT,
            ResourceExtent::ScaleInternal(1.0),
        ));
        let h_gbuffer_normal = graph.pool.register(ResourceDesc::color(
            "gbuffer_normal",
            GBuffer::NORMAL_FORMAT,
            ResourceExtent::ScaleInternal(1.0),
        ));
        let h_depth =
            graph.pool.register(ResourceDesc::depth("depth", Format::Depth32Float, ResourceExtent::ScaleInternal(1.0)));
        let h_hdr =
            graph.pool.register(ResourceDesc::color("hdr", Format::Rgba16Float, ResourceExtent::ScaleInternal(1.0)));
        let h_ldr = graph.pool.register(ResourceDesc::color("ldr", LDR_FORMAT, ResourceExtent::ScaleInternal(1.0)));
        let h_fsr_easu =
            graph.pool.register(ResourceDesc::color("fsr_easu", LDR_FORMAT, ResourceExtent::ScaleOutput(1.0)));
        let h_fsr_rcas = graph.pool.register(
            ResourceDesc::color("fsr_rcas", LDR_FORMAT, ResourceExtent::ScaleOutput(1.0))
                .with_usage(ImageUsage::TRANSFER_SRC),
        );
        let h_swapchain = graph.pool.register_swapchain_external(swapchain.format);

        let material_buffer = Arc::new(MaterialBuffer::new(gpu_assets)?);

        let shadow_pass = ShadowPass::new(gpu_assets, &material_buffer)?;
        let depth_prepass = DepthPrepass::new(gpu_assets, &material_buffer)?;

        let mut geometry_pass = GeometryPass::new(gpu_assets, GBuffer::color_formats(), &material_buffer)?;

        let lighting_pass = LightingPass::new(gpu_assets, Format::Rgba16Float)?;
        let post_pass = PostProcessPass::new(gpu_assets, LDR_FORMAT)?;

        {
            let material_buffer = Arc::clone(&material_buffer);
            pass("shadow")
                .write(h_shadow_map, ImageLayout::DepthAttachment)
                .record(move |enc, rw, gpu| shadow_pass.record(enc, rw, gpu, &material_buffer, h_shadow_map))
                .build(graph, &gpu_assets);
        }

        {
            let material_buffer = Arc::clone(&material_buffer);
            pass("depth_prepass")
                .write(h_depth, ImageLayout::DepthAttachment)
                .record(move |enc, rw, gpu| depth_prepass.record(enc, rw, gpu, &material_buffer, h_depth))
                .build(graph, &gpu_assets);
        }

        {
            let material_buffer = Arc::clone(&material_buffer);
            pass("geometry")
                .write(h_gbuffer_albedo, ImageLayout::ColorAttachment)
                .write(h_gbuffer_normal, ImageLayout::ColorAttachment)
                .read_write(h_depth, ImageLayout::DepthAttachment)
                .record(move |enc, rw, gpu| {
                    let max_handle = gpu.material_handles().map(|h| h.0).max();
                    if let Some(max_handle) = max_handle {
                        let mut collected = vec![MaterialData::default_white(); max_handle as usize + 1];
                        for handle in gpu.material_handles() {
                            collected[handle.0 as usize] = resolve_material(gpu, handle);
                        }
                        material_buffer.upload(&collected);
                    }
                    geometry_pass.record(enc, rw, gpu, &material_buffer, h_gbuffer_albedo, h_gbuffer_normal, h_depth)
                })
                .build(graph, &gpu_assets);
        }

        pass("lighting")
            .read(h_gbuffer_albedo, ImageLayout::ShaderReadOnly)
            .read(h_gbuffer_normal, ImageLayout::ShaderReadOnly)
            .read(h_depth, ImageLayout::ShaderReadOnly)
            .read(h_shadow_map, ImageLayout::ShaderReadOnly)
            .write(h_hdr, ImageLayout::ColorAttachment)
            .bind_sampled(h_gbuffer_albedo, lighting_pass.descriptor_set, 0, lighting_pass.sampler)
            .bind_sampled(h_gbuffer_normal, lighting_pass.descriptor_set, 1, lighting_pass.sampler)
            .bind_sampled(h_depth, lighting_pass.descriptor_set, 2, lighting_pass.sampler)
            .bind_sampled(h_shadow_map, lighting_pass.descriptor_set, 4, lighting_pass.shadow_sampler)
            .record(move |enc, rw, gpu| lighting_pass.record(enc, rw, gpu, h_hdr))
            .build(graph, &gpu_assets);

        pass("post_process")
            .read(h_hdr, ImageLayout::ShaderReadOnly)
            .write(h_ldr, ImageLayout::ColorAttachment)
            .bind_sampled(h_hdr, post_pass.descriptor_set, 0, post_pass.sampler)
            .record(move |enc, rw, gpu| post_pass.record(enc, rw, gpu, h_ldr))
            .build(graph, &gpu_assets);

        let fsr_pass = Arc::new(FsrPass::new(gpu_assets, LDR_FORMAT)?);
        let (fsr_easu_set, fsr_rcas_set, fsr_sampler) =
            (fsr_pass.easu_descriptor_set, fsr_pass.rcas_descriptor_set, fsr_pass.sampler);
        let (fsr_easu, fsr_rcas) = (Arc::clone(&fsr_pass), Arc::clone(&fsr_pass));

        pass("fsr_easu")
            .read(h_ldr, ImageLayout::ShaderReadOnly)
            .write(h_fsr_easu, ImageLayout::ColorAttachment)
            .bind_sampled(h_ldr, fsr_easu_set, 0, fsr_sampler)
            .record(move |enc, rw, gpu| fsr_easu.record_easu_pass(enc, rw, gpu, h_ldr, h_fsr_easu))
            .build(graph, &gpu_assets);

        pass("fsr_rcas")
            .read(h_fsr_easu, ImageLayout::ShaderReadOnly)
            .write(h_fsr_rcas, ImageLayout::ColorAttachment)
            .bind_sampled(h_fsr_easu, fsr_rcas_set, 0, fsr_sampler)
            .record(move |enc, rw, gpu| fsr_rcas.record_rcas_pass(enc, rw, gpu, h_fsr_rcas))
            .build(graph, &gpu_assets);

        let blit_handle = pass("blit_to_swapchain")
            .read(h_fsr_rcas, ImageLayout::TransferSrc)
            .write(h_swapchain, ImageLayout::TransferDst)
            .record(move |enc, _rw, _gpu| {
                enc.blit_to_swapchain(h_fsr_rcas, h_swapchain);
                Ok(())
            })
            .build(graph, &gpu_assets);

        let ui_pass = UiPass::new(gpu_assets, swapchain.format)?;

        pass("ui")
            .after(blit_handle)
            .read_write(h_swapchain, ImageLayout::ColorAttachment)
            .record(move |enc, rw, gpu| ui_pass.record(enc, rw, gpu, h_swapchain))
            .build(graph, &gpu_assets);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }
}
