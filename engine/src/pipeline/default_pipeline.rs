use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::{CpuAssetServer, MaterialHandle, MeshHandle};
use crate::components::Transform;
use crate::lighting::LightingUbo;
use crate::math::frustum::transform_aabb;
use crate::pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render_graph::{pass, RenderGraph, ResourceDesc, ResourceExtent};
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
use std::sync::Arc;

const LDR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

pub struct DefaultPipeline {
    ui_pass: Option<UiPass>,
}

impl Default for DefaultPipeline {
    fn default() -> Self {
        Self { ui_pass: None }
    }
}

impl RenderPipeline for DefaultPipeline {
    fn build(
        ctx: &VulkanContext,
        cpu_assets: &mut CpuAssetServer,
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

        let shadow_pass = ShadowPass::new(&ctx.device.handle, &mut cpu_assets.shaders)?;
        let depth_prepass = DepthPrepass::new(&ctx.device.handle, &mut cpu_assets.shaders)?;

        let mut geometry_pass = GeometryPass::new(
            &ctx.device.handle,
            GBuffer::color_formats(),
            gpu_assets.bindless.layout,
            gpu_assets.material_buffer.layout,
            cpu_assets,
        )?;

        let lighting_pass = LightingPass::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            vk::Format::R16G16B16A16_SFLOAT,
            &mut cpu_assets.shaders,
        )?;

        let post_pass = PostProcessPass::new(&ctx.device.handle, LDR_FORMAT, &mut cpu_assets.shaders)?;

