use crate::passes::depth_prepass::{DepthPrepass, DepthPrepassDrawCall};
use crate::passes::fsr::{compute_easu_con, compute_rcas_con, FsrPass};
use crate::passes::geometry::{DrawCall, GeometryPass};
use crate::passes::lighting::LightingPass;
use crate::passes::post_process::PostProcessPass;
use crate::passes::shadow::{ShadowDrawCall, ShadowPass};
use crate::passes::ui::UiPass;
use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::frame_pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use engine_core::render::graph::{pass, RenderGraph};
use engine_core::render::resource::{ResourceDesc, ResourceExtent};
use engine_core::render::world::{
    ExtractedCamera, ExtractedLights, ExtractedMeshes, ExtractedRenderSettings, ExtractedShadowMeshes,
    PreparedUiDrawList, UiPrimitive,
};
use engine_core::vulkan::resources::gbuffer::GBuffer;
use engine_core::vulkan::resources::light_buffer::LightingUbo;
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

        let shadow_pass = ShadowPass::new(&ctx.device.handle, &mut gpu_assets.shaders)?;
        let depth_prepass = DepthPrepass::new(&ctx.device.handle, &mut gpu_assets.shaders)?;

        let mut geometry_pass = GeometryPass::new(
            &ctx.device.handle,
            GBuffer::color_formats(),
            gpu_assets.bindless.layout,
            gpu_assets.material_buffer.layout,
            &mut gpu_assets.shaders,
        )?;

        let lighting_pass = LightingPass::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            vk::Format::R16G16B16A16_SFLOAT,
            &mut gpu_assets.shaders,
        )?;
        let post_pass = PostProcessPass::new(&ctx.device.handle, LDR_FORMAT, &mut gpu_assets.shaders)?;

        pass("shadow")
            .write(h_shadow_map, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, rw, gpu| {
                let sm = pool.image(h_shadow_map);
                let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();
                let meshes = rw.get::<ExtractedShadowMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);
                let calls: Vec<ShadowDrawCall> = meshes
                    .iter()
                    .filter_map(|inst| {
                        Some(ShadowDrawCall { gpu_mesh: gpu.get_gpu_mesh(inst.mesh)?, model: inst.model })
                    })
                    .collect();
                shadow_pass.record(gpu.device(), cmd, &sm, lights.light_view_proj, &calls);
                Ok(())
            })
            .build(graph);

        pass("depth_prepass")
            .write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, rw, gpu| {
                let depth = pool.image(h_depth);
                let camera = rw.get::<ExtractedCamera>().cloned().unwrap_or_default();
                let meshes = rw.get::<ExtractedMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);
                let calls: Vec<DepthPrepassDrawCall> = meshes
                    .iter()
                    .filter_map(|inst| {
                        Some(DepthPrepassDrawCall { gpu_mesh: gpu.get_gpu_mesh(inst.mesh)?, model: inst.model })
                    })
                    .collect();
                depth_prepass.record(gpu.device(), cmd, &depth, camera.view_proj, &calls);
                Ok(())
            })
            .build(graph);

        let debug_utils = ctx.debug_utils.clone();
        pass("geometry")
            .write(h_gbuffer_albedo, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .write(h_gbuffer_normal, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .read_write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, rw, gpu| {
                let camera = rw.get::<ExtractedCamera>().cloned().unwrap_or_default();
                let meshes = rw.get::<ExtractedMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);
                let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
                let albedo = pool.image(h_gbuffer_albedo);
                let normal = pool.image(h_gbuffer_normal);
                let depth = pool.image(h_depth);
                let default_shader = gpu.shaders.by_name("diffuse").unwrap();
                let draw_calls: Vec<DrawCall> = meshes
                    .iter()
                    .filter_map(|inst| {
                        Some(DrawCall {
                            gpu_mesh: gpu.get_gpu_mesh(inst.mesh)?,
                            model: inst.model,
                            material: inst.material,
                            shader: default_shader,
                        })
                    })
                    .collect();
                geometry_pass.record(
                    gpu.device(),
                    cmd,
                    &albedo,
                    &normal,
                    &depth,
                    settings.clear_color,
                    camera.view_proj,
                    &draw_calls,
                    gpu,
                    debug_utils.as_deref(),
                );
                Ok(())
            })
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
            .record(move |cmd, pool, rw, gpu| {
                let camera = rw.get::<ExtractedCamera>().cloned().unwrap_or_default();
                let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();
                let hdr = pool.image(h_hdr);
                let ubo = LightingUbo {
                    directional: lights.directional,
                    point_lights: lights.point_lights,
                    point_light_count: lights.point_light_count,
                    _pad: [0; 3],
                    light_space_matrix: lights.light_view_proj.to_cols_array_2d(),
                };
                lighting_pass.upload_lights(&ubo);
                lighting_pass.record(gpu.device(), cmd, &hdr, camera.view, camera.proj);
                Ok(())
            })
            .build(graph);

        pass("post_process")
            .read(h_hdr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_ldr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_hdr, post_pass.descriptor_set, 0, post_pass.sampler)
            .record(move |cmd, pool, rw, gpu| {
                let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
                let ldr = pool.image(h_ldr);
                post_pass.record_to_target(gpu.device(), cmd, &ldr, settings.exposure);
                Ok(())
            })
            .build(graph);

        let fsr_pass = Arc::new(FsrPass::new(&ctx.device.handle, LDR_FORMAT, &mut gpu_assets.shaders)?);
        let (fsr_easu_set, fsr_rcas_set, fsr_sampler) =
            (fsr_pass.easu_descriptor_set, fsr_pass.rcas_descriptor_set, fsr_pass.sampler);
        let (fsr_easu, fsr_rcas) = (Arc::clone(&fsr_pass), Arc::clone(&fsr_pass));

        pass("fsr_easu")
            .read(h_ldr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_easu, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_ldr, fsr_easu_set, 0, fsr_sampler)
            .record(move |cmd, pool, rw, gpu| {
                let dst = pool.image(h_fsr_easu);
                let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
                let (iw, ih) = (dst.extent.width, dst.extent.height);
                let (ow, oh) = settings.output_size;
                let pc = compute_easu_con((iw as f32, ih as f32), (iw as f32, ih as f32), (ow, oh));
                fsr_easu.record_easu(gpu.device(), cmd, &dst, &pc);
                Ok(())
            })
            .build(graph);

        pass("fsr_rcas")
            .read(h_fsr_easu, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_rcas, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_fsr_easu, fsr_rcas_set, 0, fsr_sampler)
            .record(move |cmd, pool, rw, gpu| {
                let dst = pool.image(h_fsr_rcas);
                let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
                let pc = compute_rcas_con(settings.fsr_sharpness);
                fsr_rcas.record_rcas(gpu.device(), cmd, &dst, &pc);
                Ok(())
            })
            .build(graph);

        let blit_handle = pass("blit_to_swapchain")
            .read(h_fsr_rcas, vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .write(h_swapchain, vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .record(move |cmd, pool, _rw, gpu| {
                let src = pool.image(h_fsr_rcas);
                let dst = pool.image(h_swapchain);
                let blit = vk::ImageBlit2::default()
                    .src_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .src_offsets([
                        vk::Offset3D::default(),
                        vk::Offset3D { x: src.extent.width as i32, y: src.extent.height as i32, z: 1 },
                    ])
                    .dst_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .dst_offsets([
                        vk::Offset3D::default(),
                        vk::Offset3D { x: dst.extent.width as i32, y: dst.extent.height as i32, z: 1 },
                    ]);
                unsafe {
                    gpu.device().cmd_blit_image2(
                        cmd,
                        &vk::BlitImageInfo2::default()
                            .src_image(src.image)
                            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                            .dst_image(dst.image)
                            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                            .regions(std::slice::from_ref(&blit))
                            .filter(vk::Filter::LINEAR),
                    );
                }
                Ok(())
            })
            .build(graph);

        let ui_pass =
            UiPass::new(&ctx.device.handle, swapchain.format, gpu_assets.bindless.layout, &mut gpu_assets.shaders)?;

        pass("ui")
            .after(blit_handle)
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, rw, gpu| {
                let Some(draw_list) = rw.get::<PreparedUiDrawList>() else {
                    return Ok(());
                };

                let sc = pool.image(h_swapchain);
                let screen = [sc.extent.width as f32, sc.extent.height as f32];

                ui_pass.begin(gpu.device(), cmd, sc.view, sc.extent, gpu.bindless.set);

                for primitive in &draw_list.primitives {
                    match primitive {
                        UiPrimitive::Rect { pos, size, color, border_radius: _ } => {
                            ui_pass.draw_rect(gpu.device(), cmd, screen, *pos, *size, *color);
                        }
                        UiPrimitive::TexturedRect { pos, size, color, bindless_slot, uv } => {
                            ui_pass.draw_textured_rect_uv(
                                gpu.device(),
                                cmd,
                                screen,
                                *pos,
                                *size,
                                *color,
                                *bindless_slot,
                                *uv,
                            );
                        }
                        UiPrimitive::GlyphRect { pos, size, color, texture_handle, uv } => {
                            let slot = gpu.texture_slot(*texture_handle);
                            ui_pass.draw_glyph_rect(gpu.device(), cmd, screen, *pos, *size, *color, slot, *uv);
                        }
                    }
                }

                ui_pass.end(gpu.device(), cmd);
                Ok(())
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }
}
