use crate::render::gfx::types::Format;
use crate::vulkan::core::memory::find_memory_type;
use crate::vulkan::core::DeviceContext;
use ash::vk;

/// GPU-side image + its memory, with an image view created for it.
/// Owns `(image, memory)` until `into_raw` is called - if anything goes wrong
/// before then, `Drop` will free both resources.
pub(super) struct AllocatedTextureImage {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    device: ash::Device,
}

impl AllocatedTextureImage {
    pub(super) fn create(
        ctx: DeviceContext,
        format: Format,
        width: u32,
        height: u32,
        mip_levels: u32,
        usage: vk::ImageUsageFlags,
    ) -> anyhow::Result<Self> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format.to_vk())
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { ctx.device.create_image(&image_info, None)? };

        let req = unsafe { ctx.device.get_image_memory_requirements(image) };
        let mem_type = match find_memory_type(
            ctx.instance,
            ctx.physical_device,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ) {
            Ok(t) => t,
            Err(e) => {
                unsafe { ctx.device.destroy_image(image, None) };
                return Err(e);
            }
        };

        let memory = unsafe {
            ctx.device.allocate_memory(
                &vk::MemoryAllocateInfo::default().allocation_size(req.size).memory_type_index(mem_type),
                None,
            )
        };
        let memory = match memory {
            Ok(m) => m,
            Err(e) => {
                unsafe { ctx.device.destroy_image(image, None) };
                return Err(e.into());
            }
        };

        if let Err(e) = unsafe { ctx.device.bind_image_memory(image, memory, 0) } {
            unsafe {
                ctx.device.destroy_image(image, None);
                ctx.device.free_memory(memory, None);
            }
            return Err(e.into());
        }

        Ok(Self { image, memory, device: ctx.device.clone() })
    }

    pub(super) fn create_view(&self, format: Format, mip_levels: u32) -> anyhow::Result<vk::ImageView> {
        let view_info = vk::ImageViewCreateInfo::default()
            .image(self.image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format.to_vk())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            });
        Ok(unsafe { self.device.create_image_view(&view_info, None)? })
    }

    /// Transfers ownership of `(image, memory)` to the caller without destroying them -
    /// used when `GpuTexture` takes ownership and handles their cleanup in `Drop`.
    pub(super) fn into_raw(self) -> (vk::Image, vk::DeviceMemory) {
        let out = (self.image, self.memory);
        std::mem::forget(self);
        out
    }
}

impl Drop for AllocatedTextureImage {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
