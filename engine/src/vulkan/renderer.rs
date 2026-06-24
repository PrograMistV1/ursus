use crate::assets::gpu_server::GpuAssetServer;
use crate::render::frame_pipeline::render_pipeline::{PipelineHandles, RenderPipeline};
use crate::render::frame_pipeline::FrameInput;
use crate::render::graph::RenderGraph;
use crate::render::resource::ResourcePool;
use crate::render::world::RenderWorld;
use crate::vulkan::core::commands::Commands;
use crate::vulkan::core::sync::FrameSync;
use crate::vulkan::timestamps::GpuFrameTimes;
use crate::vulkan::{Device, VulkanContext};
use ash::vk;
use std::sync::Arc;

const FRAMES_IN_FLIGHT: u32 = 3;

pub trait DynRenderer: Send {
    fn draw_frame(
        &mut self,
        ctx: &VulkanContext,
        render_world: &RenderWorld,
        gpu_assets: &mut GpuAssetServer,
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
    pub(crate) acquire_semaphores: Vec<vk::Semaphore>,
    pub(crate) present_semaphores: Vec<vk::Semaphore>,
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
        render_world: &RenderWorld,
        gpu_assets: &mut GpuAssetServer,
        clear_color: [f32; 4],
    ) -> anyhow::Result<bool> {
        puffin::profile_function!();

        let frame = &self.frames[self.current_frame];
        let cmd = self.commands.buffers[self.current_frame];
        let device = &ctx.device.handle;
        let swapchain = ctx.swapchain.as_ref().unwrap();

        unsafe {
            puffin::profile_scope!("wait_for_fences");
            device.wait_for_fences(&[frame.render_fence], true, u64::MAX)?;
        }

        let acquire_sem = self.acquire_semaphores[self.current_frame];
        let (image_index, suboptimal) = match unsafe {
            self.swapchain_loader.acquire_next_image(swapchain.handle, u64::MAX, acquire_sem, vk::Fence::null())
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
            render_world,
            gpu_assets,
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

        let present_sem = self.present_semaphores[image_index as usize];

        unsafe {
            puffin::profile_scope!("queue_submit");
            let wait_info = vk::SemaphoreSubmitInfo::default()
                .semaphore(acquire_sem)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT);

            let signal_info = vk::SemaphoreSubmitInfo::default()
                .semaphore(present_sem)
                .stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS);

            let cmd_info = vk::CommandBufferSubmitInfo::default().command_buffer(cmd);

            device.queue_submit2(
                ctx.device.graphics_queue,
                &[vk::SubmitInfo2::default()
                    .wait_semaphore_infos(std::slice::from_ref(&wait_info))
                    .command_buffer_infos(std::slice::from_ref(&cmd_info))
                    .signal_semaphore_infos(std::slice::from_ref(&signal_info))],
                frame.render_fence,
            )?;
            self.graph.mark_submitted();
        }

        let needs_recreate = match unsafe {
            puffin::profile_scope!("queue_present");
            let signal_semaphores = [present_sem];
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
        render_world: &RenderWorld,
        gpu_assets: &mut GpuAssetServer,
        clear_color: [f32; 4],
    ) -> anyhow::Result<bool> {
        self.draw_frame(ctx, render_world, gpu_assets, clear_color)
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
    gpu_assets: &mut GpuAssetServer,
    prev_exposure: f32,
    prev_fsr_sharpness: f32,
) -> anyhow::Result<Box<dyn DynRenderer>> {
    let swapchain = ctx.swapchain.as_ref().unwrap();
    let image_count = swapchain.images.len();

    let acquire_semaphores: Vec<vk::Semaphore> = (0..image_count)
        .map(|_| {
            let info = vk::SemaphoreCreateInfo::default();
            unsafe { ctx.device.handle.create_semaphore(&info, None) }
        })
        .collect::<Result<_, _>>()?;

    let present_semaphores: Vec<vk::Semaphore> = (0..image_count)
        .map(|_| unsafe {
            ctx.device.handle.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        })
        .collect::<Result<_, _>>()?;

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

    let handles = P::build(ctx, gpu_assets, &mut graph)?;
    graph.allocate()?;
    graph.compile()?;

    let frames: Vec<_> =
        (0..FRAMES_IN_FLIGHT).map(|_| FrameSync::new(&ctx.device.handle)).collect::<anyhow::Result<_>>()?;

    let commands = Commands::new(&ctx.device.handle, ctx.device.graphics_family, FRAMES_IN_FLIGHT)?;

    graph.enable_timestamps(
        &ctx.device.handle,
        ctx.device.physical,
        &ctx.instance.handle,
        FRAMES_IN_FLIGHT,
        commands.pool,
        ctx.device.graphics_queue,
    )?;

    let swapchain_loader = ash::khr::swapchain::Device::new(&ctx.instance.handle, &ctx.device.handle);

    Ok(Box::new(Renderer::<P> {
        graph,
        pipeline: Default::default(),
        commands,
        frames,
        acquire_semaphores,
        present_semaphores,
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
        unsafe {
            self.device.handle.device_wait_idle().ok();
            for &sem in &self.acquire_semaphores {
                self.device.handle.destroy_semaphore(sem, None);
            }
            for &sem in &self.present_semaphores {
                self.device.handle.destroy_semaphore(sem, None);
            }
        }
    }
}
