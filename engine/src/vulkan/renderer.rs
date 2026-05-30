use super::{
    commands::Commands,
    depth::DepthBuffer,
    passes::geometry::{DrawCall, GeometryPass},
    passes::post_process::PostProcessPass,
    render_target::RenderTarget,
    sync::FrameSync,
    Device, VulkanContext,
};
use crate::assets::AssetServer;
use ash::vk;
use glam::{Mat4, Vec3};
use std::sync::Arc;

const FRAMES_IN_FLIGHT: u32 = 2;


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
    pub geometry: GeometryPass,
    pub post_process: PostProcessPass,
    pub commands: Commands,
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
            &ctx.device.handle, ctx.device.physical, &ctx.instance.handle, w, h,
        )?;

        let depth = DepthBuffer::new(
            &ctx.device.handle, ctx.device.physical, &ctx.instance.handle, w, h,
        )?;

        let geometry = GeometryPass::new(
            &ctx.device.handle,
            render_target.format,
            assets.bindless.layout,
            assets.material_buffer.layout,
            assets,
        )?;

        let post_process = PostProcessPass::new(
            &ctx.device.handle,
            swapchain.format,
            &render_target,
        )?;

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

        Ok(Self {
            geometry,
            post_process,
            commands,
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
        assets: &AssetServer,
    ) -> anyhow::Result<()> {
        let frame = &self.frames[self.current_frame];
        let cmd = self.commands.buffers[self.current_frame];
        let device = &ctx.device.handle;
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let aspect = swapchain.extent.width as f32 / swapchain.extent.height as f32;
        let view_proj = camera.view_proj(aspect);

        assets.upload_materials();

        unsafe {
            device.wait_for_fences(&[frame.render_fence], true, u64::MAX)?;
            device.reset_fences(&[frame.render_fence])?;
        }

        let (image_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                swapchain.handle, u64::MAX,
                frame.image_available, vk::Fence::null(),
            )?
        };

        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;

            self.geometry.record(
                device, cmd,
                &self.render_target,
                &self.depth,
                clear_color,
                view_proj,
                draw_calls,
                assets,
            );

            self.post_process.record(
                device, cmd,
                swapchain.images[image_index as usize],
                swapchain.image_views[image_index as usize],
                swapchain.extent,
            );

            device.end_command_buffer(cmd)?;
        }

        let wait_semaphores = [frame.image_available];
        let signal_semaphores = [frame.render_finished];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let cmds = [cmd];

        unsafe {
            device.queue_submit(
                ctx.device.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&cmds)
                    .signal_semaphores(&signal_semaphores)],
                frame.render_fence,
            )?;

            self.swapchain_loader.queue_present(
                ctx.device.present_queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&[swapchain.handle])
                    .image_indices(&[image_index]),
            )?;
        }

        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT as usize;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe { self.device.handle.device_wait_idle().ok() };
    }
}