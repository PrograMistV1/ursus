use crate::vulkan::core::memory::{alloc_image, destroy_image_resources, ImageDesc};
use ash::vk;

pub struct RenderTarget {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub memory: vk::DeviceMemory,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    device: ash::Device,
}

impl RenderTarget {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let format = vk::Format::R16G16B16A16_SFLOAT;
        let desc = ImageDesc::color(
            format,
            width,
            height,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        );
        let img = alloc_image(device, physical_device, instance, &desc)?;

        log::debug!("RenderTarget: {}x{} {:?}", width, height, format);
        Ok(Self {
            image: img.image,
            view: img.view,
            memory: img.memory,
            format,
            extent: vk::Extent2D { width, height },
            device: device.clone(),
        })
    }
}

impl Drop for RenderTarget {
    fn drop(&mut self) {
        unsafe { destroy_image_resources(&self.device, self.image, self.view, self.memory) }
    }
}
