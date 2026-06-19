use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::CpuAssetServer;
use crate::pipeline::render_pipeline::{FrameInput, PipelineHandles, RenderPipeline};
use crate::render_graph::{pass, ExternalImageDesc, RenderGraph, ResourceKind};
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
        _cpu_assets: &mut CpuAssetServer,
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

        let vert = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")),
        )?;
        let frag = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/loading_pipeline.frag.spv")),
        )?;

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<LoadingPC>() as u32);

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )?
        };

        let pipeline = build_vk_pipeline(device, &vert, &frag, layout, swapchain.format)?;

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

fn build_vk_pipeline(
    device: &ash::Device,
    vert: &crate::vulkan::pipeline::shader::ShaderModule,
    frag: &crate::vulkan::pipeline::shader::ShaderModule,
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> anyhow::Result<vk::Pipeline> {
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert.handle)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag.handle)
            .name(entry),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly =
        vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisampling =
        vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let blend_attachment =
        vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA);
    let color_blending =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(std::slice::from_ref(&blend_attachment));
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let mut rendering_info =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(std::slice::from_ref(&color_format));

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .push_next(&mut rendering_info);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pipeline_info), None)
            .map_err(|(_, e)| e)?[0]
    };
    Ok(pipeline)
}