        pass("shadow")
            .write(h_shadow_map, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    let sm = pool.image(h_shadow_map);

                    let calls: Vec<ShadowDrawCall> =
                        data.shadow_calls.iter().map(|dc| dc.as_shadow_draw_call()).collect();

                    shadow_pass.record(&*data.device, cmd, &sm, data.light_view_proj, &calls);
                    Ok(())
                }
            })
            .build(graph);

        pass("depth_prepass")
            .write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    let depth = pool.image(h_depth);

                    let calls: Vec<DepthPrepassDrawCall> = data
                        .prepass_calls
                        .iter()
                        .map(|dc| DepthPrepassDrawCall { gpu_mesh: &*dc.gpu_mesh_ptr, transform: &dc.transform })
                        .collect();

                    depth_prepass.record(&*data.device, cmd, &depth, data.view_proj, &calls);
                    Ok(())
                }
            })
            .build(graph);

        let debug_utils = ctx.debug_utils.clone();
        pass("geometry")
            .write(h_gbuffer_albedo, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .write(h_gbuffer_normal, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .read_write(h_depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
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
                }
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
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    let hdr = pool.image(h_hdr);
                    lighting_pass.upload_lights(&data.lighting);
                    lighting_pass.record(&*data.device, cmd, &hdr, &*data.camera);
                    Ok(())
                }
            })
            .build(graph);

        pass("post_process")
            .read(h_hdr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_ldr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_hdr, post_pass.descriptor_set, 0, post_pass.sampler)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    let ldr = pool.image(h_ldr);
                    post_pass.record_to_target(&*data.device, cmd, &ldr, data.exposure);
                    Ok(())
                }
            })
            .build(graph);

        let fsr_pass = Arc::new(FsrPass::new(&ctx.device.handle, LDR_FORMAT, &mut cpu_assets.shaders)?);
        let fsr_easu_set = fsr_pass.easu_descriptor_set;
        let fsr_rcas_set = fsr_pass.rcas_descriptor_set;
        let fsr_sampler = fsr_pass.sampler;
        let fsr_pass_easu = Arc::clone(&fsr_pass);
        let fsr_pass_rcas = Arc::clone(&fsr_pass);

        pass("fsr_easu")
            .read(h_ldr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_easu, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_ldr, fsr_easu_set, 0, fsr_sampler)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    let dst = pool.image(h_fsr_easu);
                    let (iw, ih) = data.internal_resolution;
                    let (ow, oh) = data.output_resolution;
                    let pc = compute_easu_con((iw as f32, ih as f32), (iw as f32, ih as f32), (ow as f32, oh as f32));
                    fsr_pass_easu.record_easu(&*data.device, cmd, &dst, &pc);
                    Ok(())
                }
            })
            .build(graph);

        pass("fsr_rcas")
            .read(h_fsr_easu, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h_fsr_rcas, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(h_fsr_easu, fsr_rcas_set, 0, fsr_sampler)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &*(ctx_ptr as *const DefaultPipelineFrameData);
                    let dst = pool.image(h_fsr_rcas);
                    let pc = compute_rcas_con(data.fsr_sharpness);
                    fsr_pass_rcas.record_rcas(&*data.device, cmd, &dst, &pc);
                    Ok(())
                }
            })
            .build(graph);

        let blit_handle = pass("blit_to_swapchain")
            .read(h_fsr_rcas, vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .write(h_swapchain, vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .record({
                move |cmd, pool, _ctx_ptr| {
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
                        let data = &*(_ctx_ptr as *const DefaultPipelineFrameData);
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
                }
            })
            .build(graph);

        pass("ui")
            .after(blit_handle)
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record({
                move |cmd, pool, ctx_ptr| unsafe {
                    let data = &mut *(ctx_ptr as *mut DefaultPipelineFrameData);
                    let sc = pool.image(h_swapchain);
                    UiPass.record(&*data.device, cmd, sc.view, sc.extent)?;
                    Ok(())
                }
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()> {
        use crate::math::frustum::extract_planes;

        let frustum = extract_planes(input.view_proj);
        input.gpu_assets.upload_materials(&input.cpu_assets);

        let mut draw_calls: Vec<OwnedDrawCall> = Vec::new();
        let mut prepass_calls: Vec<OwnedDrawCall> = Vec::new();

        for (mesh, transform, mat) in
            input.world.inner.query::<(&MeshHandle, &Transform, Option<&MaterialHandle>)>().iter()
        {
            let gpu = match input.gpu_assets.get_gpu_mesh(*mesh) {
                Some(g) => g,
                None => continue,
            };
            let shader = mat
                .and_then(|m| input.cpu_assets.get_material(*m))
                .map(|m| m.shader)
                .unwrap_or(input.cpu_assets.shaders.by_name("diffuse").unwrap());

            let model = transform.matrix();
            if !transform_aabb(&gpu.aabb, model).intersects_frustum(&frustum) {
                continue;
            }

            prepass_calls.push(OwnedDrawCall {
                gpu_mesh_ptr: gpu as *const _,
                transform: transform.clone(),
                material: None,
                shader,
            });
            draw_calls.push(OwnedDrawCall {
                gpu_mesh_ptr: gpu as *const _,
                transform: transform.clone(),
                material: mat.copied(),
                shader,
            });
        }

        draw_calls.sort_by_key(|dc| dc.shader.0);

        let shadow_calls: Vec<OwnedDrawCall> = draw_calls
            .iter()
            .map(|dc| OwnedDrawCall {
                gpu_mesh_ptr: dc.gpu_mesh_ptr,
                transform: dc.transform.clone(),
                material: dc.material,
                shader: dc.shader,
            })
            .collect();

        let mut lighting_frame = *input.lighting;
        lighting_frame.light_space_matrix = input.light_view_proj.to_cols_array_2d();

        let frame_data = Box::new(DefaultPipelineFrameData {
            device: input.device,
            draw_calls,
            prepass_calls,
            shadow_calls,
            camera: input.camera,
            view_proj: input.view_proj,
            light_view_proj: input.light_view_proj,
            lighting: lighting_frame,
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
    gpu_mesh_ptr: *const crate::assets::GpuMesh,
    transform: Transform,
    material: Option<MaterialHandle>,
    shader: crate::assets::shader_registry::ShaderHandle,
}
unsafe impl Send for OwnedDrawCall {}

impl OwnedDrawCall {
    fn as_shadow_draw_call(&self) -> ShadowDrawCall<'_> {
        ShadowDrawCall { gpu_mesh: unsafe { &*self.gpu_mesh_ptr }, transform: &self.transform }
    }

    fn as_draw_call(&self) -> DrawCall<'_> {
        DrawCall {
            gpu_mesh: unsafe { &*self.gpu_mesh_ptr },
            transform: &self.transform,
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
    camera: *const crate::vulkan::Camera,
    view_proj: glam::Mat4,
    light_view_proj: glam::Mat4,
    lighting: LightingUbo,
    gpu_assets: *const GpuAssetServer,
    exposure: f32,
    fsr_sharpness: f32,
    clear_color: [f32; 4],
    internal_resolution: (u32, u32),
    output_resolution: (u32, u32),
}

unsafe impl Send for DefaultPipelineFrameData {}
