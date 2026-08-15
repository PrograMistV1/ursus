use crate::vulkan::core::memory::alloc_buffer;
use crate::vulkan::core::DeviceContext;
use ash::vk;

/// Temporary host-visible buffer for uploading pixels before copying them to the GPU image.
/// Automatically freed in `Drop` - including if something later in the chain (image allocation, `one_shot`)
/// returns an error.
pub(super) struct StagingBuffer {
    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    device: ash::Device,
}

impl StagingBuffer {
    pub(super) fn upload(ctx: DeviceContext, data: &[u8]) -> anyhow::Result<Self> {
        let size = data.len() as vk::DeviceSize;

        let (buffer, memory) = alloc_buffer(
            ctx.device,
            ctx.instance,
            ctx.physical_device,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        unsafe {
            let ptr = ctx.device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty());
            let ptr = match ptr {
                Ok(p) => p as *mut u8,
                Err(e) => {
                    ctx.device.destroy_buffer(buffer, None);
                    ctx.device.free_memory(memory, None);
                    return Err(e.into());
                }
            };
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            ctx.device.unmap_memory(memory);
        }

        Ok(Self { buffer, memory, device: ctx.device.clone() })
    }
}

impl Drop for StagingBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
