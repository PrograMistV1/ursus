use crate::assets::gpu_server::GpuAssetServer;
use crate::render::graph::RenderGraph;
use crate::render::resource::ResourceHandle;
use crate::render::world::RenderWorld;
use crate::vulkan::VulkanContext;

pub struct FrameInput<'a> {
    pub render_world: &'a RenderWorld,
    pub gpu_assets: &'a mut GpuAssetServer,
}

pub trait RenderPipeline: Send + 'static {
    fn build(
        ctx: &VulkanContext,
        gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized;

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()>;
    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn on_resize_internal(&mut self, _graph: &mut RenderGraph, _w: u32, _h: u32) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct PipelineHandles {
    pub swapchain: ResourceHandle,
}
