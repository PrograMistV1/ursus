use ash::vk;

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
