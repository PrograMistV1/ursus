use fontdue::{Font, FontSettings};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SizeBucket(pub u32);

impl SizeBucket {
    pub fn from_px(px: f32) -> Self {
        const BUCKETS: &[u32] = &[12, 16, 24, 32, 48, 64, 96];
        let px = px as u32;
        let bucket = BUCKETS.iter().copied().min_by_key(|&b| (b as i32 - px as i32).unsigned_abs()).unwrap_or(32);
        SizeBucket(bucket)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: FontId,
    ch: char,
    size: SizeBucket,
}

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

    pub advance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtlasId(pub u32);

const SDF_RADIUS: u32 = 12;
const OVERSAMPLE: u32 = 3;
const ATLAS_SIZE: u32 = 2048;
const PADDING: u32 = SDF_RADIUS + 2;

fn bitmap_to_sdf(bitmap: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || bitmap.is_empty() {
        return Vec::new();
    }

    let coverage: Vec<f32> = bitmap.iter().map(|&v| v as f32 / 255.0).collect();
    let at = |x: i32, y: i32| -> f32 {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            0.0
        } else {
            coverage[(y * w as i32 + x) as usize]
        }
    };

    let inside_at = |x: i32, y: i32| at(x, y) >= 0.5;

    let radius = SDF_RADIUS as i32;
    let mut out = vec![0u8; w * h];

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let self_inside = inside_at(x, y);
            let self_cov = at(x, y);

            let mut min_dist = (radius + 1) as f32 * (radius + 1) as f32;

            let y0 = (y - radius).max(0);
            let y1 = (y + radius).min(h as i32 - 1);
            let x0 = (x - radius).max(0);
            let x1 = (x + radius).min(w as i32 - 1);

            for sy in y0..=y1 {
                for sx in x0..=x1 {
                    if sx == x && sy == y {
                        continue;
                    }
                    let other_inside = inside_at(sx, sy);
                    if other_inside == self_inside {
                        continue;
                    }

                    let other_cov = at(sx, sy);
                    let denom = self_cov - other_cov;
                    let t = if denom.abs() > 1e-6 {
                        ((self_cov - 0.5) / denom).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };

                    let dx = (sx - x) as f32 * t;
                    let dy = (sy - y) as f32 * t;
                    let d2 = dx * dx + dy * dy;

                    if d2 < min_dist {
                        min_dist = d2;
                    }
                }
            }

            let dist = min_dist.sqrt();
            let signed = if self_inside { dist } else { -dist };

            let normalised = (signed / radius as f32) * 0.5 + 0.5;
            out[(y * w as i32 + x) as usize] = (normalised.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    out
}

fn downscale_sdf(src: &[u8], src_w: usize, src_h: usize, factor: usize) -> (Vec<u8>, usize, usize) {
    let dst_w = (src_w / factor).max(1);
    let dst_h = (src_h / factor).max(1);
    let mut dst = vec![0u8; dst_w * dst_h];
    let f2 = (factor * factor) as u32;

    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let mut sum = 0u32;
            for sy in 0..factor {
                for sx in 0..factor {
                    let sy_ = dy * factor + sy;
                    let sx_ = dx * factor + sx;
                    sum += src[sy_ * src_w + sx_] as u32;
                }
            }
            dst[dy * dst_w + dx] = (sum / f2) as u8;
        }
    }
    (dst, dst_w, dst_h)
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
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
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

        let cx = self.cursor_x;
        let cy = self.cursor_y;
        let stride = self.width as usize * 4;

        for row in 0..gh as usize {
            for col in 0..gw as usize {
                let src_idx = row * gw as usize + col;
                let dst_base = (cy as usize + row) * stride + (cx as usize + col) * 4;
                let v = sdf[src_idx];
                self.pixels[dst_base] = v;
                self.pixels[dst_base + 1] = 255;
                self.pixels[dst_base + 2] = 255;
                self.pixels[dst_base + 3] = 255;
            }
        }

        let u0 = cx as f32 / self.width as f32;
        let v0 = cy as f32 / self.height as f32;
        let u1 = (cx + gw) as f32 / self.width as f32;
        let v1 = (cy + gh) as f32 / self.height as f32;

        self.cursor_x += needed_w;
        if gh > self.row_height {
            self.row_height = gh;
        }
        self.dirty = true;

        Some((u0, v0, u1, v1))
    }
}

pub struct FontManager {
    fonts: Vec<Font>,
    atlases: Vec<FontAtlasPage>,
    glyph_cache: HashMap<GlyphKey, GlyphInfo>,
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

    pub fn preload(&mut self, font_id: FontId, chars: impl Iterator<Item = char>, size: SizeBucket) {
        let chars: Vec<char> = chars.collect();
        for ch in chars {
            self.get_or_rasterize(font_id, ch, size);
        }
    }

    pub fn glyph(&mut self, font_id: FontId, ch: char, px: f32) -> Option<GlyphInfo> {
        let size = SizeBucket::from_px(px);
        self.get_or_rasterize(font_id, ch, size)
    }

    pub fn advance(&mut self, font_id: FontId, ch: char, px: f32) -> f32 {
        self.glyph(font_id, ch, px).map(|g| g.advance).unwrap_or(px * 0.5)
    }

