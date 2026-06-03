use crate::assets::AssetServer;
use crate::ecs::systems::collect_draw_calls;
use crate::ecs::GameWorld;
use crate::lighting::{compute_light_view_proj, LightingUbo};
use crate::math::frustum::{extract_planes, transform_aabb};
use crate::render_graph::{pass, RenderGraph, ResourceDesc, ResourceExtent, ResourcePool};
use crate::vulkan::core::commands::Commands;
use crate::vulkan::core::sync::FrameSync;
use crate::vulkan::frame_ctx::FrameCtx;
use crate::vulkan::passes::geometry::{DrawCall, GeometryPass};
use crate::vulkan::passes::lighting::LightingPass;
use crate::vulkan::passes::post_process::PostProcessPass;
use crate::vulkan::passes::shadow::{ShadowDrawCall, ShadowPass};
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::resources::gbuffer::GBuffer;
use crate::vulkan::resources::shadow_map::SHADOW_MAP_SIZE;
use crate::vulkan::{Device, VulkanContext};
use ash::vk;
use glam::{Mat4, Vec3};
use std::sync::Arc;

const FRAMES_IN_FLIGHT: u32 = 3;

pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(2.0, 2.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 60_f32.to_radians(),
            z_near: 0.1,
            z_far: 100.0,
        }
    }
}

impl Camera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let mut proj = Mat4::perspective_rh(self.fov_y, aspect, self.z_near, self.z_far);
        proj.y_axis.y *= -1.0;
        proj * view
    }
}

#[derive(Clone, Copy)]
struct Handles {
    shadow_map: crate::render_graph::ResourceHandle,
    gbuffer_albedo: crate::render_graph::ResourceHandle,
    gbuffer_normal: crate::render_graph::ResourceHandle,
    depth: crate::render_graph::ResourceHandle,
    hdr: crate::render_graph::ResourceHandle,
}

pub struct Renderer {
    pub graph: RenderGraph,
    pub commands: Commands,
    frames: Vec<FrameSync>,
    current_frame: usize,
    swapchain_loader: ash::khr::swapchain::Device,
    device: Arc<Device>,

    pub exposure: f32,
    pub fxaa_enabled: bool,
}

impl Renderer {
    pub fn new(ctx: &VulkanContext, assets: &mut AssetServer) -> anyhow::Result<Self> {
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let internal = (swapchain.extent.width, swapchain.extent.height);
        let output = internal;

        let pool = ResourcePool::new(
            ctx.device.handle.clone(),
            ctx.device.physical,
            ctx.instance.handle.clone(),
        );

        let mut graph = RenderGraph::new(pool, ctx.device.handle.clone(), internal, output);

        let h = Handles {
            shadow_map: graph.pool.register(ResourceDesc::depth(
                "shadow_map",
                vk::Format::D32_SFLOAT,
                ResourceExtent::Absolute(SHADOW_MAP_SIZE, SHADOW_MAP_SIZE),
            )),
            gbuffer_albedo: graph.pool.register(ResourceDesc::color(
                "gbuffer_albedo",
                GBuffer::ALBEDO_FORMAT,
                ResourceExtent::ScaleInternal(1.0),
            )),
            gbuffer_normal: graph.pool.register(ResourceDesc::color(
                "gbuffer_normal",
                GBuffer::NORMAL_FORMAT,
                ResourceExtent::ScaleInternal(1.0),
            )),
            depth: graph.pool.register(ResourceDesc::depth(
                "depth",
                vk::Format::D32_SFLOAT,
                ResourceExtent::ScaleInternal(1.0),
            )),
            hdr: graph.pool.register(ResourceDesc::color(
                "hdr",
                vk::Format::R16G16B16A16_SFLOAT,
                ResourceExtent::ScaleInternal(1.0),
            )),
        };

        let shadow_pass = ShadowPass::new(&ctx.device.handle)?;

        let mut geometry_pass = GeometryPass::new(
            &ctx.device.handle,
            GBuffer::color_formats(),
            assets.bindless.layout,
            assets.material_buffer.layout,
            assets,
        )?;

        let lighting_pass = LightingPass::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            vk::Format::R16G16B16A16_SFLOAT, // hdr format
        )?;

        let post_pass = PostProcessPass::new(&ctx.device.handle, swapchain.format)?;

