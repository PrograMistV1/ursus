use ash::vk;

pub struct FrameSync {
    pub render_fence: vk::Fence,
    pub image_available: vk::Semaphore,
    pub render_finished: vk::Semaphore,
    device: ash::Device,
}

impl FrameSync {
    pub fn new(device: &ash::Device) -> anyhow::Result<Self> {
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        let sem_info = vk::SemaphoreCreateInfo::default();

        let render_fence = unsafe { device.create_fence(&fence_info, None)? };
        let image_available = unsafe { device.create_semaphore(&sem_info, None)? };
        let render_finished = unsafe { device.create_semaphore(&sem_info, None)? };

        Ok(Self {
            render_fence,
            image_available,
            render_finished,
            device: device.clone(),
        })
    }
}

impl Drop for FrameSync {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.render_fence, None);
            self.device.destroy_semaphore(self.image_available, None);
            self.device.destroy_semaphore(self.render_finished, None);
        }
    }
}
