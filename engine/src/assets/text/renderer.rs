use crate::assets::text::atlas::TextAtlas;
use crate::vulkan::passes::ui::UiPass;
use crate::vulkan::resources::bindless::BindlessSet;
use crate::vulkan::GpuTexture;
use ash::vk;
use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent};
use glam::Vec2;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub fontdb::ID);

pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    families: HashMap<FontId, String>,

    page_textures: HashMap<u32, GpuTexture>,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl TextRenderer {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas: TextAtlas::new(),
            families: HashMap::new(),
            page_textures: HashMap::new(),
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        }
    }

    pub fn load_font(&mut self, data: &[u8]) -> FontId {
        let db = self.font_system.db_mut();
        let before: HashSet<fontdb::ID> = db.faces().map(|f| f.id).collect();
        db.load_font_data(data.to_vec());

        let id = db
            .faces()
            .map(|f| f.id)
            .find(|id| !before.contains(id))
            .expect("TextRenderer::load_font: не удалось загрузить шрифт");

        let family = db
            .face(id)
            .and_then(|f| f.families.first().map(|(name, _)| name.clone()))
            .unwrap_or_else(|| "sans-serif".to_string());

        log::info!("TextRenderer: загружен шрифт '{}' ({:?})", family, id);

        self.families.insert(FontId(id), family);
        FontId(id)
    }

    pub fn measure(&mut self, font: FontId, text: &str, px: f32) -> Vec2 {
        let metrics = Metrics::new(px, px * 1.2);
        let family_name: Option<String> = self.families.get(&font).cloned();
        let attrs = match &family_name {
            Some(name) => Attrs::new().family(Family::Name(name)),
            None => Attrs::new(),
        };

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.set_size(None, None);
        buffer.shape_until_scroll(&mut self.font_system,false);

        let mut width = 0.0f32;
        let mut lines = 0usize;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            lines += 1;
        }
        Vec2::new(width, lines.max(1) as f32 * metrics.line_height)
    }

    pub fn line_height(&self, px: f32) -> f32 {
        px * 1.2
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        ui_pass: &UiPass,
        font: FontId,
        text: &str,
        px: f32,
        origin: Vec2,
        color: [f32; 4],
        max_width: Option<f32>,
        screen_size: [f32; 2],
    ) {
        let metrics = Metrics::new(px, px * 1.2);
        let family_name: Option<String> = self.families.get(&font).cloned();
        let attrs = match &family_name {
            Some(name) => Attrs::new().family(Family::Name(name)),
            None => Attrs::new(),
        };

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.set_size(max_width, None);
        buffer.shape_until_scroll(&mut self.font_system,false);

        let runs: Vec<(f32, Vec<cosmic_text::LayoutGlyph>)> =
            buffer.layout_runs().map(|r| (r.line_y, r.glyphs.to_vec())).collect();

        for (line_y, glyphs) in &runs {
            for glyph in glyphs {
                let physical = glyph.physical((0.0, 0.0), 1.0);

                let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) else {
                    continue;
                };

                let width = image.placement.width;
                let height = image.placement.height;
                let left = image.placement.left;
                let top = image.placement.top;

                let coverage: Vec<u8> = match image.content {
                    SwashContent::Mask => image.data.clone(),
                    SwashContent::SubpixelMask => {
                        image.data.chunks(3).map(|c| ((c[0] as u32 + c[1] as u32 + c[2] as u32) / 3) as u8).collect()
                    }
                    SwashContent::Color => image.data.chunks(4).map(|c| c[3]).collect(),
                };

                let Some(uv) = self.atlas.get_or_rasterize(physical.cache_key, width, height, left, top, &coverage)
                else {
                    continue;
                };

                let slot = self.bindless_slot_for_page(uv.page);
                let pos = Vec2::new(
                    origin.x + physical.x as f32 + uv.left as f32,
                    origin.y + *line_y + physical.y as f32 - uv.top as f32,
                );
                ui_pass.draw_glyph(device, cmd, screen_size, pos, &uv, slot, color);
            }
        }
    }

    fn bindless_slot_for_page(&self, page: u32) -> u32 {
        self.atlas.pages.get(page as usize).and_then(|p| p.bindless_slot).unwrap_or(0)
    }

    pub fn flush_atlas(&mut self, bindless: &mut BindlessSet) -> anyhow::Result<()> {
        for (idx, page) in self.atlas.pages.iter_mut().enumerate() {
            if !page.dirty {
                continue;
            }

            let tex = GpuTexture::upload_no_mip(
                &self.device,
                self.physical_device,
                &self.instance,
                self.command_pool,
                self.queue,
                &page.pixels,
                crate::assets::text::atlas::ATLAS_SIZE as u32,
                crate::assets::text::atlas::ATLAS_SIZE as u32,
                vk::Format::R8_UNORM,
                &format!("text_atlas_{idx}"),
            )?;

            match page.bindless_slot {
                Some(slot) => {
                    bindless.update_slot(slot, tex.view);
                    self.page_textures.insert(idx as u32, tex);
                }
                None => {
                    let slot = bindless.alloc_slot(tex.view);
                    page.bindless_slot = Some(slot);
                    self.page_textures.insert(idx as u32, tex);
                }
            }

            page.dirty = false;
        }
        Ok(())
    }
}
