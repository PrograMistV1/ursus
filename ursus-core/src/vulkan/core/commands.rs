use ash::vk;

pub struct Commands {
    pub pool: vk::CommandPool,
    pub buffers: Vec<vk::CommandBuffer>,
    device: ash::Device,
}

impl Commands {
    pub fn new(device: &ash::Device, graphics_family: u32, frames_in_flight: u32) -> anyhow::Result<Self> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(graphics_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

        let pool = unsafe { device.create_command_pool(&pool_info, None)? };

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(frames_in_flight);

        let buffers = unsafe { device.allocate_command_buffers(&alloc_info)? };

        log::debug!("Command pool created ({} buffers)", frames_in_flight);
        Ok(Self { pool, buffers, device: device.clone() })
    }
}

impl Drop for Commands {
    fn drop(&mut self) {
        unsafe { self.device.destroy_command_pool(self.pool, None) };
    }
}
