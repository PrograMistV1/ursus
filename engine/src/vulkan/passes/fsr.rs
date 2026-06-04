use crate::render_graph::GpuImage;
use ash::vk;

#[repr(C)]
pub struct EasuPC {
    pub con0: [u32; 4],
    pub con1: [u32; 4],
    pub con2: [u32; 4],
    pub con3: [u32; 4],
}

#[repr(C)]
pub struct RcasPC {
    pub con0: [u32; 4],
}

pub struct FsrPass {
    pub easu_pipeline: vk::Pipeline,
    pub easu_layout: vk::PipelineLayout,
    pub easu_descriptor_set_layout: vk::DescriptorSetLayout,
    pub easu_descriptor_set: vk::DescriptorSet,

    pub rcas_pipeline: vk::Pipeline,
    pub rcas_layout: vk::PipelineLayout,
    pub rcas_descriptor_set: vk::DescriptorSet,

    pub sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,

    device: ash::Device,
}

impl FsrPass {
    pub fn new(device: &ash::Device, output_format: vk::Format) -> anyhow::Result<Self> {
        let sampler = unsafe {
            device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };

        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 2,
        }];
        let descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&pool_sizes)
                    .max_sets(2),
                None,
            )?
        };

        let dsl = create_sampled_image_dsl(device)?;

        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[dsl, dsl]),
            )?
        };
        let easu_descriptor_set = sets[0];
        let rcas_descriptor_set = sets[1];

        let easu_push = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<EasuPC>() as u32);

        let easu_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(std::slice::from_ref(&dsl))
                    .push_constant_ranges(std::slice::from_ref(&easu_push)),
                None,
            )?
        };

        let rcas_push = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<RcasPC>() as u32);

        let rcas_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(std::slice::from_ref(&dsl))
                    .push_constant_ranges(std::slice::from_ref(&rcas_push)),
                None,
            )?
        };

        let easu_pipeline = build_fullscreen_pipeline(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/fsr_easu.frag.spv")),
            easu_layout,
            output_format,
        )?;

        let rcas_pipeline = build_fullscreen_pipeline(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/fsr_rcas.frag.spv")),
            rcas_layout,
            output_format,
        )?;

        log::debug!("FsrPass создан");

        Ok(Self {
            easu_pipeline,
            easu_layout,
            easu_descriptor_set_layout: dsl,
            easu_descriptor_set,
            rcas_pipeline,
            rcas_layout,
            rcas_descriptor_set,
            sampler,
            descriptor_pool,
            device: device.clone(),
        })
    }

    pub fn record_easu(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        dst: &impl GpuImage,
        easu_pc: &EasuPC,
    ) {
        self.record_fullscreen_pass(
            device,
            cmd,
            dst,
            self.easu_pipeline,
            self.easu_layout,
            self.easu_descriptor_set,
            unsafe {
                std::slice::from_raw_parts(
                    easu_pc as *const EasuPC as *const u8,
                    size_of::<EasuPC>(),
                )
            },
        );
    }

    pub fn record_rcas(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        dst: &impl GpuImage,
        rcas_pc: &RcasPC,
    ) {
        self.record_fullscreen_pass(
            device,
            cmd,
            dst,
            self.rcas_pipeline,
            self.rcas_layout,
            self.rcas_descriptor_set,
            unsafe {
                std::slice::from_raw_parts(
                    rcas_pc as *const RcasPC as *const u8,
                    size_of::<RcasPC>(),
                )
            },
        );
    }

    fn record_fullscreen_pass(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        dst: &impl GpuImage,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        set: vk::DescriptorSet,
        pc_bytes: &[u8],
    ) {
        let extent = dst.extent();
        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(dst.view())
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
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

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[set],
                &[],
            );
            device.cmd_push_constants(cmd, layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);
            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
        }
    }
}

impl Drop for FsrPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.easu_pipeline, None);
            self.device.destroy_pipeline_layout(self.easu_layout, None);
            self.device.destroy_pipeline(self.rcas_pipeline, None);
            self.device.destroy_pipeline_layout(self.rcas_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.easu_descriptor_set_layout, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

fn create_sampled_image_dsl(device: &ash::Device) -> anyhow::Result<vk::DescriptorSetLayout> {
    let binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT);

    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding)),
            None,
        )?
    })
}

fn build_fullscreen_pipeline(
    device: &ash::Device,
    frag_spv: &[u8],
    layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> anyhow::Result<vk::Pipeline> {
    let vert = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(
        device,
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")),
    )?;
    let frag = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(device, frag_spv)?;

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
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);
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
        .depth_stencil_state(&depth_stencil)
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

pub fn compute_easu_con(
    input_viewport: (f32, f32),
    input_size: (f32, f32),
    output_size: (f32, f32),
) -> EasuPC {
    let (ivw, ivh) = input_viewport;
    let (isw, ish) = input_size;
    let (osw, osh) = output_size;

    EasuPC {
        con0: [
            f32_to_bits(ivw / osw),
            f32_to_bits(ivh / osh),
            f32_to_bits(0.5 * ivw / osw - 0.5),
            f32_to_bits(0.5 * ivh / osh - 0.5),
        ],
        con1: [
            f32_to_bits(1.0 / isw),
            f32_to_bits(1.0 / ish),
            f32_to_bits(1.0 / isw),
            f32_to_bits(-1.0 / ish),
        ],
        con2: [
            f32_to_bits(-1.0 / isw),
            f32_to_bits(2.0 / ish),
            f32_to_bits(1.0 / isw),
            f32_to_bits(2.0 / ish),
        ],
        con3: [f32_to_bits(0.0 / isw), f32_to_bits(4.0 / ish), 0, 0],
    }
}

pub fn compute_rcas_con(sharpness: f32) -> RcasPC {
    let sharpness_stop = sharpness.exp2();
    RcasPC {
        con0: [f32_to_bits(sharpness_stop), 0, 0, 0],
    }
}

#[inline]
fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}
