use crate::assets::asset_registry::TextureHandle;
use crate::render::gfx::descriptor::DescriptorAllocator;
use crate::vulkan::core::SubmitContext;
use crate::vulkan::resources::texture::TextureSource;
use crate::vulkan::{BindlessSet, GpuTexture};
use ash::vk;
use engine_core::vulkan::core::DeviceContext;
use std::collections::HashMap;

pub const BINDLESS_SLOT_WHITE: u32 = 0;

/// Owns loaded GPU textures and their bindless slots.
/// TextureHandle (stable, assigned by AssetRegistry on the CPU side) -> bindless slot -> GpuTexture.
pub struct GpuTextureStore {
    slots: HashMap<TextureHandle, u32>,
    textures: HashMap<u32, GpuTexture>,
    bindless: BindlessSet,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl GpuTextureStore {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        descriptors: &mut DescriptorAllocator,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let bindless = BindlessSet::new(&device, physical_device, &instance, descriptors, command_pool, queue)?;
        assert_eq!(bindless.next_slot(), 1, "slot 0 must be white fallback");

        Ok(Self {
            slots: HashMap::new(),
            textures: HashMap::new(),
            bindless,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        })
    }

    pub fn upload(
        &mut self,
        descriptors: &DescriptorAllocator,
        handle: TextureHandle,
        upload: TextureSource,
    ) -> anyhow::Result<()> {
        let tex = GpuTexture::upload(
            DeviceContext { device: &self.device, physical_device: self.physical_device, instance: &self.instance },
            SubmitContext { command_pool: self.command_pool, queue: self.queue },
            upload,
        )?;
        let slot = self.bindless.alloc_slot(descriptors, tex.view);
        self.slots.insert(handle, slot);
        self.textures.insert(slot, tex);
        Ok(())
    }

    pub fn slot(&self, handle: TextureHandle) -> u32 {
        self.slots.get(&handle).copied().unwrap_or(BINDLESS_SLOT_WHITE)
    }

    pub fn bindless(&self) -> &BindlessSet {
        &self.bindless
    }

    pub fn bindless_mut(&mut self) -> &mut BindlessSet {
        &mut self.bindless
    }
}
