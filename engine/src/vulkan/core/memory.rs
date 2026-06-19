use ash::vk;

pub struct AllocatedImage {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub memory: vk::DeviceMemory,
}

pub struct ImageDesc {
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub usage: vk::ImageUsageFlags,
    pub aspect_mask: vk::ImageAspectFlags,
    pub mip_levels: u32,
}

impl ImageDesc {
    pub fn color(format: vk::Format, width: u32, height: u32, usage: vk::ImageUsageFlags) -> Self {
        Self { format, width, height, usage, aspect_mask: vk::ImageAspectFlags::COLOR, mip_levels: 1 }
    }

    pub fn depth(format: vk::Format, width: u32, height: u32, usage: vk::ImageUsageFlags) -> Self {
        Self { format, width, height, usage, aspect_mask: vk::ImageAspectFlags::DEPTH, mip_levels: 1 }
    }
}

pub fn alloc_image(
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: &ash::Instance,
    desc: &ImageDesc,
) -> anyhow::Result<AllocatedImage> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(desc.format)
        .extent(vk::Extent3D { width: desc.width, height: desc.height, depth: 1 })
        .mip_levels(desc.mip_levels)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(desc.usage)
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
                .format(desc.format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: desc.aspect_mask,
                    base_mip_level: 0,
                    level_count: desc.mip_levels,
                    base_array_layer: 0,
                    layer_count: 1,
                }),
            None,
        )?
    };

    Ok(AllocatedImage { image, view, memory })
}

pub fn find_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_filter: u32,
    required: vk::MemoryPropertyFlags,
) -> anyhow::Result<u32> {
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    for i in 0..mem_props.memory_type_count {
        if (type_filter & (1 << i)) != 0 && mem_props.memory_types[i as usize].property_flags.contains(required) {
            return Ok(i);
        }
    }

    anyhow::bail!("Не найден тип памяти с флагами {:?}, type_filter={:#034b}", required, type_filter)
}

pub fn alloc_buffer(
    device: &ash::Device,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    props: vk::MemoryPropertyFlags,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let buf_info = vk::BufferCreateInfo::default().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buf = unsafe { device.create_buffer(&buf_info, None)? };
    let req = unsafe { device.get_buffer_memory_requirements(buf) };
    let alloc = vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(find_memory_type(
        instance,
        physical_device,
        req.memory_type_bits,
        props,
    )?);
    let mem = unsafe { device.allocate_memory(&alloc, None)? };
    unsafe { device.bind_buffer_memory(buf, mem, 0)? };
    Ok((buf, mem))
}

pub unsafe fn destroy_image_resources(
    device: &ash::Device,
    image: vk::Image,
    view: vk::ImageView,
    memory: vk::DeviceMemory,
) {
    device.destroy_image_view(view, None);
    device.destroy_image(image, None);
    device.free_memory(memory, None);
}
