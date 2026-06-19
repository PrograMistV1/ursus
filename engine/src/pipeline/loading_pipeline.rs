use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::CpuAssetServer;
use crate::pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render_graph::{pass, ExternalImageDesc, RenderGraph, ResourceKind};
use crate::vulkan::pipeline::builder::PipelineBuilder;
use crate::vulkan::VulkanContext;
use ash::vk;

#[repr(C)]
struct LoadingPC {
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
}

struct LoadingFrameData {
    device: *const ash::Device,
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
}
unsafe impl Send for LoadingFrameData {}

struct PendingState {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    device: ash::Device,
}
thread_local! {
    static PENDING: std::cell::RefCell<Option<PendingState>> =
        std::cell::RefCell::new(None);
}

pub struct LoadingPipeline {
    start_time: std::time::Instant,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    device: ash::Device,
}

impl Default for LoadingPipeline {
    fn default() -> Self {
        let s = PENDING
            .with(|c| c.borrow_mut().take())
            .expect("LoadingPipeline::default() вызван без предшествующего build()");
        Self { start_time: std::time::Instant::now(), pipeline: s.pipeline, layout: s.layout, device: s.device }
    }
}

impl Drop for LoadingPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

impl RenderPipeline for LoadingPipeline {
    fn build(
        ctx: &VulkanContext,
        cpu_assets: &mut CpuAssetServer,
        _gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized,
    {
        let swapchain = ctx.swapchain.as_ref().unwrap();
        let device = &ctx.device.handle;

        let h_swapchain = graph.pool.register_external(ExternalImageDesc {
            name: "swapchain".into(),
            format: swapchain.format,
            kind: ResourceKind::Color,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
        });

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<LoadingPC>() as u32);

        let handle = cpu_assets.shaders.by_name("loading").expect("шейдер 'loading' не зарегистрирован");
        let (vert_spv, frag_spv) = cpu_assets.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'loading' должен иметь frag").to_vec();

        let (pipeline, layout) =
            PipelineBuilder::fullscreen(&vert_spv, &frag_spv, std::slice::from_ref(&swapchain.format))
                .push_constants(std::slice::from_ref(&push_range))
                .build(device)?;

        PENDING.with(|c| {
            *c.borrow_mut() = Some(PendingState { pipeline, layout, device: device.clone() });
        });

        let device_bg = device.clone();
        pass("loading_background")
            .write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &mut *(ctx_ptr as *mut LoadingFrameData);
                let sc = pool.image(h_swapchain);
                let extent = sc.extent;

                let color_attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(sc.view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.05, 0.05, 0.08, 1.0] } });

                device_bg.cmd_begin_rendering(
                    cmd,
                    &vk::RenderingInfo::default()
                        .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
                        .layer_count(1)
                        .color_attachments(std::slice::from_ref(&color_attachment)),
                );
                device_bg.cmd_set_viewport(
                    cmd,
                    0,
                    &[vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }],
                );
                device_bg.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);

                device_bg.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);

                let pc = LoadingPC { time: data.time, progress: data.progress, width: data.width, height: data.height };
                let pc_bytes = std::slice::from_raw_parts(&pc as *const LoadingPC as *const u8, size_of::<LoadingPC>());
                device_bg.cmd_push_constants(cmd, layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);
                device_bg.cmd_draw(cmd, 3, 1, 0, 0);
                device_bg.cmd_end_rendering(cmd);
                Ok(())
            })
            .build(graph);

        pass("loading_ui")
            .read_write(h_swapchain, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .record(move |cmd, pool, ctx_ptr| unsafe {
                let data = &mut *(ctx_ptr as *mut LoadingFrameData);
                let sc = pool.image(h_swapchain);
                let extent = sc.extent;

                let color_attachment = vk::RenderingAttachmentInfo::default()
                    .image_view(sc.view)
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE);

                (*data.device).cmd_begin_rendering(
                    cmd,
                    &vk::RenderingInfo::default()
                        .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
                        .layer_count(1)
                        .color_attachments(std::slice::from_ref(&color_attachment)),
                );
                (*data.device).cmd_set_viewport(
                    cmd,
                    0,
                    &[vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }],
                );
                (*data.device).cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);

                (*data.device).cmd_end_rendering(cmd);
                Ok(())
            })
            .build(graph);

        Ok(PipelineHandles { swapchain: h_swapchain })
    }

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()> {
        let (w, h) = input.output_resolution;

        graph.set_frame_data(Box::new(LoadingFrameData {
            device: input.device,
            time: self.start_time.elapsed().as_secs_f32(),
            progress: input.cpu_assets.load_progress.fraction(),
            width: w as f32,
            height: h as f32,
        }));

        Ok(())
    }

    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
}
