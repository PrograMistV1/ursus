use fontdue::{Font, FontSettings};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct GlyphUv {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub width: u32,
    pub height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

pub struct FontAtlas {
    font: Font,
    pub pixels: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub dirty: bool,

    glyphs: HashMap<(char, u32), GlyphUv>,
    advances: HashMap<(char, u32), f32>,

    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    padding: u32,

    preload_chars: String,
}

impl FontAtlas {
    pub fn new(ttf_data: &[u8], preload_chars: &str, preload_sizes: &[u32]) -> anyhow::Result<Self> {
        let font = Font::from_bytes(ttf_data, FontSettings::default())
            .map_err(|e| anyhow::anyhow!("Ошибка загрузки шрифта: {}", e))?;

        let atlas_width = 1024u32;
        let atlas_height = 1024u32;

        let mut atlas = Self {
            font,
            pixels: vec![0u8; (atlas_width * atlas_height * 4) as usize],
            atlas_width,
            atlas_height,
            dirty: true,
            glyphs: HashMap::new(),
            advances: HashMap::new(),
            cursor_x: 1,
            cursor_y: 1,
            row_height: 0,
            padding: 1,
            preload_chars: preload_chars.to_string(),
        };

        for &size in preload_sizes {
            let chars: Vec<char> = preload_chars.chars().collect();
            for ch in chars {
                atlas.rasterize_char_internal(ch, size);
            }
        }

        Ok(atlas)
    }

    pub fn get_glyph(&mut self, ch: char, size: u32) -> Option<&GlyphUv> {
        if !self.glyphs.contains_key(&(ch, size)) {
            self.rasterize_char_internal(ch, size);
        }
        self.glyphs.get(&(ch, size))
    }

    pub fn get_advance(&mut self, ch: char, size: u32) -> f32 {
        if !self.advances.contains_key(&(ch, size)) {
            self.rasterize_char_internal(ch, size);
        }
        self.advances.get(&(ch, size)).copied().unwrap_or(0.0)
    }

    pub fn preload_size(&mut self, size: u32) {
        let chars: Vec<char> = self.preload_chars.chars().collect();
        for ch in chars {
            if !self.glyphs.contains_key(&(ch, size)) {
                self.rasterize_char_internal(ch, size);
            }
        }
    }

    pub fn measure_text(&mut self, text: &str, size: u32) -> f32 {
        text.chars().map(|c| self.get_advance(c, size)).sum()
    }

    pub fn line_height(&self, size: u32) -> f32 {
        size as f32 * 1.2
    }

    fn rasterize_char_internal(&mut self, ch: char, size: u32) {
        let (metrics, bitmap) = self.font.rasterize(ch, size as f32);

        self.advances.insert((ch, size), metrics.advance_width);

        if bitmap.is_empty() || metrics.width == 0 || metrics.height == 0 {
            self.glyphs.insert(
                (ch, size),
                GlyphUv { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, width: 0, height: 0, offset_x: 0, offset_y: 0 },
            );
            return;
        }

        let gw = metrics.width as u32;
        let gh = metrics.height as u32;

        if self.cursor_x + gw + self.padding > self.atlas_width {
            self.cursor_y += self.row_height + self.padding;
            self.cursor_x = self.padding;
            self.row_height = 0;
        }

        if self.cursor_y + gh + self.padding > self.atlas_height {
            log::warn!("FontAtlas переполнен: глиф '{}' размер {}", ch, size);
            self.glyphs.insert(
                (ch, size),
                GlyphUv { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, width: 0, height: 0, offset_x: 0, offset_y: 0 },
            );
            return;
        }

        let cx = self.cursor_x;
        let cy = self.cursor_y;
        let stride = self.atlas_width as usize * 4;

        for row in 0..gh as usize {
            for col in 0..gw as usize {
                let src = row * gw as usize + col;
                let dst = (cy as usize + row) * stride + (cx as usize + col) * 4;
                let a = bitmap[src];
                self.pixels[dst] = 255;
                self.pixels[dst + 1] = 255;
                self.pixels[dst + 2] = 255;
                self.pixels[dst + 3] = a;
            }
        }

        let u0 = cx as f32 / self.atlas_width as f32;
        let v0 = cy as f32 / self.atlas_height as f32;
        let u1 = (cx + gw) as f32 / self.atlas_width as f32;
        let v1 = (cy + gh) as f32 / self.atlas_height as f32;

        self.glyphs.insert(
            (ch, size),
            GlyphUv { u0, v0, u1, v1, width: gw, height: gh, offset_x: metrics.xmin, offset_y: metrics.ymin },
        );

        self.cursor_x += gw + self.padding;
        if gh > self.row_height {
            self.row_height = gh;
        }

        self.dirty = true;
    }
}
