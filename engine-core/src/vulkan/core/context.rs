use ash::vk;

/// A set of handles required for any one-shot GPU operation
/// (memory allocation, one-shot commands). Groups the handles that are typically
/// passed around together throughout the `vulkan/` module.
#[derive(Clone, Copy)]
pub struct DeviceContext<'a> {
    pub device: &'a ash::Device,
    pub physical_device: vk::PhysicalDevice,
    pub instance: &'a ash::Instance,
}

/// Command pool + queue for submitting one-shot commands (staging uploads,
/// mipmap generation, etc.).
#[derive(Clone, Copy)]
pub struct SubmitContext {
    pub command_pool: vk::CommandPool,
    pub queue: vk::Queue,
}
