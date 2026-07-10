use crate::vulkan::core::memory::find_memory_type;
use ash::vk;
use std::marker::PhantomData;
use std::ptr::{copy_nonoverlapping, write};

pub struct MappedGpuBuffer<T> {
    pub(crate) buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut T,
    pub capacity: usize,
    device: ash::Device,
    _marker: PhantomData<T>,
}

unsafe impl<T> Send for MappedGpuBuffer<T> {}
unsafe impl<T> Sync for MappedGpuBuffer<T> {}

impl<T: Copy> MappedGpuBuffer<T> {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        usage: vk::BufferUsageFlags,
        capacity: usize,
    ) -> anyhow::Result<Self> {
        let size = (capacity * size_of::<T>()) as vk::DeviceSize;

        let buf_info = vk::BufferCreateInfo::default().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);
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

        Ok(Self { buffer, memory, mapped, capacity, device: device.clone(), _marker: PhantomData })
    }

    pub fn new_single(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        initial: T,
    ) -> anyhow::Result<Self> {
        let this = Self::new(device, physical_device, instance, vk::BufferUsageFlags::UNIFORM_BUFFER, 1)?;
        this.upload_one(&initial);
        Ok(this)
    }

    pub fn upload_one(&self, data: &T) {
        debug_assert_eq!(self.capacity, 1, "upload_one вызван для буфера с capacity != 1");
        unsafe { write(self.mapped, *data) };
    }

    pub fn upload_slice(&self, data: &[T]) {
        assert!(data.len() <= self.capacity, "MappedGpuBuffer::upload_slice: превышена capacity");
        unsafe { copy_nonoverlapping(data.as_ptr(), self.mapped, data.len()) };
    }

    pub fn size(&self) -> vk::DeviceSize {
        (self.capacity * size_of::<T>()) as vk::DeviceSize
    }
}

impl<T> Drop for MappedGpuBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            self.device.unmap_memory(self.memory);
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
