use fontdue::{Font, FontSettings};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

const ATLAS_GLYPH_PX: f32 = 64.0;

const SDF_RADIUS: i32 = 8;

const PADDING: u32 = SDF_RADIUS as u32 + 1;

const ATLAS_SIZE: u32 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtlasId(pub u32);

#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    pub atlas_id: AtlasId,

    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,

    pub width: u32,
    pub height: u32,

    pub offset_x: i32,
    pub offset_y: i32,

    pub advance_atlas: f32,
}

pub struct FontAtlasPage {
    pub id: AtlasId,
    pub font_id: FontId,

    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,

    pub dirty: bool,

    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl FontAtlasPage {
    fn new(id: AtlasId, font_id: FontId) -> Self {
        Self {
            id,
            font_id,
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            dirty: true,
            cursor_x: PADDING,
            cursor_y: PADDING,
            row_height: 0,
        }
    }

    fn pack(&mut self, sdf: &[u8], gw: u32, gh: u32) -> Option<(f32, f32, f32, f32)> {
        let needed_w = gw + PADDING;
        let needed_h = gh + PADDING;

        if self.cursor_x + needed_w > self.width {
            self.cursor_y += self.row_height + PADDING;
            self.cursor_x = PADDING;
            self.row_height = 0;
        }

        if self.cursor_y + needed_h > self.height {
            return None;
        }

        let cx = self.cursor_x as usize;
        let cy = self.cursor_y as usize;
        let stride = self.width as usize;

        for row in 0..gh as usize {
            let src = row * gw as usize;
            let dst = (cy + row) * stride + cx;
            self.pixels[dst..dst + gw as usize].copy_from_slice(&sdf[src..src + gw as usize]);
        }

        let u0 = self.cursor_x as f32 / self.width as f32;
        let v0 = self.cursor_y as f32 / self.height as f32;
        let u1 = (self.cursor_x + gw) as f32 / self.width as f32;
        let v1 = (self.cursor_y + gh) as f32 / self.height as f32;

        self.cursor_x += needed_w;
        if gh > self.row_height {
            self.row_height = gh;
        }
        self.dirty = true;

        Some((u0, v0, u1, v1))
    }
}

fn bitmap_to_sdf(coverage: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 {
        return Vec::new();
    }

    let radius = SDF_RADIUS;
    let mut out = vec![0u8; w * h];

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let idx = (y * w as i32 + x) as usize;
            let self_inside = coverage[idx] >= 128;

            let mut min_dist_sq = (radius + 1) * (radius + 1);

            let y0 = (y - radius).max(0);
            let y1 = (y + radius).min(h as i32 - 1);
            let x0 = (x - radius).max(0);
            let x1 = (x + radius).min(w as i32 - 1);

            'outer: for sy in y0..=y1 {
                for sx in x0..=x1 {
                    let other_inside = coverage[(sy * w as i32 + sx) as usize] >= 128;
                    if other_inside == self_inside {
                        continue;
                    }
                    let dx = sx - x;
                    let dy = sy - y;
                    let d2 = dx * dx + dy * dy;
                    if d2 < min_dist_sq {
                        min_dist_sq = d2;
                        if min_dist_sq == 1 {
                            break 'outer;
                        }
                    }
                }
            }

            let dist = (min_dist_sq as f32).sqrt();
            let signed = if self_inside { dist } else { -dist };
            let normalised = (signed / radius as f32) * 0.5 + 0.5;
            out[idx] = (normalised.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }

    out
}

pub struct FontManager {
    fonts: Vec<Font>,
    atlases: Vec<FontAtlasPage>,

    glyph_cache: HashMap<(FontId, char), GlyphInfo>,
    next_atlas_id: u32,
}

impl FontManager {
    pub fn new() -> Self {
        Self { fonts: Vec::new(), atlases: Vec::new(), glyph_cache: HashMap::new(), next_atlas_id: 0 }
    }

    pub fn load_font(&mut self, ttf_data: &[u8]) -> anyhow::Result<FontId> {
        let font = Font::from_bytes(ttf_data, FontSettings::default())
            .map_err(|e| anyhow::anyhow!("Failed to load font: {}", e))?;
        let id = FontId(self.fonts.len() as u32);
        self.fonts.push(font);
        log::info!("FontManager: loaded font {:?}", id);
        Ok(id)
    }

    pub fn preload(&mut self, font_id: FontId, chars: impl Iterator<Item = char>, _size: SizeBucket) {
        for ch in chars {
            self.get_or_rasterize(font_id, ch);
        }
    }

