use crate::assets::gpu_server::GpuAssetServer;
use crate::render::graph::{pass, RenderGraph};
use crate::render::resource::ResourceHandle;
use crate::render::world::RenderWorld;
use crate::vulkan::VulkanContext;
use ash::vk;

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

#[derive(Default)]
pub struct NoopPipeline;

impl RenderPipeline for NoopPipeline {
    fn build(
        ctx: &VulkanContext,
        _gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized,
    {
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let h_swapchain = graph.pool.register_swapchain_external(swapchain.format);

        pass("noop_clear")
            .write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, _rw, gpu| {
                let sc = pool.image(h_swapchain);
                let attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(sc.view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] } });

                unsafe {
                    gpu.device().cmd_begin_rendering(
                        cmd,
                        &vk::RenderingInfo::default()
                            .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: sc.extent })
                            .layer_count(1)
                            .color_attachments(std::slice::from_ref(&attachment)),
                    );
                    gpu.device().cmd_end_rendering(cmd);
                }
                Ok(())
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }
}
