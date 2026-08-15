use crate::render::gfx::descriptor::{DescriptorAllocator, DescriptorSetDesc};
use crate::render::gfx::types::Format;
use crate::render::gfx::types::{DescriptorSetId, ShaderStage};
use crate::vulkan::core::{sampler, DeviceContext};
use crate::vulkan::resources::texture::TextureSource;
use crate::vulkan::GpuTexture;
use ash::vk;
use engine_core::vulkan::core::SubmitContext;

pub const MAX_TEXTURES: u32 = 4096;

pub struct BindlessSet {
    pub set_id: DescriptorSetId,
    pub sampler: vk::Sampler,
    next_slot: u32,
    owned_textures: Vec<GpuTexture>,
    device: ash::Device,
}

impl BindlessSet {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        descriptors: &mut DescriptorAllocator,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let max_aniso =
            unsafe { instance.get_physical_device_properties(physical_device).limits.max_sampler_anisotropy.min(16.0) };
        let sampler = sampler::create_linear_repeat_aniso_sampler(device, max_aniso)?;

        let desc = DescriptorSetDesc::new()
            .with_immutable_sampler(0, ShaderStage::Fragment, sampler)
            .with_bindless_sampled_images(1, ShaderStage::Fragment, MAX_TEXTURES);

        let set_id = descriptors.create_set(desc)?;

        let mut this = Self { set_id, sampler, next_slot: 0, owned_textures: Vec::new(), device: device.clone() };

        let white = GpuTexture::upload(
            DeviceContext { device, physical_device, instance },
            SubmitContext { command_pool, queue },
            TextureSource {
                pixels: &[255u8, 255, 255, 255],
                width: 1,
                height: 1,
                format: Format::Rgba8Srgb,
                name: "white_fallback",
            },
        )?;
        let slot = this.alloc_slot(descriptors, white.view);
        assert_eq!(slot, 0, "white fallback должен быть слотом 0");
        this.owned_textures.push(white);

        Ok(this)
    }

    pub fn alloc_slot(&mut self, descriptors: &DescriptorAllocator, view: vk::ImageView) -> u32 {
        let slot = self.next_slot;
        assert!(slot < MAX_TEXTURES, "bindless texture array is full");
        descriptors.write_sampled_image_array(self.set_id, 1, slot, view);
        self.next_slot += 1;
        slot
    }

    pub fn update_slot(&self, descriptors: &DescriptorAllocator, slot: u32, view: vk::ImageView) {
        assert!(slot < self.next_slot);
        descriptors.write_sampled_image_array(self.set_id, 1, slot, view);
    }

    pub fn next_slot(&self) -> u32 {
        self.next_slot
    }
}

impl Drop for BindlessSet {
    fn drop(&mut self) {
        unsafe { self.device.destroy_sampler(self.sampler, None) };
    }
}
