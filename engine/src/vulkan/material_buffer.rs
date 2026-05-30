use crate::assets::material::MaterialData;
use ash::vk;

pub struct MaterialBuffer {
    pub buffer: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub mapped: *mut MaterialData,
    pub capacity: usize,
    pub layout: vk::DescriptorSetLayout,
    pub set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    device: ash::Device,
}

unsafe impl Send for MaterialBuffer {}
unsafe impl Sync for MaterialBuffer {}

impl MaterialBuffer {
    pub const MAX_MATERIALS: usize = 4096;

    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
    ) -> anyhow::Result<Self> {
        let size = (Self::MAX_MATERIALS * size_of::<MaterialData>()) as vk::DeviceSize;

        let buf_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&buf_info, None)? };

        let req = unsafe { device.get_buffer_memory_requirements(buffer) };

        let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let mem_type = find_memory_type(
            &mem_props,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(mem_type),
                None,
            )?
        };
        unsafe { device.bind_buffer_memory(buffer, memory, 0)? };

        let mapped = unsafe {
            device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut MaterialData
        };

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .bindings(std::slice::from_ref(&binding)),
                None,
            )?
        };

        let pool_size = vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        };
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(std::slice::from_ref(&pool_size))
                    .max_sets(1),
                None,
            )?
        };

        let set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&layout)),
            )?[0]
        };

        let buf_info = vk::DescriptorBufferInfo::default()
            .buffer(buffer)
            .offset(0)
            .range(size);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(std::slice::from_ref(&buf_info));

        unsafe { device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };

        log::info!(
            "MaterialBuffer: {} слотов ({} KB)",
            Self::MAX_MATERIALS,
            size / 1024
        );

        Ok(Self {
            buffer,
            memory,
            mapped,
            capacity: Self::MAX_MATERIALS,
            layout,
            set,
            pool,
            device: device.clone(),
        })
    }

    pub fn upload(&self, materials: &[MaterialData]) {
        assert!(materials.len() <= self.capacity);
        unsafe {
            std::ptr::copy_nonoverlapping(materials.as_ptr(), self.mapped, materials.len());
        }
    }
}

impl Drop for MaterialBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.unmap_memory(self.memory);
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
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
    anyhow::bail!("Не найден подходящий тип памяти для MaterialBuffer")
}
