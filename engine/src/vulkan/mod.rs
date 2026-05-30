pub mod bindless;
pub mod commands;
mod debug;
mod device;
mod instance;
pub mod material_buffer;
pub mod pipeline;
pub mod renderer;
pub mod shader;
mod swapchain;
pub mod sync;
pub mod texture;

pub use bindless::BindlessSet;
pub use debug::DebugMessenger;
pub use device::Device;
pub use instance::Instance;
pub use material_buffer::MaterialBuffer;
pub use pipeline::Pipeline;
pub use renderer::{Camera, DrawCall, Renderer};
pub use swapchain::Swapchain;
pub use texture::GpuTexture;

use ash::vk;
use std::sync::Arc;

pub struct VulkanContext {
    pub swapchain: Option<Swapchain>,
    pub device: Arc<Device>,
    pub surface: vk::SurfaceKHR,
    _debug: Option<DebugMessenger>,
    pub instance: Arc<Instance>,
}

impl VulkanContext {
    pub fn new(window: &winit::window::Window, validation: bool) -> anyhow::Result<Self> {
        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

        let display = window.display_handle()?.as_raw();
        let whandle = window.window_handle()?.as_raw();

        let instance = Arc::new(Instance::new(display, validation)?);

        let debug = if validation {
            Some(DebugMessenger::new(&instance)?)
        } else {
            None
        };

        let surface = unsafe {
            ash_window::create_surface(&instance.entry, &instance.handle, display, whandle, None)?
        };

        let device = Arc::new(Device::new(&instance, surface)?);
        let size = window.inner_size();
        let swapchain = Swapchain::new(&instance, &device, surface, size.width, size.height)?;

        Ok(Self {
            swapchain: Some(swapchain),
            device,
            surface,
            _debug: debug,
            instance,
        })
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.device_wait_idle().ok();
            drop(self.swapchain.take());
            let surface_loader =
                ash::khr::surface::Instance::new(&self.instance.entry, &self.instance.handle);
            surface_loader.destroy_surface(self.surface, None);
        }
    }
}
