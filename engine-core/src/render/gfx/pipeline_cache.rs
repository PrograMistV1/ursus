use crate::render::gfx::handles::PipelineId;
use crate::render::gfx::{Format, VertexLayout};
use crate::vulkan::gfx_pipeline::builder::PipelineBuilder;
use crate::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use ash::vk;

pub(crate) struct StoredPipeline {
    pub handle: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

#[derive(Default)]
pub struct PipelineCache {
    pipelines: Vec<StoredPipeline>,
    device: Option<ash::Device>,
}

impl PipelineCache {
    pub fn new(device: ash::Device) -> Self {
        Self { pipelines: Vec::new(), device: Some(device) }
    }

    pub(crate) fn insert(&mut self, handle: vk::Pipeline, layout: vk::PipelineLayout) -> PipelineId {
        let id = PipelineId(self.pipelines.len() as u32);
        self.pipelines.push(StoredPipeline { handle, layout });
        id
    }

    pub(crate) fn get(&self, id: PipelineId) -> &StoredPipeline {
        &self.pipelines[id.0 as usize]
    }

    pub fn create_graphics_pipeline(
        &mut self,
        device: &ash::Device,
        desc: &PipelineDesc,
        set_layouts: &[vk::DescriptorSetLayout],
    ) -> anyhow::Result<PipelineId> {
        let binding = desc.vertex_layout.to_vk_binding(0);
        let attributes = desc.vertex_layout.to_vk_attributes(0);

        let (handle, layout) = PipelineBuilder::mesh(
            desc.vert_spv,
            desc.frag_spv,
            desc.color_formats,
            std::slice::from_ref(&binding),
            &attributes,
        )
        .cull_mode(desc.cull_mode)
        .depth_test(desc.depth_test, desc.depth_write)
        .depth_compare(desc.depth_compare)
        .depth_format(desc.depth_format)
        .set_layouts(set_layouts)
        .push_constants(desc.push_constant_ranges)
        .build(device)?;

        Ok(self.insert(handle, layout))
    }

    pub fn create_fullscreen_pipeline(
        &mut self,
        device: &ash::Device,
        vert_spv: &[u8],
        frag_spv: &[u8],
        color_formats: &[Format],
        set_layouts: &[vk::DescriptorSetLayout],
        push_constant_ranges: &[vk::PushConstantRange],
        blend_attachments: Option<&[vk::PipelineColorBlendAttachmentState]>,
    ) -> anyhow::Result<PipelineId> {
        let mut builder = PipelineBuilder::fullscreen(vert_spv, frag_spv, color_formats)
            .set_layouts(set_layouts)
            .push_constants(push_constant_ranges);

        if let Some(blend) = blend_attachments {
            builder = builder.blend_attachments(blend);
        }

        let (handle, layout) = builder.build(device)?;
        Ok(self.insert(handle, layout))
    }

    pub fn create_depth_only_pipeline(
        &mut self,
        device: &ash::Device,
        vert_spv: &[u8],
        vertex_layout: &VertexLayout,
        push_constant_ranges: &[vk::PushConstantRange],
        depth_bias: Option<(f32, f32)>,
    ) -> anyhow::Result<PipelineId> {
        let binding = vertex_layout.to_vk_binding(0);
        let attributes = vertex_layout.to_vk_attributes(0);

        let mut builder = PipelineBuilder::depth_only(vert_spv, std::slice::from_ref(&binding), &attributes)
            .push_constants(push_constant_ranges);

        if let Some((constant, slope)) = depth_bias {
            builder = builder.depth_bias(constant, slope);
        }

        let (handle, layout) = builder.build(device)?;
        Ok(self.insert(handle, layout))
    }

    pub fn layout_of(&self, id: PipelineId) -> vk::PipelineLayout {
        self.get(id).layout
    }
}

impl Drop for PipelineCache {
    fn drop(&mut self) {
        if let Some(device) = &self.device {
            unsafe {
                for p in &self.pipelines {
                    device.destroy_pipeline(p.handle, None);
                    device.destroy_pipeline_layout(p.layout, None);
                }
            }
        }
    }
}
