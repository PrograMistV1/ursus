use crate::render::gfx::Format;
use crate::vulkan::core::memory::{alloc_image, destroy_image_resources, ImageDesc};
use ash::vk;

pub struct GBuffer {
    pub albedo: vk::Image,
    pub albedo_view: vk::ImageView,
    pub albedo_memory: vk::DeviceMemory,

    pub normal: vk::Image,
    pub normal_view: vk::ImageView,
    pub normal_memory: vk::DeviceMemory,

    pub extent: vk::Extent2D,
    device: ash::Device,
}

impl GBuffer {
    pub const ALBEDO_FORMAT: Format = Format::Rgba8Unorm;
    pub const NORMAL_FORMAT: Format = Format::Rgba16Float;

    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let extent = vk::Extent2D { width, height };
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;

        let albedo = alloc_image(
            device,
            physical_device,
            instance,
            &ImageDesc::color(Self::ALBEDO_FORMAT.to_vk(), width, height, usage),
        )?;
        let normal = alloc_image(
            device,
            physical_device,
            instance,
            &ImageDesc::color(Self::NORMAL_FORMAT.to_vk(), width, height, usage),
        )?;

        log::debug!("GBuffer: {}x{}", width, height);
        Ok(Self {
            albedo: albedo.image,
            albedo_view: albedo.view,
            albedo_memory: albedo.memory,
            normal: normal.image,
            normal_view: normal.view,
            normal_memory: normal.memory,
            extent,
            device: device.clone(),
        })
    }

    pub fn color_formats() -> [Format; 2] {
        [Self::ALBEDO_FORMAT, Self::NORMAL_FORMAT]
    }
}

impl Drop for GBuffer {
    fn drop(&mut self) {
        unsafe {
            destroy_image_resources(&self.device, self.albedo, self.albedo_view, self.albedo_memory);
            destroy_image_resources(&self.device, self.normal, self.normal_view, self.normal_memory);
        }
    }
}
