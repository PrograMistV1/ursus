use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::{GpuMesh, MaterialHandle, ShaderHandle};
use crate::lighting::buffer::LightingUbo;
use crate::pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render_graph::{pass, RenderGraph, ResourceDesc, ResourceExtent};
use crate::render_world::{ExtractedInstance, ExtractedLights, RenderWorld};
use crate::vulkan::passes::depth_prepass::{DepthPrepass, DepthPrepassDrawCall};
use crate::vulkan::passes::fsr::{compute_easu_con, compute_rcas_con, FsrPass};
use crate::vulkan::passes::geometry::GeometryPass;
use crate::vulkan::passes::lighting::LightingPass;
use crate::vulkan::passes::post_process::PostProcessPass;
use crate::vulkan::passes::shadow::{ShadowDrawCall, ShadowPass};
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::resources::gbuffer::GBuffer;
use crate::vulkan::resources::shadow_map::SHADOW_MAP_SIZE;
use crate::vulkan::{DrawCall, VulkanContext};
use ash::vk;
use glam::{Mat4, Vec2};
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
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let sm = pool.image(h_shadow_map);
                let calls: Vec<ShadowDrawCall> = data.shadow_calls.iter().map(|dc| dc.as_shadow_draw_call()).collect();
                shadow_pass.record(&*data.device, cmd, &sm, data.light_view_proj, &calls);
                Ok(())
            })
            .build(graph);

        pass("depth_prepass")
            .write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let depth = pool.image(h_depth);
                let calls: Vec<DepthPrepassDrawCall> =
                    data.prepass_calls.iter().map(|dc| dc.as_depth_prepass_draw_call()).collect();
                depth_prepass.record(&*data.device, cmd, &depth, data.view_proj, &calls);
                Ok(())
            })
            .build(graph);

        let debug_utils = ctx.debug_utils.clone();
        pass("geometry")
            .write(h_gbuffer_albedo, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .write(h_gbuffer_normal, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .read_write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let albedo = pool.image(h_gbuffer_albedo);
                let normal = pool.image(h_gbuffer_normal);
                let depth = pool.image(h_depth);
                let draw_calls: Vec<DrawCall> = data.draw_calls.iter().map(|dc| dc.as_draw_call()).collect();
                geometry_pass.record(
                    &*data.device,
                    cmd,
                    &albedo,
                    &normal,
                    &depth,
                    data.clear_color,
                    data.view_proj,
                    &draw_calls,
                    &*data.gpu_assets,
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
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let hdr = pool.image(h_hdr);
                lighting_pass.upload_lights(&data.lighting);
                lighting_pass.record(&*data.device, cmd, &hdr, data.view, data.proj);
                Ok(())
            })
            .build(graph);

        pass("post_process")
            .read(h_hdr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_ldr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_hdr, post_pass.descriptor_set, 0, post_pass.sampler)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let ldr = pool.image(h_ldr);
                post_pass.record_to_target(&*data.device, cmd, &ldr, data.exposure);
                Ok(())
            })
            .build(graph);

        let fsr_pass = Arc::new(FsrPass::new(&ctx.device.handle, LDR_FORMAT, &mut gpu_assets.shaders)?);
        let (fsr_easu_set, fsr_rcas_set, fsr_sampler) =
            (fsr_pass.easu_descriptor_set, fsr_pass.rcas_descriptor_set, fsr_pass.sampler);
        let (fsr_pass_easu, fsr_pass_rcas) = (Arc::clone(&fsr_pass), Arc::clone(&fsr_pass));

        pass("fsr_easu")
            .read(h_ldr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_easu, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_ldr, fsr_easu_set, 0, fsr_sampler)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let dst = pool.image(h_fsr_easu);
                let (iw, ih) = data.internal_resolution;
                let (ow, oh) = data.output_resolution;
                let pc = compute_easu_con((iw as f32, ih as f32), (iw as f32, ih as f32), (ow as f32, oh as f32));
                fsr_pass_easu.record_easu(&*data.device, cmd, &dst, &pc);
                Ok(())
            })
            .build(graph);

        pass("fsr_rcas")
            .read(h_fsr_easu, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_rcas, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_fsr_easu, fsr_rcas_set, 0, fsr_sampler)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let dst = pool.image(h_fsr_rcas);
                let pc = compute_rcas_con(data.fsr_sharpness);
                fsr_pass_rcas.record_rcas(&*data.device, cmd, &dst, &pc);
                Ok(())
            })
            .build(graph);

        let blit_handle = pass("blit_to_swapchain")
            .read(h_fsr_rcas, vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .write(h_swapchain, vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| {
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
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    (*data.device).cmd_blit_image2(
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
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                let sc = pool.image(h_swapchain);
                let gpu_assets = &*data.gpu_assets;
                ui_pass.record(
                    &*data.device,
                    cmd,
                    sc.view,
                    sc.extent,
                    gpu_assets.bindless.set,
                    &data.ui_rects,
                    &data.ui_texts,
                    gpu_assets.font_atlas.as_ref(),
                    gpu_assets.font_atlas_texture.map(|h| h.0).unwrap_or(0),
                )?;
                Ok(())
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()> {
        let rw: &RenderWorld = input.render_world;
        let default_shader = input.gpu_assets.shaders.by_name("diffuse").unwrap();

        let to_owned = |instances: &[ExtractedInstance]| -> Vec<OwnedDrawCall> {
            instances
                .iter()
                .filter_map(|inst| {
                    let gpu = input.gpu_assets.get_gpu_mesh(inst.mesh)?;
                    let shader = default_shader; // материал -> шейдер сейчас не маршрутизируется отдельно; диффуз для всех
                    Some(OwnedDrawCall {
                        gpu_mesh_ptr: gpu as *const _,
                        model: inst.model,
                        material: inst.material,
                        shader,
                    })
                })
                .collect()
        };

        let meshes = rw.get::<crate::render_world::ExtractedMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);
        let shadow_meshes =
            rw.get::<crate::render_world::ExtractedShadowMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);
        let camera = rw.get::<crate::render_world::ExtractedCamera>().cloned().unwrap_or_default();
        let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();
        let ui_rects = rw
            .get::<crate::render_world::ExtractedUiRects>()
            .map(|r| r.rects.iter().map(|r| (r.pos, r.size, r.color)).collect())
            .unwrap_or_default();
        let ui_texts = rw
            .get::<crate::render_world::ExtractedUiTexts>()
            .map(|t| t.texts.iter().map(|t| (t.pos, t.text.clone(), t.font_size, t.color)).collect())
            .unwrap_or_default();

        let mut draw_calls = to_owned(meshes);
        draw_calls.sort_by_key(|dc| dc.shader.0);
        let prepass_calls = draw_calls
            .iter()
            .map(|dc| OwnedDrawCall {
                gpu_mesh_ptr: dc.gpu_mesh_ptr,
                model: dc.model,
                material: None,
                shader: dc.shader,
            })
            .collect();
        let shadow_calls = to_owned(shadow_meshes);

        let frame_data = Box::new(DefaultPipelineFrameData {
            device: input.device,
            draw_calls,
            prepass_calls,
            shadow_calls,
            view: camera.view,
            proj: camera.proj,
            view_proj: camera.view_proj,
            light_view_proj: lights.light_view_proj,
            lighting: LightingUbo {
                directional: lights.directional,
                point_lights: lights.point_lights,
                point_light_count: lights.point_light_count,
                _pad: [0; 3],
                light_space_matrix: lights.light_view_proj.to_cols_array_2d(),
            },
            ui_rects,
            ui_texts,
            gpu_assets: input.gpu_assets,
            exposure: input.exposure,
            fsr_sharpness: input.fsr_sharpness,
            clear_color: input.clear_color,
            internal_resolution: input.internal_resolution,
            output_resolution: input.output_resolution,
        });

        graph.set_frame_data(frame_data);
        Ok(())
    }
}

struct OwnedDrawCall {
    gpu_mesh_ptr: *const GpuMesh,
    model: Mat4,
    material: Option<MaterialHandle>,
    shader: ShaderHandle,
}
unsafe impl Send for OwnedDrawCall {}

impl OwnedDrawCall {
    fn as_shadow_draw_call(&self) -> ShadowDrawCall<'_> {
        ShadowDrawCall { gpu_mesh: unsafe { &*self.gpu_mesh_ptr }, model: self.model }
    }
    fn as_depth_prepass_draw_call(&self) -> DepthPrepassDrawCall<'_> {
        DepthPrepassDrawCall { gpu_mesh: unsafe { &*self.gpu_mesh_ptr }, model: self.model }
    }
    fn as_draw_call(&self) -> DrawCall<'_> {
        DrawCall {
            gpu_mesh: unsafe { &*self.gpu_mesh_ptr },
            model: self.model,
            material: self.material,
            shader: self.shader,
        }
    }
}

struct DefaultPipelineFrameData {
    device: *const ash::Device,
    draw_calls: Vec<OwnedDrawCall>,
    prepass_calls: Vec<OwnedDrawCall>,
    shadow_calls: Vec<OwnedDrawCall>,
    view: Mat4,
    proj: Mat4,
    view_proj: Mat4,
    light_view_proj: Mat4,
    lighting: LightingUbo,
    ui_rects: Vec<(Vec2, Vec2, [f32; 4])>,
    ui_texts: Vec<(Vec2, String, f32, [f32; 4])>,
    gpu_assets: *const GpuAssetServer,
    exposure: f32,
    fsr_sharpness: f32,
    clear_color: [f32; 4],
    internal_resolution: (u32, u32),
    output_resolution: (u32, u32),
}
unsafe impl Send for DefaultPipelineFrameData {}
