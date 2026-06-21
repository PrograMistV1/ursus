use crate::assets::mesh::Vertex;
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
            depth_compare: vk::CompareOp::LESS,
        }
    }

    pub fn with_depth_equal(vert_spv: &'a [u8], frag_spv: &'a [u8], color_formats: &'a [vk::Format]) -> Self {
        Self {
            vert_spv,
            frag_spv,
            color_formats,
            depth_format: vk::Format::D32_SFLOAT,
            cull_mode: vk::CullModeFlags::NONE,
            depth_test: true,
            depth_write: false,
            depth_compare: vk::CompareOp::EQUAL,
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
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<crate::vulkan::passes::geometry::MeshPushConstants>() as u32);

        let binding = Vertex::binding_description();
        let attributes = Vertex::attribute_descriptions();

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
        .push_constants(std::slice::from_ref(&push_range))
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
