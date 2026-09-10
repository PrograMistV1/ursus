use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    Uniform,
    Storage,
}

impl BufferUsage {
    pub(crate) fn to_vk(self) -> vk::BufferUsageFlags {
        match self {
            Self::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
            Self::Storage => vk::BufferUsageFlags::STORAGE_BUFFER,
        }
    }
}
