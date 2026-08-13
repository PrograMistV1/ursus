use ash::vk;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PipelineId(pub(crate) u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SamplerId(pub(crate) u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DescriptorSetId(pub(crate) u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    VertexFragment,
}

impl ShaderStage {
    pub(crate) fn to_vk(self) -> vk::ShaderStageFlags {
        match self {
            Self::Vertex => vk::ShaderStageFlags::VERTEX,
            Self::Fragment => vk::ShaderStageFlags::FRAGMENT,
            Self::VertexFragment => vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PushConstantRange {
    pub stage: ShaderStage,
    pub size: u32,
}

impl PushConstantRange {
    pub fn of<T: bytemuck::Pod>(stage: ShaderStage) -> Self {
        Self { stage, size: size_of::<T>() as u32 }
    }

    pub(crate) fn to_vk(self) -> vk::PushConstantRange {
        vk::PushConstantRange::default().stage_flags(self.stage.to_vk()).offset(0).size(self.size)
    }
}
