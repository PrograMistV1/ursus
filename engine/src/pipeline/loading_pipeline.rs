use crate::assets::LoadProgress;
use crate::egui_layer::EguiLayer;
use crate::vulkan::core::sync::FrameSync;
use crate::vulkan::pipeline::shader::ShaderModule;
use crate::vulkan::VulkanContext;
use ash::vk;

#[repr(C)]
struct LoadingPC {
    time: f32,
    progress: f32,
    width: f32,
    height: f32,
}

pub struct LoadingPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    frame_sync: FrameSync,
    swapchain_loader: ash::khr::swapchain::Device,
    start_time: std::time::Instant,
    device: ash::Device,
}

impl LoadingPipeline {
    pub fn new(ctx: &VulkanContext, swapchain_format: vk::Format) -> anyhow::Result<Self> {
        let device = &ctx.device.handle;

        let vert = ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")),
        )?;
        let frag = ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/loading_pipeline.frag.spv")),
        )?;

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<LoadingPC>() as u32);

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )?
        };

        let pipeline = build_pipeline(device, &vert, &frag, layout, swapchain_format)?;

        let frame_sync = FrameSync::new(device)?;

        let swapchain_loader =
            ash::khr::swapchain::Device::new(&ctx.instance.handle, &ctx.device.handle);

        Ok(Self {
            pipeline,
            layout,
            frame_sync,
            swapchain_loader,
            start_time: std::time::Instant::now(),
            device: device.clone(),
        })
    }

    pub fn render(
        &mut self,
        ctx: &VulkanContext,
        egui: &mut EguiLayer,
        egui_output: egui::FullOutput,
        window: &winit::window::Window,
        progress: &LoadProgress,
    ) -> anyhow::Result<()> {
        let device = &ctx.device.handle;
        let swapchain = ctx.swapchain.as_ref().unwrap();

        unsafe {
            device.wait_for_fences(&[self.frame_sync.render_fence], true, u64::MAX)?;
        }

        let (image_index, _suboptimal) = match unsafe {
            self.swapchain_loader.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                self.frame_sync.image_available,
                vk::Fence::null(),
            )
        } {
            Ok(r) => r,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        unsafe { device.reset_fences(&[self.frame_sync.render_fence])? };

        let cmd_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(ctx.device.graphics_family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )?
        };

        let cmd = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(cmd_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0]
        };

        unsafe {
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }

        let swapchain_image = swapchain.images[image_index as usize];
        let swapchain_view = swapchain.image_views[image_index as usize];
        let extent = swapchain.extent;

        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        self.record_background(device, cmd, swapchain_view, extent, progress);

        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(swapchain_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&color_attachment)),
            );
            device.cmd_set_viewport(
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
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                }],
            );
        }

        egui.end_frame_and_draw(
            window,
            ctx.device.graphics_queue,
            cmd_pool,
            cmd,
            extent,
            egui_output,
        )?;

        unsafe {
            device.cmd_end_rendering(cmd);
        }

        transition_image(
            device,
            cmd,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        unsafe {
            device.end_command_buffer(cmd)?;

            device.queue_submit(
                ctx.device.graphics_queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&[self.frame_sync.image_available])
                    .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
                    .command_buffers(&[cmd])
                    .signal_semaphores(&[self.frame_sync.render_finished])],
                self.frame_sync.render_fence,
            )?;

            self.swapchain_loader
                .queue_present(
                    ctx.device.present_queue,
                    &vk::PresentInfoKHR::default()
                        .wait_semaphores(&[self.frame_sync.render_finished])
                        .swapchains(&[swapchain.handle])
                        .image_indices(&[image_index]),
                )
                .ok();

            device.wait_for_fences(&[self.frame_sync.render_fence], true, u64::MAX)?;
            device.destroy_command_pool(cmd_pool, None);
        }

        Ok(())
    }

    fn record_background(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        view: vk::ImageView,
        extent: vk::Extent2D,
        progress: &LoadProgress,
    ) {
        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.05, 0.05, 0.08, 1.0],
                    },
                });

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&color_attachment)),
            );

            device.cmd_set_viewport(
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
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                }],
            );

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            let pc = LoadingPC {
                time: self.start_time.elapsed().as_secs_f32(),
                progress: progress.fraction(),
                width: extent.width as f32,
                height: extent.height as f32,
            };
            let pc_bytes = std::slice::from_raw_parts(
                &pc as *const LoadingPC as *const u8,
                std::mem::size_of::<LoadingPC>(),
            );
            device.cmd_push_constants(
                cmd,
                self.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                pc_bytes,
            );

            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
        }
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

fn build_pipeline(
    device: &ash::Device,
    vert: &ShaderModule,
    frag: &ShaderModule,
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
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&blend_attachment));
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(std::slice::from_ref(&color_format));

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
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&pipeline_info),
                None,
            )
            .map_err(|(_, e)| e)?[0]
    };

    Ok(pipeline)
}

fn transition_image(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
) {
    let (src_stage, src_access, dst_stage, dst_access) = match (from, to) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::PRESENT_SRC_KHR) => (
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            vk::AccessFlags2::empty(),
        ),
        (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        _ => return,
    };

    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(from)
        .new_layout(to)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    unsafe {
        device.cmd_pipeline_barrier2(
            cmd,
            &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier)),
        );
    }
}