    pub fn measure(&mut self, font_id: FontId, text: &str, px: f32) -> f32 {
        text.chars().map(|c| self.advance(font_id, c, px)).sum()
    }

    pub fn line_height(&self, px: f32) -> f32 {
        px * 1.2
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

    fn get_or_rasterize(&mut self, font_id: FontId, ch: char, size: SizeBucket) -> Option<GlyphInfo> {
        let key = GlyphKey { font: font_id, ch, size };

        if let Some(&info) = self.glyph_cache.get(&key) {
            return Some(info);
        }

        let info = self.rasterize(font_id, ch, size)?;
        self.glyph_cache.insert(key, info);
        Some(info)
    }

    fn rasterize(&mut self, font_id: FontId, ch: char, size: SizeBucket) -> Option<GlyphInfo> {
        let font = self.fonts.get(font_id.0 as usize)?;

        let render_px = (size.0 * OVERSAMPLE) as f32;
        let (metrics, bitmap) = font.rasterize(ch, render_px);

        let advance = metrics.advance_width / OVERSAMPLE as f32;

        if bitmap.is_empty() || metrics.width == 0 || metrics.height == 0 {
            let info = GlyphInfo {
                atlas_id: AtlasId(0),
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                width: 0,
                height: 0,
                offset_x: 0,
                offset_y: 0,
                advance,
            };
            return Some(info);
        }

        let sdf_hi = bitmap_to_sdf(&bitmap, metrics.width, metrics.height);

        let (sdf, gw, gh) = downscale_sdf(&sdf_hi, metrics.width, metrics.height, OVERSAMPLE as usize);
        let gw = gw as u32;
        let gh = gh as u32;

        let offset_x = metrics.xmin / OVERSAMPLE as i32;
        let offset_y = metrics.ymin / OVERSAMPLE as i32;

        let atlas_id = self.find_or_create_atlas(font_id, gw, gh)?;
        let page = &mut self.atlases[atlas_id.0 as usize];
        let (u0, v0, u1, v1) = page.pack(&sdf, gw, gh)?;

        Some(GlyphInfo { atlas_id, u0, v0, u1, v1, width: gw, height: gh, offset_x, offset_y, advance })
    }

    fn find_or_create_atlas(&mut self, font_id: FontId, gw: u32, gh: u32) -> Option<AtlasId> {
        for page in self.atlases.iter() {
            if page.font_id == font_id {
                let fits_x = page.cursor_x + gw + PADDING <= page.width;
                let fits_y = page.cursor_y + gh + PADDING <= page.height;
                let fits_wrap = page.cursor_y + page.row_height + PADDING + gh + PADDING <= page.height;
                if fits_x && fits_y || fits_wrap {
                    return Some(page.id);
                }
            }
        }

        if gw + PADDING * 2 > ATLAS_SIZE || gh + PADDING * 2 > ATLAS_SIZE {
            log::error!("Glyph {}×{} is too large for a {}×{} atlas", gw, gh, ATLAS_SIZE, ATLAS_SIZE);
            return None;
        }

        let id = AtlasId(self.next_atlas_id);
        self.next_atlas_id += 1;
        self.atlases.push(FontAtlasPage::new(id, font_id));
        log::info!("FontManager: allocated atlas page {:?} for font {:?}", id, font_id);
        Some(id)
    }
}

impl Default for FontManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/RobotoMono.ttf");

    #[test]
    fn size_bucket_snapping() {
        assert_eq!(SizeBucket::from_px(14.0), SizeBucket(16));
        assert_eq!(SizeBucket::from_px(30.0), SizeBucket(32));
        assert_eq!(SizeBucket::from_px(48.0), SizeBucket(48));
    }

    #[test]
    fn load_and_preload() {
        let mut mgr = FontManager::new();
        let fid = mgr.load_font(FONT_BYTES).unwrap();
        mgr.preload(fid, "Hello".chars(), SizeBucket(32));
        assert!(!mgr.atlases.is_empty());
        assert!(mgr.atlases[0].dirty);
    }

    #[test]
    fn glyph_cached() {
        let mut mgr = FontManager::new();
        let fid = mgr.load_font(FONT_BYTES).unwrap();
        let g1 = mgr.glyph(fid, 'A', 32.0).unwrap();
        let g2 = mgr.glyph(fid, 'A', 32.0).unwrap();

        assert_eq!(g1.atlas_id, g2.atlas_id);
        assert!((g1.u0 - g2.u0).abs() < f32::EPSILON);
    }

    #[test]
    fn whitespace_glyph_no_panic() {
        let mut mgr = FontManager::new();
        let fid = mgr.load_font(FONT_BYTES).unwrap();
        let g = mgr.glyph(fid, ' ', 16.0).unwrap();
        assert!(g.advance > 0.0);
        assert_eq!(g.width, 0);
    }

    #[test]
    fn measure_text() {
        let mut mgr = FontManager::new();
        let fid = mgr.load_font(FONT_BYTES).unwrap();
        let w = mgr.measure(fid, "Hi", 32.0);
        assert!(w > 0.0);
    }
}
