use crate::assets::cpu_server::TextureHandle;
use crate::assets::text::atlas::TextAtlas;
use crate::assets::text::atlas::ATLAS_SIZE;
use crate::assets::upload::GpuUploadRequest;
use crate::render::gfx::Format;
use crate::render::world::PreparedUiDrawList;
use cosmic_text::fontdb::Query;
use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, LayoutGlyph, Metrics, Shaping, SwashCache, SwashContent};
use glam::Vec2;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub fontdb::ID);

pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    pub atlas: TextAtlas,
    families: HashMap<FontId, String>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas: TextAtlas::new(),
            families: HashMap::new(),
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

    pub fn find_system_font(&self, family: Family) -> Option<FontId> {
        let db = self.font_system.db();
        let query = Query { families: &[family], ..Query::default() };
        db.query(&query).map(FontId)
    }

    pub fn measure(&mut self, font: FontId, text: &str, px: f32) -> Vec2 {
        let metrics = Metrics::new(px, px * 1.2);
        let family_name = self.families.get(&font).cloned();
        let attrs = match &family_name {
            Some(name) => Attrs::new().family(Family::Name(name)),
            None => Attrs::new(),
        };
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.set_size(None, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

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

    pub fn prepare_text(
        &mut self,
        font: FontId,
        text: &str,
        px: f32,
        origin: Vec2,
        color: [f32; 4],
        max_width: Option<f32>,
        out: &mut PreparedUiDrawList,
    ) {
        let metrics = Metrics::new(px, px * 1.2);
        let family_name = self.families.get(&font).cloned();
        let attrs = match &family_name {
            Some(name) => Attrs::new().family(Family::Name(name)),
            None => Attrs::new(),
        };

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.set_size(max_width, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let runs: Vec<(f32, Vec<LayoutGlyph>)> = buffer.layout_runs().map(|r| (r.line_y, r.glyphs.to_vec())).collect();

        for (line_y, glyphs) in &runs {
            for glyph in glyphs {
                let physical = glyph.physical((0.0, 0.0), 1.0);

                let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical.cache_key) else {
                    continue;
                };

                let coverage: Vec<u8> = match image.content {
                    SwashContent::Mask => image.data.clone(),
                    SwashContent::SubpixelMask => {
                        image.data.chunks(3).map(|c| ((c[0] as u32 + c[1] as u32 + c[2] as u32) / 3) as u8).collect()
                    }
                    SwashContent::Color => image.data.chunks(4).map(|c| c[3]).collect(),
                };

                let Some(uv) = self.atlas.get_or_rasterize(
                    physical.cache_key,
                    image.placement.width,
                    image.placement.height,
                    image.placement.left,
                    image.placement.top,
                    &coverage,
                ) else {
                    continue;
                };

                if uv.width == 0 || uv.height == 0 {
                    continue;
                }

                let Some(texture_handle) = self.atlas.page_texture_handle(uv.page) else {
                    continue;
                };

                let pos = Vec2::new(
                    origin.x + physical.x as f32 + uv.left as f32,
                    origin.y + line_y + physical.y as f32 - uv.top as f32,
                );

                out.push_glyph(
                    pos,
                    Vec2::new(uv.width as f32, uv.height as f32),
                    color,
                    texture_handle,
                    [uv.u0, uv.v0, uv.u1, uv.v1],
                );
            }
        }
    }

    pub fn flush_atlas_to_channel(&mut self, next_texture_handle: &mut u32, upload_tx: &Sender<GpuUploadRequest>) {
        for (idx, page) in self.atlas.pages.iter_mut().enumerate() {
            if !page.dirty {
                continue;
            }

            let handle = match page.texture_handle {
                Some(h) => h,
                None => {
                    let h = TextureHandle(*next_texture_handle);
                    *next_texture_handle += 1;
                    page.texture_handle = Some(h);
                    h
                }
            };

            upload_tx
                .send(GpuUploadRequest::Texture {
                    handle,
                    pixels: page.pixels.clone(),
                    width: ATLAS_SIZE as u32,
                    height: ATLAS_SIZE as u32,
                    format: Format::R8Unorm,
                    name: format!("text_atlas_{idx}"),
                })
                .ok();

            page.dirty = false;
        }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
