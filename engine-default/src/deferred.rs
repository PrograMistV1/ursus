use crate::passes::depth_prepass::DepthPrepass;
use crate::passes::fsr::FsrPass;
use crate::passes::geometry::GeometryPass;
use crate::passes::lighting::LightingPass;
use crate::passes::post_process::PostProcessPass;
use crate::passes::shadow::ShadowPass;
use crate::passes::ui::UiPass;
use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::frame_pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use engine_core::render::graph::{pass, RenderGraph};
use engine_core::render::resource::{ResourceDesc, ResourceExtent};
use engine_core::vulkan::resources::gbuffer::GBuffer;
use engine_core::vulkan::resources::shadow_map::SHADOW_MAP_SIZE;
use engine_core::vulkan::VulkanContext;
use std::sync::Arc;

const LDR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

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
            vk::Format::D32_SFLOAT,
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
        let h_depth = graph.pool.register(ResourceDesc::depth(
            "depth",
            vk::Format::D32_SFLOAT,
            ResourceExtent::ScaleInternal(1.0),
        ));
        let h_hdr = graph.pool.register(ResourceDesc::color(
            "hdr",
            vk::Format::R16G16B16A16_SFLOAT,
            ResourceExtent::ScaleInternal(1.0),
        ));
        let h_ldr = graph.pool.register(ResourceDesc::color("ldr", LDR_FORMAT, ResourceExtent::ScaleInternal(1.0)));
        let h_fsr_easu =
            graph.pool.register(ResourceDesc::color("fsr_easu", LDR_FORMAT, ResourceExtent::ScaleOutput(1.0)));
        let h_fsr_rcas = graph.pool.register(
            ResourceDesc::color("fsr_rcas", LDR_FORMAT, ResourceExtent::ScaleOutput(1.0))
                .with_usage(vk::ImageUsageFlags::TRANSFER_SRC),
        );
        let h_swapchain = graph.pool.register_swapchain_external(swapchain.format);

        let shadow_pass = ShadowPass::new(gpu_assets)?;
        let depth_prepass = DepthPrepass::new(gpu_assets)?;

        let mut geometry_pass = GeometryPass::new(gpu_assets, GBuffer::color_formats())?;

        let lighting_pass = LightingPass::new(
            gpu_assets,
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            vk::Format::R16G16B16A16_SFLOAT,
        )?;
        let post_pass = PostProcessPass::new(gpu_assets, &ctx.device.handle, LDR_FORMAT)?;

        pass("shadow")
            .write(h_shadow_map, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |enc, rw, gpu| shadow_pass.record(enc, rw, gpu, h_shadow_map))
            .build(graph);

        pass("depth_prepass")
            .write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |enc, rw, gpu| depth_prepass.record(enc, rw, gpu, h_depth))
            .build(graph);

        pass("geometry")
            .write(h_gbuffer_albedo, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .write(h_gbuffer_normal, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .read_write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |enc, rw, gpu| geometry_pass.record(enc, rw, gpu, h_gbuffer_albedo, h_gbuffer_normal, h_depth))
            .build(graph);

        pass("lighting")
            .read(h_gbuffer_albedo, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .read(h_gbuffer_normal, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .read(h_depth, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .read(h_shadow_map, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_hdr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_gbuffer_albedo, lighting_pass.descriptor_set, 0, lighting_pass.sampler)
            .bind_sampled(h_gbuffer_normal, lighting_pass.descriptor_set, 1, lighting_pass.sampler)
            .bind_sampled(h_depth, lighting_pass.descriptor_set, 2, lighting_pass.sampler)
            .bind_sampled(h_shadow_map, lighting_pass.descriptor_set, 4, lighting_pass.shadow_sampler)
            .record(move |enc, rw, gpu| lighting_pass.record(enc, rw, gpu, h_hdr))
            .build(graph);

        pass("post_process")
            .read(h_hdr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_ldr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_hdr, post_pass.descriptor_set, 0, post_pass.sampler)
            .record(move |enc, rw, gpu| post_pass.record(enc, rw, gpu, h_ldr))
            .build(graph);

        let fsr_pass = Arc::new(FsrPass::new(gpu_assets, &ctx.device.handle, LDR_FORMAT)?);
        let (fsr_easu_set, fsr_rcas_set, fsr_sampler) =
            (fsr_pass.easu_descriptor_set, fsr_pass.rcas_descriptor_set, fsr_pass.sampler);
        let (fsr_easu, fsr_rcas) = (Arc::clone(&fsr_pass), Arc::clone(&fsr_pass));

        pass("fsr_easu")
            .read(h_ldr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_easu, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_ldr, fsr_easu_set, 0, fsr_sampler)
            .record(move |enc, rw, gpu| fsr_easu.record_easu_pass(enc, rw, gpu, h_ldr, h_fsr_easu))
            .build(graph);

        pass("fsr_rcas")
            .read(h_fsr_easu, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_rcas, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_fsr_easu, fsr_rcas_set, 0, fsr_sampler)
            .record(move |enc, rw, gpu| fsr_rcas.record_rcas_pass(enc, rw, gpu, h_fsr_rcas))
            .build(graph);

        let blit_handle = pass("blit_to_swapchain")
            .read(h_fsr_rcas, vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .write(h_swapchain, vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .record(move |enc, _rw, _gpu| {
                enc.blit_to_swapchain(h_fsr_rcas, h_swapchain);
                Ok(())
            })
            .build(graph);

        let ui_pass = UiPass::new(gpu_assets, swapchain.format)?;

        pass("ui")
            .after(blit_handle)
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |enc, rw, gpu| ui_pass.record(enc, rw, gpu, h_swapchain))
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }
}
