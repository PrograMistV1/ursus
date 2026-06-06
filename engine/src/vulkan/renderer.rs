use crate::assets::AssetServer;
use crate::ecs::GameWorld;
use crate::egui_layer::EguiLayer;
use crate::lighting::LightingUbo;
use crate::pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use crate::pipeline::FrameInput;
use crate::render_graph::{RenderGraph, ResourcePool};
use crate::vulkan::core::commands::Commands;
use crate::vulkan::core::sync::FrameSync;
use crate::vulkan::timestamps::GpuFrameTimes;
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

pub trait DynRenderer: Send {
    fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        world: &mut GameWorld,
        assets: &AssetServer,
        camera: &Camera,
        lighting: &LightingUbo,
        egui: &mut EguiLayer,
        egui_output: egui::FullOutput,
        window: &winit::window::Window,
        clear_color: [f32; 4],
    ) -> anyhow::Result<bool>;

    fn resize_output(&mut self, w: u32, h: u32) -> anyhow::Result<()>;
    fn resize_internal(&mut self, w: u32, h: u32) -> anyhow::Result<()>;

    fn last_frame_times(&self) -> Option<&GpuFrameTimes>;

    fn exposure(&self) -> f32;
    fn set_exposure(&mut self, v: f32);

    fn fsr_sharpness(&self) -> f32;
    fn set_fsr_sharpness(&mut self, v: f32);
}

pub struct Renderer<P: RenderPipeline> {
    pub graph: RenderGraph,
    pub pipeline: P,

    pub commands: Commands,
    pub(crate) frames: Vec<FrameSync>,
    pub(crate) current_frame: usize,
    pub(crate) swapchain_loader: ash::khr::swapchain::Device,
    pub(crate) device: Arc<Device>,
    pub(crate) handles: PipelineHandles,

    pub exposure: f32,
    pub fsr_sharpness: f32,
}

impl<P: RenderPipeline> Renderer<P> {
    pub fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        world: &mut GameWorld,
        assets: &AssetServer,
        camera: &Camera,
        lighting: &LightingUbo,
        egui: &mut EguiLayer,
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

        let light_dir: [f32; 3] = lighting.directional.direction[0..3].try_into()?;
        let light_view_proj = crate::lighting::compute_light_view_proj(light_dir, Vec3::new(0.0, 2.0, 0.0), 20.0);

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

        self.graph.pool.update_external(
            self.handles.swapchain,
            swapchain.images[image_index as usize],
            swapchain.image_views[image_index as usize],
            swapchain.extent,
        );
        self.graph.reset_external_layouts();

        unsafe { device.reset_fences(&[frame.render_fence])? };
        unsafe {
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }

        let input = FrameInput {
            device,
            world,
            assets,
            camera,
            lighting,
            view_proj,
            light_view_proj,
            egui,
            egui_output,
            window,
            graphics_queue: ctx.device.graphics_queue,
            command_pool: self.commands.pool,
            exposure: self.exposure,
            clear_color,
            internal_resolution: self.graph.internal_resolution(),
            output_resolution: self.graph.output_resolution(),
            fsr_sharpness: self.fsr_sharpness,
        };

        {
            puffin::profile_scope!("pipeline_prepare");
            self.pipeline.prepare_frame(&mut self.graph, input)?;
        }

        {
            puffin::profile_scope!("graph_execute");
            self.graph.execute(device, cmd)?;
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
            self.graph.mark_submitted();
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
        self.graph.resize_output((new_w, new_h))?;
        self.pipeline.on_resize(&mut self.graph, new_w, new_h)
    }

    pub fn resize_internal(&mut self, new_w: u32, new_h: u32) -> anyhow::Result<()> {
        unsafe { self.device.handle.device_wait_idle()? };
        self.graph.resize_internal((new_w, new_h))?;
        self.pipeline.on_resize_internal(&mut self.graph, new_w, new_h)
    }
}

impl<P: RenderPipeline> DynRenderer for Renderer<P> {
    fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        world: &mut GameWorld,
        assets: &AssetServer,
        camera: &Camera,
        lighting: &LightingUbo,
        egui: &mut EguiLayer,
        egui_output: egui::FullOutput,
        window: &winit::window::Window,
        clear_color: [f32; 4],
    ) -> anyhow::Result<bool> {
        self.draw_frame(ctx, world, assets, camera, lighting, egui, egui_output, window, clear_color)
    }

    fn resize_output(&mut self, w: u32, h: u32) -> anyhow::Result<()> {
        self.resize_output(w, h)
    }

    fn resize_internal(&mut self, w: u32, h: u32) -> anyhow::Result<()> {
        self.resize_internal(w, h)
    }

    fn last_frame_times(&self) -> Option<&GpuFrameTimes> {
        self.graph.last_frame_times.as_ref()
    }

    fn exposure(&self) -> f32 {
        self.exposure
    }
    fn set_exposure(&mut self, v: f32) {
        self.exposure = v;
    }

    fn fsr_sharpness(&self) -> f32 {
        self.fsr_sharpness
    }
    fn set_fsr_sharpness(&mut self, v: f32) {
        self.fsr_sharpness = v;
    }
}

pub fn build_dyn_renderer<P: RenderPipeline + Default + 'static>(
    ctx: &VulkanContext,
    assets: &mut AssetServer,
    prev_exposure: f32,
    prev_fsr_sharpness: f32,
) -> anyhow::Result<Box<dyn DynRenderer>> {
    let swapchain = ctx.swapchain.as_ref().unwrap();

    let pool = ResourcePool::new(
        ctx.device.handle.clone(),
        ctx.device.physical,
        ctx.instance.handle.clone(),
        ctx.debug_utils.clone(),
    );

    let mut graph = RenderGraph::new(
        pool,
        ctx.device.handle.clone(),
        (1280, 720),
        (swapchain.extent.width, swapchain.extent.height),
        ctx.debug_utils.clone(),
    );

    let handles = P::build(ctx, assets, &mut graph)?;
    graph.allocate()?;
    graph.compile()?;

    let frames: Vec<_> =
        (0..FRAMES_IN_FLIGHT).map(|_| FrameSync::new(&ctx.device.handle)).collect::<anyhow::Result<_>>()?;

    let commands = Commands::new(&ctx.device.handle, ctx.device.graphics_family, FRAMES_IN_FLIGHT)?;

    let swapchain_loader = ash::khr::swapchain::Device::new(&ctx.instance.handle, &ctx.device.handle);

    Ok(Box::new(Renderer::<P> {
        graph,
        pipeline: Default::default(),
        commands,
        frames,
        current_frame: 0,
        swapchain_loader,
        device: ctx.device.clone(),
        handles,
        exposure: prev_exposure,
        fsr_sharpness: prev_fsr_sharpness,
    }))
}

impl<P: RenderPipeline> Drop for Renderer<P> {
    fn drop(&mut self) {
        unsafe { self.device.handle.device_wait_idle().ok() };
    }
}
