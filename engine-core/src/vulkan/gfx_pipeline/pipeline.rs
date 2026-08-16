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

impl<'a> PipelineDesc<'a> {
    pub fn standard(
        vert_spv: &'a [u8],
        frag_spv: &'a [u8],
        color_formats: &'a [Format],
        vertex_layout: &'a VertexLayout,
        push_constant_ranges: &'a [PushConstantRange],
    ) -> Self {
        Self {
            vert_spv,
            frag_spv,
            color_formats,
            depth_format: Some(Format::Depth32Float),
            cull_mode: CullMode::None,
            depth_test: true,
            depth_write: true,
            depth_compare: CompareOp::Less,
            vertex_layout,
            push_constant_ranges,
            blend_attachments: None,
        }
    }

    pub fn with_depth_equal(
        vert_spv: &'a [u8],
        frag_spv: &'a [u8],
        color_formats: &'a [Format],
        vertex_layout: &'a VertexLayout,
        push_constant_ranges: &'a [PushConstantRange],
    ) -> Self {
        Self {
            vert_spv,
            frag_spv,
            color_formats,
            depth_format: Some(Format::Depth32Float),
            cull_mode: CullMode::None,
            depth_test: true,
            depth_write: false,
            depth_compare: CompareOp::Equal,
            vertex_layout,
            push_constant_ranges,
            blend_attachments: None,
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
