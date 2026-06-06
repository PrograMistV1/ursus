use crate::lighting::buffer::LightBuffer;
use crate::render_graph::GpuImage;
use crate::vulkan::Camera;
use ash::vk;

#[repr(C)]
struct LightingPC {
    inv_proj: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    viewport: [f32; 2],
    _pad: [f32; 2],
}

pub struct LightingPass {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_set: vk::DescriptorSet,
    pub sampler: vk::Sampler,
    pub shadow_sampler: vk::Sampler,
    pub light_buffer: LightBuffer,
    device: ash::Device,
}

impl LightingPass {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        hdr_format: vk::Format,
    ) -> anyhow::Result<Self> {
        let light_buffer = LightBuffer::new(device, physical_device, instance)?;

        let sampler = unsafe {
            device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::NEAREST)
                    .min_filter(vk::Filter::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };

        let shadow_sampler = unsafe {
            device.create_sampler(
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

        let bindings = [
            make_image_binding(0), // albedo
            make_image_binding(1), // normal
            make_image_binding(2), // depth
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            make_image_binding(4), // shadow map
        ];

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)?
        };

        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 4 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER, descriptor_count: 1 },
        ];
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default().pool_sizes(&pool_sizes).max_sets(1),
                None,
            )?
        };

        let descriptor_set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(std::slice::from_ref(&descriptor_set_layout)),
            )?[0]
        };

        let buf_info = vk::DescriptorBufferInfo::default()
            .buffer(light_buffer.buffer)
            .offset(0)
            .range(size_of::<crate::lighting::buffer::LightingUbo>() as vk::DeviceSize);

        let ubo_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(3)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(std::slice::from_ref(&buf_info));

        unsafe { device.update_descriptor_sets(&[ubo_write], &[]) };

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<LightingPC>() as u32);

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(std::slice::from_ref(&descriptor_set_layout))
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )?
        };

        let vert = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")),
        )?;
        let frag = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/lighting.frag.spv")),
        )?;

        let pipeline = build_fullscreen_pipeline(device, &vert, &frag, layout, hdr_format)?;

        log::debug!("LightingPass создан");
        Ok(Self {
            pipeline,
            layout,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            sampler,
            shadow_sampler,
            light_buffer,
            device: device.clone(),
        })
    }

    pub fn upload_lights(&self, data: &crate::lighting::buffer::LightingUbo) {
        self.light_buffer.upload(data);
    }

    pub fn record(&self, device: &ash::Device, cmd: vk::CommandBuffer, hdr: &impl GpuImage, camera: &Camera) {
        let extent = hdr.extent();

        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(hdr.view())
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE);

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
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
            device.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[self.descriptor_set],
                &[],
            );

            let aspect = extent.width as f32 / extent.height as f32;
            let view = glam::Mat4::look_at_rh(camera.eye, camera.target, camera.up);
            let mut proj = glam::Mat4::perspective_rh(camera.fov_y, aspect, camera.z_near, camera.z_far);
            proj.y_axis.y *= -1.0;

            let pc = LightingPC {
                inv_proj: proj.inverse().to_cols_array_2d(),
                inv_view: view.inverse().to_cols_array_2d(),
                viewport: [extent.width as f32, extent.height as f32],
                _pad: [0.0; 2],
            };
            let pc_bytes = std::slice::from_raw_parts(&pc as *const LightingPC as *const u8, size_of::<LightingPC>());
            device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);

            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
        }
    }
}

impl Drop for LightingPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_sampler(self.shadow_sampler, None);
        }
    }
}

fn make_image_binding(binding: u32) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
}

fn build_fullscreen_pipeline(
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
    let depth_stencil =
        vk::PipelineDepthStencilStateCreateInfo::default().depth_test_enable(false).depth_write_enable(false);
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
        .depth_stencil_state(&depth_stencil)
        .layout(layout)
        .push_next(&mut rendering_info);
    let pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pipeline_info), None)
            .map_err(|(_, e)| e)?[0]
    };
    Ok(pipeline)
}
