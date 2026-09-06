use crate::vulkan::MappedGpuBuffer;
use ash::vk;

pub struct GrowableBuffer {
    inner: MappedGpuBuffer<u8>,
    len: usize,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    usage: vk::BufferUsageFlags,
}

impl GrowableBuffer {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        usage: vk::BufferUsageFlags,
        initial_capacity: usize,
    ) -> anyhow::Result<Self> {
        let inner = MappedGpuBuffer::new(device, physical_device, instance, usage, initial_capacity.max(1))?;
        Ok(Self { inner, len: 0, device: device.clone(), physical_device, instance: instance.clone(), usage })
    }

    pub fn buffer(&self) -> vk::Buffer {
        self.inner.buffer
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Replaces the buffer's contents with `bytes`, growing the underlying
    /// GPU buffer first if it isn't large enough. Returns `true` if the
    /// underlying `vk::Buffer` handle changed (a reallocation happened) -
    /// callers that keep a descriptor pointing at this buffer must rewrite
    /// that descriptor when this returns `true`.
    pub fn upload(&mut self, bytes: &[u8]) -> anyhow::Result<bool> {
        let mut reallocated = false;
        if bytes.len() > self.inner.capacity {
            let new_capacity = bytes.len().next_power_of_two();
            self.inner =
                MappedGpuBuffer::new(&self.device, self.physical_device, &self.instance, self.usage, new_capacity)?;
            reallocated = true;
        }
        self.inner.upload_slice(bytes);
        self.len = bytes.len();
        Ok(reallocated)
    }
}
