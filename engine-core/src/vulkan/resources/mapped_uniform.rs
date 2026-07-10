use crate::vulkan::core::memory::find_memory_type;
use ash::vk;
use std::ptr::write;

pub struct MappedUniformBuffer<T> {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut T,
    device: ash::Device,
}

unsafe impl<T> Send for MappedUniformBuffer<T> {}
unsafe impl<T> Sync for MappedUniformBuffer<T> {}

impl<T: Copy> MappedUniformBuffer<T> {
    pub(crate) fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        initial: T,
    ) -> anyhow::Result<Self> {
        let size = size_of::<T>() as vk::DeviceSize;

        let buf_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&buf_info, None)? };

        let req = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mem_type = find_memory_type(
            instance,
            physical_device,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(mem_type),
                None,
            )?
        };
        unsafe { device.bind_buffer_memory(buffer, memory, 0)? };

        let mapped = unsafe { device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut T };
        unsafe { write(mapped, initial) };

        Ok(Self { buffer, memory, mapped, device: device.clone() })
    }

    pub fn upload(&self, data: &T) {
        unsafe { write(self.mapped, *data) };
    }

    pub(crate) fn size(&self) -> vk::DeviceSize {
        size_of::<T>() as vk::DeviceSize
    }
}

impl<T> Drop for MappedUniformBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            self.device.unmap_memory(self.memory);
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