    pub fn glyph(&mut self, font_id: FontId, ch: char, _px: f32) -> Option<GlyphInfo> {
        self.get_or_rasterize(font_id, ch)
    }

    pub fn advance(&mut self, font_id: FontId, ch: char, px: f32) -> f32 {
        let scale = px / ATLAS_GLYPH_PX;
        self.get_or_rasterize(font_id, ch).map(|g| g.advance_atlas * scale).unwrap_or(px * 0.5)
    }

    pub fn measure(&mut self, font_id: FontId, text: &str, px: f32) -> f32 {
        text.chars().map(|c| self.advance(font_id, c, px)).sum()
    }

    pub fn line_height(&self, px: f32) -> f32 {
        px * 1.2
    }

    pub fn scale_for_px(px: f32) -> f32 {
        px / ATLAS_GLYPH_PX
    }

    pub fn atlases(&self) -> &[FontAtlasPage] {
        &self.atlases
    }

    pub fn atlas(&self, id: AtlasId) -> Option<&FontAtlasPage> {
        self.atlases.get(id.0 as usize)
    }

    pub fn dirty_atlases(&self) -> impl Iterator<Item = &FontAtlasPage> {
        self.atlases.iter().filter(|a| a.dirty)
    }

    pub fn mark_clean(&mut self) {
        for a in &mut self.atlases {
            a.dirty = false;
        }
    }

    pub fn mark_atlas_clean(&mut self, id: AtlasId) {
        if let Some(a) = self.atlases.get_mut(id.0 as usize) {
            a.dirty = false;
        }
    }

    fn get_or_rasterize(&mut self, font_id: FontId, ch: char) -> Option<GlyphInfo> {
        let key = (font_id, ch);
        if let Some(&info) = self.glyph_cache.get(&key) {
            return Some(info);
        }
        let info = self.rasterize(font_id, ch)?;
        self.glyph_cache.insert(key, info);
        Some(info)
    }

    fn rasterize(&mut self, font_id: FontId, ch: char) -> Option<GlyphInfo> {
        let font = self.fonts.get(font_id.0 as usize)?;
        let (metrics, bitmap) = font.rasterize(ch, ATLAS_GLYPH_PX);

        let advance_atlas = metrics.advance_width;

        if bitmap.is_empty() || metrics.width == 0 || metrics.height == 0 {
            return Some(GlyphInfo {
                atlas_id: AtlasId(0),
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                width: 0,
                height: 0,
                offset_x: 0,
                offset_y: 0,
                advance_atlas,
            });
        }

        let sdf = bitmap_to_sdf(&bitmap, metrics.width, metrics.height);
        let gw = metrics.width as u32;
        let gh = metrics.height as u32;

        let atlas_id = self.find_or_create_atlas(font_id, gw, gh)?;
        let page = &mut self.atlases[atlas_id.0 as usize];
        let (u0, v0, u1, v1) = page.pack(&sdf, gw, gh)?;

        Some(GlyphInfo {
            atlas_id,
            u0,
            v0,
            u1,
            v1,
            width: gw,
            height: gh,
            offset_x: metrics.xmin,
            offset_y: metrics.ymin,
            advance_atlas,
        })
    }

    fn find_or_create_atlas(&mut self, font_id: FontId, gw: u32, gh: u32) -> Option<AtlasId> {
        for page in self.atlases.iter() {
            if page.font_id != font_id {
                continue;
            }
            let fits_row = page.cursor_x + gw + PADDING <= page.width && page.cursor_y + gh + PADDING <= page.height;
            let fits_wrap = page.cursor_y + page.row_height + PADDING + gh + PADDING <= page.height;
            if fits_row || fits_wrap {
                return Some(page.id);
            }
        }

        if gw + PADDING * 2 > ATLAS_SIZE || gh + PADDING * 2 > ATLAS_SIZE {
            log::error!("Glyph {}×{} is too large for atlas", gw, gh);
            return None;
        }

        let id = AtlasId(self.next_atlas_id);
        self.next_atlas_id += 1;
        self.atlases.push(FontAtlasPage::new(id, font_id));
        log::info!("FontManager: new atlas page {:?} for font {:?}", id, font_id);
        Some(id)
    }
}

impl Default for FontManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SizeBucket(pub u32);

impl SizeBucket {
    pub fn from_px(_px: f32) -> Self {
        SizeBucket(0)
    }
}
