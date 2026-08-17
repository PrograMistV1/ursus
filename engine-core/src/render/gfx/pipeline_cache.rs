use crate::render::gfx::types::handles::PipelineId;
use crate::render::gfx::types::Format;
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

        let depth_format_vk = desc.depth.format.map(Format::to_vk).unwrap_or(vk::Format::UNDEFINED);

        let vk_blend: Option<Vec<vk::PipelineColorBlendAttachmentState>> =
            desc.blend_attachments.map(|states| states.iter().map(|s| s.to_vk()).collect());

        let mut builder = PipelineBuilder::mesh(
            desc.vert_spv,
            desc.frag_spv,
            desc.color_formats,
            std::slice::from_ref(&binding),
            &attributes,
        )
        .cull_mode(desc.cull_mode.to_vk())
        .depth_test(desc.depth.test, desc.depth.write)
        .depth_compare(desc.depth.compare.to_vk())
        .depth_format(depth_format_vk)
        .set_layouts(set_layouts)
        .push_constants(desc.push_constant_ranges);

        if let Some(blend) = vk_blend.as_deref() {
            builder = builder.blend_attachments(blend);
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
