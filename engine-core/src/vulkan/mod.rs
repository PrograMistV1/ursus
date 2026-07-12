pub mod core;
pub mod gfx_pipeline;
pub mod renderer;
pub mod resources;
pub mod timestamps;

pub use core::debug::DebugMessenger;
pub use core::device::Device;
pub use core::instance::Instance;
pub use core::swapchain::Swapchain;
pub use gfx_pipeline::Pipeline;
pub use renderer::{build_dyn_renderer, DynRenderer, Renderer};
pub use resources::bindless::BindlessSet;
pub use resources::depth::DepthBuffer;
pub use resources::mapped_buffer::MappedGpuBuffer;
pub use resources::render_target::RenderTarget;
pub use resources::texture::GpuTexture;

use crate::EngineFlags;
use ash::ext::debug_utils;
use ash::vk;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
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
    pub fn from_handles(
        display: RawDisplayHandle,
        window: RawWindowHandle,
        flags: EngineFlags,
    ) -> anyhow::Result<Self> {
        let instance = Arc::new(Instance::new(display, flags.validation, flags.debug_labels)?);

        let validation_active = instance.validation_active;
        let debug_utils_active = instance.debug_utils_active;

        let debug = if validation_active {
            Some(DebugMessenger::new(&instance)?)
        } else {
            None
        };

        let surface = unsafe { ash_window::create_surface(&instance.entry, &instance.handle, display, window, None)? };

        let device = Arc::new(Device::new(&instance, surface)?);

        let swapchain = Swapchain::new(&instance, &device, surface, 1280, 720, false)?;

        let debug_utils = if debug_utils_active {
            Some(Arc::new(debug_utils::Device::new(&instance.handle, &device.handle)))
        } else {
            None
        };

        Ok(Self { swapchain: Some(swapchain), debug_utils, device, surface, _debug: debug, instance })
    }

    pub fn new(window: &winit::window::Window, flags: EngineFlags) -> anyhow::Result<Self> {
        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        let display = window.display_handle()?.as_raw();
        let whandle = window.window_handle()?.as_raw();
        let size = window.inner_size();

        let mut ctx = Self::from_handles(display, whandle, flags)?;
        ctx.recreate_swapchain(size.width, size.height, false)?;
        Ok(ctx)
    }

    pub fn recreate_swapchain(&mut self, width: u32, height: u32, vsync: bool) -> anyhow::Result<()> {
        unsafe { self.device.handle.device_wait_idle()? };
        drop(self.swapchain.take());
        self.swapchain = Some(Swapchain::new(&self.instance, &self.device, self.surface, width, height, vsync)?);
        Ok(())
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.device_wait_idle().ok();
            drop(self.swapchain.take());
            let surface_loader = ash::khr::surface::Instance::new(&self.instance.entry, &self.instance.handle);
            surface_loader.destroy_surface(self.surface, None);
        }
    }
}
