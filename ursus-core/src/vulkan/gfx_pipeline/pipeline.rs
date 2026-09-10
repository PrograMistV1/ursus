use crate::render::gfx::types::{BlendState, CompareOp, CullMode, Format, PushConstantRange, VertexLayout};
use ash::vk;

/// Descriptor for building a graphics pipeline via [`PipelineCache::create_graphics_pipeline`].
pub struct PipelineDesc<'a> {
    pub(crate) vert_spv: &'a [u8],
    pub(crate) frag_spv: &'a [u8],
    pub(crate) color_formats: &'a [Format],
    pub(crate) vertex_layout: &'a VertexLayout,
    pub(crate) push_constant_ranges: &'a [PushConstantRange],
    pub(crate) depth: DepthState,
    pub(crate) cull_mode: CullMode,
    pub(crate) blend_attachments: Option<&'a [BlendState]>,
}

#[derive(Clone, Copy)]
pub(crate) struct DepthState {
    pub format: Option<Format>,
    pub test: bool,
    pub write: bool,
    pub compare: CompareOp,
}

impl<'a> PipelineDesc<'a> {
    /// Creates a pipeline descriptor with sensible defaults:
    /// - `depth_format = NONE`
    /// - `depth_test = true`
    /// - `depth_write = true`
    /// - `depth_compare = LESS`
    /// - `cull_mode = NONE`
    /// - `blend_attachments = NONE`
    pub fn new(
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
            vertex_layout,
            push_constant_ranges,
            depth: DepthState { format: None, test: true, write: true, compare: CompareOp::Less },
            cull_mode: CullMode::None,
            blend_attachments: None,
        }
    }

    pub fn depth_format(mut self, format: Format) -> Self {
        self.depth.format = Some(format);
        self
    }

    pub fn depth_test(mut self, test: bool) -> Self {
        self.depth.test = test;
        self
    }

    pub fn depth_write(mut self, write: bool) -> Self {
        self.depth.write = write;
        self
    }

    pub fn depth_compare(mut self, compare: CompareOp) -> Self {
        self.depth.compare = compare;
        self
    }

    pub fn cull_mode(mut self, cull_mode: CullMode) -> Self {
        self.cull_mode = cull_mode;
        self
    }

    pub fn blend_attachments(mut self, states: &'a [BlendState]) -> Self {
        self.blend_attachments = Some(states);
        self
    }
    /*
    pub(crate) fn vert_spv(&self) -> &'a [u8] {
        self.vert_spv
    }
    pub(crate) fn frag_spv(&self) -> &'a [u8] {
        self.frag_spv
    }
    pub(crate) fn color_formats(&self) -> &'a [Format] {
        self.color_formats
    }
    pub(crate) fn vertex_layout(&self) -> &'a VertexLayout {
        self.vertex_layout
    }
    pub(crate) fn push_constant_ranges(&self) -> &'a [PushConstantRange] {
        self.push_constant_ranges
    }
    pub(crate) fn depth_format_value(&self) -> Option<Format> {
        self.depth.format
    }
    pub(crate) fn depth_test_value(&self) -> bool {
        self.depth.test
    }
    pub(crate) fn depth_write_value(&self) -> bool {
        self.depth.write
    }
    pub(crate) fn depth_compare_value(&self) -> CompareOp {
        self.depth.compare
    }
    pub(crate) fn cull_mode_value(&self) -> CullMode {
        self.cull_mode
    }

    pub(crate) fn blend_attachments_value(&self) -> Option<&'a [BlendState]> {
        self.blend_attachments
    }*/
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
