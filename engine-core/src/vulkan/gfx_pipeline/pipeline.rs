use crate::render::gfx::format::Format;
use ash::vk;

pub struct PipelineDesc<'a> {
    pub vert_spv: &'a [u8],
    pub frag_spv: &'a [u8],
    pub color_formats: &'a [Format],
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
        color_formats: &'a [Format],
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
        color_formats: &'a [Format],
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

impl Drop for Pipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.handle, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}
