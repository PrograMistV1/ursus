use crate::assets::cpu_server::CpuAssetServer;
use crate::assets::gpu_server::GpuAssetServer;
use crate::render_graph::{RenderGraph, ResourceHandle};
use crate::render_world::RenderWorld;
use crate::vulkan::VulkanContext;
use ash::vk;

pub struct FrameInput<'a> {
    pub device: &'a ash::Device,
    pub render_world: &'a RenderWorld,
    pub cpu_assets: &'a mut CpuAssetServer,
    pub gpu_assets: &'a mut GpuAssetServer,
    pub graphics_queue: vk::Queue,
    pub command_pool: vk::CommandPool,
    pub exposure: f32,
    pub clear_color: [f32; 4],
    pub internal_resolution: (u32, u32),
    pub output_resolution: (u32, u32),
    pub fsr_sharpness: f32,
}

pub trait RenderPipeline: Send + 'static {
    fn build(
        ctx: &VulkanContext,
        cpu_assets: &mut CpuAssetServer,
        gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized;

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()>;
    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn on_resize_internal(
        &mut self,
        _graph: &mut RenderGraph,
        _new_width: u32,
        _new_height: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct PipelineHandles {
    pub swapchain: ResourceHandle,
}
