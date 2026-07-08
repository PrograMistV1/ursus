use crate::render::gfx::format::Format;
use crate::vulkan::{Device, Instance};
use ash::vk;

pub struct Swapchain {
    pub handle: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: Format,
    pub extent: vk::Extent2D,
    loader: ash::khr::swapchain::Device,
    device: ash::Device,
}

impl Swapchain {
    pub fn new(
        instance: &Instance,
        device: &Device,
        surface: vk::SurfaceKHR,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> anyhow::Result<Self> {
        let surface_loader = ash::khr::surface::Instance::new(&instance.entry, &instance.handle);
        let loader = ash::khr::swapchain::Device::new(&instance.handle, &device.handle);

        let formats = unsafe { surface_loader.get_physical_device_surface_formats(device.physical, surface)? };
        let format = formats
            .iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_SRGB && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .copied()
            .unwrap_or(formats[0]);

        let present_modes =
            unsafe { surface_loader.get_physical_device_surface_present_modes(device.physical, surface)? };

        let present_mode = if vsync {
            vk::PresentModeKHR::FIFO
        } else {
            [
                vk::PresentModeKHR::MAILBOX,
                vk::PresentModeKHR::IMMEDIATE,
                vk::PresentModeKHR::FIFO,
            ]
            .iter()
            .find(|&&mode| present_modes.contains(&mode))
            .copied()
            .unwrap_or(vk::PresentModeKHR::FIFO)
        };

        let capabilities =
            unsafe { surface_loader.get_physical_device_surface_capabilities(device.physical, surface)? };
        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: width.clamp(capabilities.min_image_extent.width, capabilities.max_image_extent.width),
                height: height.clamp(capabilities.min_image_extent.height, capabilities.max_image_extent.height),
            }
        };

        let image_count = (capabilities.min_image_count + 1).min(if capabilities.max_image_count == 0 {
            u32::MAX
        } else {
            capabilities.max_image_count
        });

        let (sharing_mode, queue_families): (vk::SharingMode, Vec<u32>) =
            if device.graphics_family != device.present_family {
                (vk::SharingMode::CONCURRENT, vec![device.graphics_family, device.present_family])
            } else {
                (vk::SharingMode::EXCLUSIVE, vec![])
            };

        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(sharing_mode)
            .queue_family_indices(&queue_families)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);

        let handle = unsafe { loader.create_swapchain(&create_info, None)? };
        let images = unsafe { loader.get_swapchain_images(handle)? };

        let image_views: anyhow::Result<Vec<_>> = images
            .iter()
            .map(|&image| {
                let view_info = vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format.format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });
                Ok(unsafe { device.handle.create_image_view(&view_info, None)? })
            })
            .collect();
        let image_views = image_views?;

        log::debug!(
            "Swapchain: {}x{} {:?} ({} images, {:?})",
            extent.width,
            extent.height,
            format.format,
            image_views.len(),
            present_mode
        );

        Ok(Self {
            handle,
            images,
            image_views,
            format: Format::from_vk(format.format),
            extent,
            loader,
            device: device.handle.clone(),
        })
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            for &view in &self.image_views {
                self.device.destroy_image_view(view, None);
            }
            self.loader.destroy_swapchain(self.handle, None);
        }
        log::info!("Swapchain уничтожен");
    }
}
