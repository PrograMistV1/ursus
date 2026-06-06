use super::shader::ShaderModule;
use crate::assets::mesh::Vertex;
use ash::vk;

pub struct PipelineDesc<'a> {
    pub vert_spv: &'a [u8],
    pub frag_spv: &'a [u8],
    pub color_formats: &'a [vk::Format],
    pub depth_format: vk::Format,
    pub cull_mode: vk::CullModeFlags,
    pub depth_test: bool,
    pub depth_write: bool,
}

impl<'a> PipelineDesc<'a> {
    pub fn standard(vert_spv: &'a [u8], frag_spv: &'a [u8], color_formats: &'a [vk::Format]) -> Self {
        Self {
            vert_spv,
            frag_spv,
            color_formats,
            depth_format: vk::Format::D32_SFLOAT,
            cull_mode: vk::CullModeFlags::NONE,
            depth_test: true,
            depth_write: true,
        }
    }
}

pub struct Pipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    device: ash::Device,
}

impl Pipeline {
    pub fn new(
        device: &ash::Device,
        desc: &PipelineDesc,
        set_layouts: &[vk::DescriptorSetLayout],
    ) -> anyhow::Result<Self> {
        let vert = ShaderModule::from_bytes(device, desc.vert_spv)?;
        let frag = ShaderModule::from_bytes(device, desc.frag_spv)?;

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
            .stride(size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);

        let attributes = Vertex::attribute_descriptions();

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(std::slice::from_ref(&binding))
            .vertex_attribute_descriptions(&attributes);

        let input_assembly =
            vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(desc.cull_mode)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);

        let multisampling =
            vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let blend_attachments = [
            vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA),
            vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA),
        ];

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<crate::vulkan::passes::geometry::MeshPushConstants>() as u32);

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(set_layouts)
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )?
        };

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(desc.depth_test)
            .depth_write_enable(desc.depth_write)
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(desc.color_formats)
            .depth_attachment_format(desc.depth_format);

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

        let handle = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pipeline_info), None)
                .map_err(|(_, e)| e)?[0]
        };

        Ok(Self { handle, layout, device: device.clone() })
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
