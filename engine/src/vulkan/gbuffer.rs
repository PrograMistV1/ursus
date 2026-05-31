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
    pub const ALBEDO_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
    pub const NORMAL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let extent = vk::Extent2D { width, height };

        let (albedo, albedo_view, albedo_memory) = create_attachment(
            device,
            instance,
            physical_device,
            Self::ALBEDO_FORMAT,
            width,
            height,
        )?;

        let (normal, normal_view, normal_memory) = create_attachment(
            device,
            instance,
            physical_device,
            Self::NORMAL_FORMAT,
            width,
            height,
        )?;

        log::info!("GBuffer: {}x{}", width, height);
        Ok(Self {
            albedo,
            albedo_view,
            albedo_memory,
            normal,
            normal_view,
            normal_memory,
            extent,
            device: device.clone(),
        })
    }

    pub fn color_formats() -> [vk::Format; 2] {
        [Self::ALBEDO_FORMAT, Self::NORMAL_FORMAT]
    }
}

impl Drop for GBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.albedo_view, None);
            self.device.destroy_image(self.albedo, None);
            self.device.free_memory(self.albedo_memory, None);

            self.device.destroy_image_view(self.normal_view, None);
            self.device.destroy_image(self.normal, None);
            self.device.free_memory(self.normal_memory, None);
        }
    }
}

fn create_attachment(
    device: &ash::Device,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    format: vk::Format,
    width: u32,
    height: u32,
) -> anyhow::Result<(vk::Image, vk::ImageView, vk::DeviceMemory)> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image = unsafe { device.create_image(&image_info, None)? };
    let req = unsafe { device.get_image_memory_requirements(image) };

    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let mem_type = find_memory_type(
        &mem_props,
        req.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    let memory = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(mem_type),
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
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                }),
            None,
        )?
    };

    Ok((image, view, memory))
}

fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_filter: u32,
    required: vk::MemoryPropertyFlags,
) -> anyhow::Result<u32> {
    for i in 0..props.memory_type_count {
        if (type_filter & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(required)
        {
            return Ok(i);
        }
    }
    anyhow::bail!("Не найден тип памяти для GBuffer attachment")
}
