pub mod core;
pub mod frame_ctx;
mod passes;
pub mod pipeline;
pub mod renderer;
pub mod resources;
pub mod timestamps;

pub use core::debug::DebugMessenger;
pub use core::device::Device;
pub use core::instance::Instance;
pub use core::swapchain::Swapchain;
pub use pipeline::material_buffer::MaterialBuffer;
pub use pipeline::Pipeline;
pub use renderer::Renderer;
pub use resources::bindless::BindlessSet;
pub use resources::texture::GpuTexture;
pub use timestamps::{GpuFrameTimes, GpuStage, GpuTimestampPool};

pub use passes::geometry::DrawCall;
pub use renderer::Camera;

use ash::ext::debug_utils;
use ash::vk;
pub use resources::depth::DepthBuffer;
pub use resources::render_target::RenderTarget;
use std::sync::Arc;

pub struct VulkanContext {
    pub swapchain: Option<Swapchain>,
    pub device: Arc<Device>,
    pub surface: vk::SurfaceKHR,
    _debug: Option<DebugMessenger>,
    pub debug_utils: Option<Arc<debug_utils::Device>>,
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
        let swapchain =
            Swapchain::new(&instance, &device, surface, size.width, size.height, false)?;

        let debug_utils = if validation {
            Some(Arc::new(debug_utils::Device::new(
                &instance.handle,
                &device.handle,
            )))
        } else {
            None
        };

        Ok(Self {
            swapchain: Some(swapchain),
            debug_utils,
            device,
            surface,
            _debug: debug,
            instance,
        })
    }

    pub fn recreate_swapchain(
        &mut self,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> anyhow::Result<()> {
        unsafe { self.device.handle.device_wait_idle()? };
        drop(self.swapchain.take());
        self.swapchain = Some(Swapchain::new(
            &self.instance,
            &self.device,
            self.surface,
            width,
            height,
            vsync,
        )?);
        Ok(())
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
