use crate::vulkan::pipeline::builder::cmd::begin_rendering_load;
use ash::vk;

pub struct UiPass;

impl UiPass {
    pub fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        swapchain_view: vk::ImageView,
        extent: vk::Extent2D,
    ) -> anyhow::Result<()> {
        begin_rendering_load(device, cmd, swapchain_view, extent);
        unsafe { device.cmd_end_rendering(cmd) };
        Ok(())
    }
}
