use super::shader::ShaderModule;
use ash::vk;

pub struct Pipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    device: ash::Device,
}

impl Pipeline {
    pub fn new_mesh(
        device: &ash::Device,
        color_format: vk::Format,
        bindless_layout: vk::DescriptorSetLayout,
        material_layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<Self> {
        let vert = ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")),
        )?;
        let frag = ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")),
        )?;

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

        let binding = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(32)
            .input_rate(vk::VertexInputRate::VERTEX);

        let attributes = [
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12),
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(2)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(24),
        ];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(std::slice::from_ref(&binding))
            .vertex_attribute_descriptions(&attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
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

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(132);

        let set_layouts = [bindless_layout, material_layout];

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )?
        };

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

        let handle = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .map_err(|(_, e)| e)?[0]
        };

        drop(vert);
        drop(frag);

        log::info!("Mesh pipeline создан (bindless + material buffer)");
        Ok(Self {
            handle,
            layout,
            device: device.clone(),
        })
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.handle, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
