use ash::vk;

pub struct FrameSync {
    pub render_fence: vk::Fence,
    device: ash::Device,
}

impl FrameSync {
    pub fn new(device: &ash::Device) -> anyhow::Result<Self> {
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let render_fence = unsafe { device.create_fence(&fence_info, None)? };
        Ok(Self { render_fence, device: device.clone() })
    }
}

impl Drop for FrameSync {
    fn drop(&mut self) {
        unsafe { self.device.destroy_fence(self.render_fence, None); }
    }
}