use crate::render::gfx::types::{BlendState, CompareOp, CullMode, Format, PushConstantRange, VertexLayout};
use ash::vk;

pub struct PipelineDesc<'a> {
    pub vert_spv: &'a [u8],
    pub frag_spv: &'a [u8],
    pub color_formats: &'a [Format],
    pub depth_format: Option<Format>,
    pub cull_mode: CullMode,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_compare: CompareOp,
    pub vertex_layout: &'a VertexLayout,
    pub push_constant_ranges: &'a [PushConstantRange],
    pub blend_attachments: Option<&'a [BlendState]>,
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