        pass("shadow")
            .write(h.shadow_map, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record({
                move |cmd, pool, ctx_ptr| {
                    let ctx = unsafe { FrameCtx::from_ptr(ctx_ptr) };
                    let sm = pool.image(h.shadow_map);
                    let calls: Vec<ShadowDrawCall> = ctx
                        .shadow_calls
                        .iter()
                        .map(|dc| ShadowDrawCall {
                            gpu_mesh: dc.gpu_mesh,
                            transform: dc.transform,
                        })
                        .collect();
                    shadow_pass.record(ctx.device, cmd, sm, ctx.light_view_proj, &calls);
                    Ok(())
                }
            })
            .build(&mut graph);

        pass("geometry")
            .write(h.gbuffer_albedo, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .write(h.gbuffer_normal, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .read_write(h.depth, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .record({
                move |cmd, pool, ctx_ptr| {
                    let ctx = unsafe { FrameCtx::from_ptr(ctx_ptr) };
                    let albedo = pool.image(h.gbuffer_albedo);
                    let normal = pool.image(h.gbuffer_normal);
                    let depth = pool.image(h.depth);
                    geometry_pass.record(
                        ctx.device,
                        cmd,
                        albedo,
                        normal,
                        depth,
                        [0.0, 0.0, 0.0, 1.0],
                        ctx.view_proj,
                        &ctx.draw_calls,
                        ctx.assets,
                    );
                    Ok(())
                }
            })
            .build(&mut graph);

        pass("lighting")
            .read(h.gbuffer_albedo, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .read(h.gbuffer_normal, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .read(h.depth, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .read(h.shadow_map, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .write(h.hdr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .bind_sampled(
                h.gbuffer_albedo,
                lighting_pass.descriptor_set,
                0,
                lighting_pass.sampler,
            )
            .bind_sampled(
                h.gbuffer_normal,
                lighting_pass.descriptor_set,
                1,
                lighting_pass.sampler,
            )
            .bind_sampled(
                h.depth,
                lighting_pass.descriptor_set,
                2,
                lighting_pass.sampler,
            )
            .bind_sampled(
                h.shadow_map,
                lighting_pass.descriptor_set,
                4,
                lighting_pass.shadow_sampler,
            )
            .record({
                move |cmd, pool, ctx_ptr| {
                    let ctx = unsafe { FrameCtx::from_ptr(ctx_ptr) };
                    let hdr = pool.image(h.hdr);
                    lighting_pass.upload_lights(&ctx.lighting);
                    lighting_pass.record(ctx.device, cmd, hdr, ctx.camera);
                    Ok(())
                }
            })
            .build(&mut graph);

        pass("post_process")
            .read(h.hdr, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .bind_sampled(h.hdr, post_pass.descriptor_set, 0, post_pass.sampler)
            .record({
                move |cmd, _pool, ctx_ptr| {
                    let ctx = unsafe { FrameCtx::from_ptr(ctx_ptr) };
                    post_pass.record(
                        ctx.device,
                        cmd,
                        ctx.swapchain_image,
                        ctx.swapchain_view,
                        ctx.swapchain_extent,
                        ctx.exposure,
                        ctx.fxaa_enabled,
                    );
                    Ok(())
                }
            })
            .build(&mut graph);

        pass("ui")
            .record({
                move |cmd, _pool, ctx_ptr| {
                    let ctx = unsafe { FrameCtx::from_ptr(ctx_ptr) };
                    let egui = unsafe { &mut *ctx.egui };
                    let output = ctx
                        .egui_output
                        .take()
                        .expect("egui_output должен быть Some на момент ui pass");
                    UiPass.record(
                        ctx.device,
                        cmd,
                        ctx.swapchain_image,
                        ctx.swapchain_view,
                        ctx.swapchain_extent,
                        unsafe { &*ctx.window },
                        egui,
                        output,
                        ctx.graphics_queue,
                        ctx.command_pool,
                    )?;
                    Ok(())
                }
            })
            .build(&mut graph);

        graph.allocate()?;
        graph.compile()?;

        let frames: Vec<_> = (0..FRAMES_IN_FLIGHT)
            .map(|_| FrameSync::new(&ctx.device.handle))
            .collect::<anyhow::Result<_>>()?;

        let commands = Commands::new(
            &ctx.device.handle,
            ctx.device.graphics_family,
            FRAMES_IN_FLIGHT,
        )?;

        let swapchain_loader =
            ash::khr::swapchain::Device::new(&ctx.instance.handle, &ctx.device.handle);

        Ok(Self {
            graph,
            commands,
            frames,
            current_frame: 0,
            swapchain_loader,
            device: ctx.device.clone(),
            exposure: 0.5,
            fxaa_enabled: true,
        })
    }

    pub fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        world: &mut GameWorld,
        assets: &AssetServer,
        camera: &Camera,
        lighting: &LightingUbo,
        egui: &mut crate::egui_layer::EguiLayer,
        egui_output: egui::FullOutput,
        window: &winit::window::Window,
        clear_color: [f32; 4],
    ) -> anyhow::Result<bool> {
        puffin::profile_function!();

        let frame = &self.frames[self.current_frame];
        let cmd = self.commands.buffers[self.current_frame];
        let device = &ctx.device.handle;
        let swapchain = ctx.swapchain.as_ref().unwrap();

        let aspect = swapchain.extent.width as f32 / swapchain.extent.height as f32;
        let view_proj = camera.view_proj(aspect);
        let frustum = extract_planes(view_proj);

        let ecs_calls = collect_draw_calls(world, assets);

        assets.upload_materials();

        let draw_calls: Vec<DrawCall> = ecs_calls
            .iter()
            .filter_map(|dc| {
                let gpu = assets.get_gpu_mesh(dc.mesh)?;
                let model = dc.transform.matrix();
                if !transform_aabb(&gpu.aabb, model).intersects_frustum(&frustum) {
                    return None;
                }
                Some(DrawCall {
                    gpu_mesh: gpu,
                    transform: &dc.transform,
                    material: dc.material,
                    shader: dc.shader,
                })
            })
            .collect();

        let shadow_calls: Vec<DrawCall> = ecs_calls
            .iter()
            .filter_map(|dc| {
                let gpu = assets.get_gpu_mesh(dc.mesh)?;
                Some(DrawCall {
                    gpu_mesh: gpu,
                    transform: &dc.transform,
                    material: dc.material,
                    shader: dc.shader,
                })
            })
            .collect();

        let light_view_proj = compute_light_view_proj(
            lighting.directional.direction[0..3].try_into()?,
            Vec3::new(0.0, 2.0, 0.0),
            20.0,
        );

        unsafe {
            puffin::profile_scope!("wait_for_fences");
            device.wait_for_fences(&[frame.render_fence], true, u64::MAX)?;
        }

        let (image_index, suboptimal) = match unsafe {
            self.swapchain_loader.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                frame.image_available,
                vk::Fence::null(),
            )
        } {
            Ok(r) => r,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
            Err(e) => return Err(e.into()),
        };

        unsafe { device.reset_fences(&[frame.render_fence])? };
        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }

        let mut frame_ctx = FrameCtx {
            device,
            draw_calls,
            shadow_calls,
            camera,
            view_proj,
            light_view_proj,
            lighting,
            swapchain_image: swapchain.images[image_index as usize],
            swapchain_view: swapchain.image_views[image_index as usize],
            swapchain_extent: swapchain.extent,
            assets,
            egui,
            egui_output: Some(egui_output),
            graphics_queue: ctx.device.graphics_queue,
            command_pool: self.commands.pool,
            window: window as *const winit::window::Window,
            exposure: self.exposure,
            fxaa_enabled: self.fxaa_enabled,
            clear_color,
        };

        {
            puffin::profile_scope!("graph_execute");
            self.graph.execute(device, cmd, frame_ctx.as_ptr())?;
        }

        unsafe { device.end_command_buffer(cmd)? };

        let wait_semaphores = [frame.image_available];
        let signal_semaphores = [frame.render_finished];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];

        unsafe {
            puffin::profile_scope!("queue_submit");
            device.queue_submit(
                ctx.device.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&[cmd])
                    .signal_semaphores(&signal_semaphores)],
                frame.render_fence,
            )?;
        }

        let needs_recreate = match unsafe {
            puffin::profile_scope!("queue_present");
            self.swapchain_loader.queue_present(
                ctx.device.present_queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&[swapchain.handle])
                    .image_indices(&[image_index]),
            )
        } {
            Ok(false) => false,
            Ok(true) => true,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
            Err(e) => return Err(e.into()),
        };

        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT as usize;
        Ok(needs_recreate || suboptimal)
    }

    pub fn resize_output(&mut self, new_w: u32, new_h: u32) -> anyhow::Result<()> {
        unsafe { self.device.handle.device_wait_idle()? };
        self.graph.resize_output((new_w, new_h))
    }

    pub fn resize_internal(&mut self, new_w: u32, new_h: u32) -> anyhow::Result<()> {
        unsafe { self.device.handle.device_wait_idle()? };
        self.graph.resize_internal((new_w, new_h))
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe { self.device.handle.device_wait_idle().ok() };
    }
}
