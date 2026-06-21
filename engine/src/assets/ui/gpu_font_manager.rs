use crate::assets::ui::font_manager::{AtlasId, FontManager, GlyphInfo};
use crate::vulkan::resources::bindless::BindlessSet;
use crate::vulkan::GpuTexture;
use ash::vk;
use std::collections::HashMap;

struct GpuFontAtlas {
    atlas_id: AtlasId,

    pub bindless_slot: u32,

    _texture: GpuTexture,
}

pub struct GpuFontManager {
    gpu_atlases: HashMap<AtlasId, GpuFontAtlas>,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl GpuFontManager {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> Self {
        Self { gpu_atlases: HashMap::new(), device, physical_device, instance, command_pool, queue }
    }

    pub fn flush(&mut self, font_manager: &mut FontManager, bindless: &mut BindlessSet) -> anyhow::Result<()> {
        let dirty_ids: Vec<AtlasId> = font_manager.dirty_atlases().map(|a| a.id).collect();

        if dirty_ids.is_empty() {
            return Ok(());
        }

        for atlas_id in dirty_ids {
            let page = font_manager.atlas(atlas_id).expect("dirty atlas must exist");

            let tex = GpuTexture::upload(
                &self.device,
                self.physical_device,
                &self.instance,
                self.command_pool,
                self.queue,
                &page.pixels,
                page.width,
                page.height,
                vk::Format::R8G8B8A8_UNORM,
                &format!("font_atlas_{}", atlas_id.0),
            )?;

            let existing_slot = self.gpu_atlases.get(&atlas_id).map(|g| g.bindless_slot);

            match existing_slot {
                Some(slot) => {
                    bindless.update_slot(slot, tex.view);

                    self.gpu_atlases.insert(atlas_id, GpuFontAtlas { atlas_id, bindless_slot: slot, _texture: tex });
                    log::debug!("GpuFontManager: re-uploaded atlas {:?} (slot {})", atlas_id, slot);
                }

                None => {
                    let slot = bindless.alloc_slot(tex.view);
                    self.gpu_atlases.insert(atlas_id, GpuFontAtlas { atlas_id, bindless_slot: slot, _texture: tex });
                    log::info!("GpuFontManager: uploaded new atlas {:?} → bindless slot {}", atlas_id, slot);
                }
            }

            font_manager.mark_atlas_clean(atlas_id);
        }

        Ok(())
    }

    pub fn bindless_slot(&self, atlas_id: AtlasId) -> u32 {
        self.gpu_atlases.get(&atlas_id).map(|g| g.bindless_slot).unwrap_or(0)
    }

    pub fn slot_for_glyph(&self, glyph: &GlyphInfo) -> u32 {
        self.bindless_slot(glyph.atlas_id)
    }
}
