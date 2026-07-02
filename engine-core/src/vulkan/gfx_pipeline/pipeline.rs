use crate::vulkan::gfx_pipeline::builder::PipelineBuilder;
use ash::vk;

pub struct PipelineDesc<'a> {
    pub vert_spv: &'a [u8],
    pub frag_spv: &'a [u8],
    pub color_formats: &'a [vk::Format],
    pub depth_format: vk::Format,
    pub cull_mode: vk::CullModeFlags,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_compare: vk::CompareOp,
    pub vertex_bindings: &'a [vk::VertexInputBindingDescription],
    pub vertex_attributes: &'a [vk::VertexInputAttributeDescription],
    pub push_constant_ranges: &'a [vk::PushConstantRange],
}

impl<'a> PipelineDesc<'a> {
    pub fn standard(
        vert_spv: &'a [u8],
        frag_spv: &'a [u8],
        color_formats: &'a [vk::Format],
        vertex_bindings: &'a [vk::VertexInputBindingDescription],
        vertex_attributes: &'a [vk::VertexInputAttributeDescription],
        push_constant_ranges: &'a [vk::PushConstantRange],
    ) -> Self {
        Self {
            vert_spv,
            frag_spv,
            color_formats,
            depth_format: vk::Format::D32_SFLOAT,
            cull_mode: vk::CullModeFlags::NONE,
            depth_test: true,
            depth_write: true,
            depth_compare: vk::CompareOp::LESS,
            vertex_bindings,
            vertex_attributes,
            push_constant_ranges,
        }
    }

    pub fn with_depth_equal(
        vert_spv: &'a [u8],
        frag_spv: &'a [u8],
        color_formats: &'a [vk::Format],
        vertex_bindings: &'a [vk::VertexInputBindingDescription],
        vertex_attributes: &'a [vk::VertexInputAttributeDescription],
        push_constant_ranges: &'a [vk::PushConstantRange],
    ) -> Self {
        Self {
            vert_spv,
            frag_spv,
            color_formats,
            depth_format: vk::Format::D32_SFLOAT,
            cull_mode: vk::CullModeFlags::NONE,
            depth_test: true,
            depth_write: false,
            depth_compare: vk::CompareOp::EQUAL,
            vertex_bindings,
            vertex_attributes,
            push_constant_ranges,
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
        let (handle, layout) = PipelineBuilder::mesh(
            desc.vert_spv,
            desc.frag_spv,
            desc.color_formats,
            desc.vertex_bindings,
            desc.vertex_attributes,
        )
        .cull_mode(desc.cull_mode)
        .depth_test(desc.depth_test, desc.depth_write)
        .depth_compare(desc.depth_compare)
        .depth_format(desc.depth_format)
        .set_layouts(set_layouts)
        .push_constants(desc.push_constant_ranges)
        .build(device)?;

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
