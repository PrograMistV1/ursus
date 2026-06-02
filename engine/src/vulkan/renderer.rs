use super::{
    passes::geometry::{DrawCall, GeometryPass},
    passes::post_process::PostProcessPass,
    Device, VulkanContext,
};
use crate::assets::AssetServer;
use crate::vulkan::core::commands::Commands;
use crate::vulkan::core::sync::FrameSync;
use crate::vulkan::passes::lighting::LightingPass;
use crate::vulkan::passes::shadow::{ShadowDrawCall, ShadowPass};
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::resources::depth::DepthBuffer;
use crate::vulkan::resources::gbuffer::GBuffer;
use crate::vulkan::resources::render_target::RenderTarget;
use crate::vulkan::resources::shadow_map::ShadowMap;
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

pub struct Renderer {
    pub shadow_pass: ShadowPass,
    pub shadow_map: ShadowMap,
    shadow_sampler: vk::Sampler,
    pub geometry: GeometryPass,
    pub lighting: LightingPass,
    pub post_process: PostProcessPass,
    pub ui: UiPass,
    pub commands: Commands,
    gbuffer: GBuffer,
    render_target: RenderTarget,
    depth: DepthBuffer,
    frames: Vec<FrameSync>,
    current_frame: usize,
    swapchain_loader: ash::khr::swapchain::Device,
    device: Arc<Device>,
}

impl Renderer {
    pub fn new(ctx: &VulkanContext, assets: &mut AssetServer) -> anyhow::Result<Self> {
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let w = swapchain.extent.width;
        let h = swapchain.extent.height;

        let render_target = RenderTarget::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            w,
            h,
        )?;
        let depth = DepthBuffer::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            w,
            h,
        )?;

        let shadow_pass = ShadowPass::new(&ctx.device.handle)?;

        let shadow_map = ShadowMap::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
        )?;

        let geometry = GeometryPass::new(
            &ctx.device.handle,
            GBuffer::color_formats(),
            assets.bindless.layout,
            assets.material_buffer.layout,
            assets,
        )?;

        let post_process =
            PostProcessPass::new(&ctx.device.handle, swapchain.format, &render_target)?;

        let ui = UiPass;

        let frames: Vec<_> = (0..FRAMES_IN_FLIGHT)
            .map(|_| FrameSync::new(&ctx.device.handle))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let commands = Commands::new(
            &ctx.device.handle,
            ctx.device.graphics_family,
            FRAMES_IN_FLIGHT,
        )?;
        let swapchain_loader =
            ash::khr::swapchain::Device::new(&ctx.instance.handle, &ctx.device.handle);

        let gbuffer = GBuffer::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            w,
            h,
        )?;
        let lighting = LightingPass::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            &gbuffer,
            &depth,
            render_target.format,
        )?;

        let shadow_sampler = unsafe {
            ctx.device.handle.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
                    .compare_enable(true)
                    .compare_op(vk::CompareOp::LESS_OR_EQUAL),
                None,
            )?
        };
        lighting.bind_shadow_map(&shadow_map, shadow_sampler);

        Ok(Self {
            shadow_pass,
            shadow_map,
            shadow_sampler,
            geometry,
            lighting,
            post_process,
            ui,
            commands,
            gbuffer,
            render_target,
            depth,
            frames,
            current_frame: 0,
            swapchain_loader,
            device: ctx.device.clone(),
        })
    }

    pub fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        clear_color: [f32; 4],
        camera: &Camera,
        draw_calls: &[DrawCall<'_>],
        shadow_calls: &[DrawCall<'_>],
        assets: &AssetServer,
        window: &winit::window::Window,
        egui: &mut crate::egui_layer::EguiLayer,
        egui_output: egui::FullOutput,
        light_view_proj: Mat4,
    ) -> anyhow::Result<bool> {
        puffin::profile_function!();
        let frame = &self.frames[self.current_frame];
        let cmd = self.commands.buffers[self.current_frame];
        let device = &ctx.device.handle;
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let aspect = swapchain.extent.width as f32 / swapchain.extent.height as f32;
        let view_proj = camera.view_proj(aspect);

        assets.upload_materials();

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
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
            Err(e) => return Err(e.into()),
        };

        puffin::profile_scope!("record_commands");
        unsafe {
            device.reset_fences(&[frame.render_fence])?;
        }

        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            {
                puffin::profile_scope!("shadow_pass");
                let shadow_draw_calls: Vec<ShadowDrawCall> = shadow_calls
                    .iter()
                    .map(|dc| ShadowDrawCall {
                        gpu_mesh: dc.gpu_mesh,
                        transform: dc.transform,
                    })
                    .collect();
                self.shadow_pass.record(
                    device,
                    cmd,
                    &self.shadow_map,
                    light_view_proj,
                    &shadow_draw_calls,
                );
            }

            {
                puffin::profile_scope!("geometry_pass");
                self.geometry.record(
                    device,
                    cmd,
                    &self.gbuffer,
                    &self.depth,
                    clear_color,
                    view_proj,
                    draw_calls,
                    assets,
                );
            }

            {
                puffin::profile_scope!("lighting_pass");
                self.lighting
                    .record(device, cmd, &self.render_target, camera, swapchain.extent);
            }

            {
                puffin::profile_scope!("post_process_pass");
                self.post_process.record(
                    device,
                    cmd,
                    swapchain.images[image_index as usize],
                    swapchain.image_views[image_index as usize],
                    swapchain.extent,
                );
            }

            {
                puffin::profile_scope!("ui_pass");
                self.ui.record(
                    device,
                    cmd,
                    swapchain.images[image_index as usize],
                    swapchain.image_views[image_index as usize],
                    swapchain.extent,
                    window,
                    egui,
                    egui_output,
                    ctx.device.graphics_queue,
                    self.commands.pool,
                )?;
            }

            device.end_command_buffer(cmd)?;
        }

        let wait_semaphores = [frame.image_available];
        let signal_semaphores = [frame.render_finished];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let cmds = [cmd];

        unsafe {
            puffin::profile_scope!("queue_submit");
            device.queue_submit(
                ctx.device.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&cmds)
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

    pub fn resize(&mut self, ctx: &VulkanContext) -> anyhow::Result<()> {
        unsafe { self.device.handle.device_wait_idle()? };

        let swapchain = ctx.swapchain.as_ref().unwrap();
        let w = swapchain.extent.width;
        let h = swapchain.extent.height;

        self.render_target = RenderTarget::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            w,
            h,
        )?;
        self.depth = DepthBuffer::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            w,
            h,
        )?;
        self.gbuffer = GBuffer::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            w,
            h,
        )?;
        self.lighting = LightingPass::new(
            &ctx.device.handle,
            ctx.device.physical,
            &ctx.instance.handle,
            &self.gbuffer,
            &self.depth,
            self.render_target.format,
        )?;
        self.lighting
            .bind_shadow_map(&self.shadow_map, self.shadow_sampler);
        self.post_process =
            PostProcessPass::new(&ctx.device.handle, swapchain.format, &self.render_target)?;

        log::debug!("Renderer resized to {}x{}", w, h);
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.device_wait_idle().ok();
            self.device
                .handle
                .destroy_sampler(self.shadow_sampler, None);
        };
    }
}
