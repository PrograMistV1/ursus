use crate::vulkan::core::memory::{alloc_image, destroy_image_resources, ImageDesc};
use ash::vk;

pub struct DepthBuffer {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub memory: vk::DeviceMemory,
    pub format: vk::Format,
    device: ash::Device,
}

impl DepthBuffer {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let format = vk::Format::D32_SFLOAT;
        let desc = ImageDesc::depth(
            format,
            width,
            height,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        );
        let img = alloc_image(device, physical_device, instance, &desc)?;

        Ok(Self { image: img.image, view: img.view, memory: img.memory, format, device: device.clone() })
    }
}

impl Drop for DepthBuffer {
    fn drop(&mut self) {
        unsafe { destroy_image_resources(&self.device, self.image, self.view, self.memory) }
    }
}
