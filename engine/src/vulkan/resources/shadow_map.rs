use crate::vulkan::core::memory::{destroy_image_resources, find_memory_type};
use ash::vk;

pub const SHADOW_MAP_SIZE: u32 = 2048;

pub struct ShadowMap {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub memory: vk::DeviceMemory,
    pub format: vk::Format,
    device: ash::Device,
}

impl ShadowMap {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
    ) -> anyhow::Result<Self> {
        let format = vk::Format::D32_SFLOAT;

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width: SHADOW_MAP_SIZE, height: SHADOW_MAP_SIZE, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { device.create_image(&image_info, None)? };
        let req = unsafe { device.get_image_memory_requirements(image) };

        let mem_type =
            find_memory_type(instance, physical_device, req.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;

        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(mem_type),
                None,
            )?
        };
        unsafe { device.bind_image_memory(image, memory, 0)? };

        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::DEPTH,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )?
        };

        log::debug!("ShadowMap: {}x{}", SHADOW_MAP_SIZE, SHADOW_MAP_SIZE);
        Ok(Self { image, view, memory, format, device: device.clone() })
    }
}

impl Drop for ShadowMap {
    fn drop(&mut self) {
        unsafe { destroy_image_resources(&self.device, self.image, self.view, self.memory) }
    }
}
