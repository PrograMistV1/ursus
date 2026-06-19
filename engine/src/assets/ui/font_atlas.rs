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
    pub pixels: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    glyphs: HashMap<(char, u32), GlyphUv>,
    pub advances: HashMap<(char, u32), f32>,
}

impl FontAtlas {
    /// Создаёт атлас из TTF данных.
    /// `preload_chars` — символы которые растеризуем сразу.
    /// `preload_sizes` — размеры шрифта для которых растеризуем.
    pub fn new(ttf_data: &[u8], preload_chars: &str, preload_sizes: &[u32]) -> anyhow::Result<Self> {
        let font = Font::from_bytes(ttf_data, FontSettings::default())
            .map_err(|e| anyhow::anyhow!("Ошибка загрузки шрифта: {}", e))?;

        let atlas_width = 1024u32;
        let atlas_height = 1024u32;
        // R8 — один канал, потом при загрузке в Vulkan используем как alpha
        let mut pixels = vec![0u8; (atlas_width * atlas_height) as usize];

        let mut glyphs = HashMap::new();
        let mut advances = HashMap::new();

        // Простой row-packer: идём слева направо, при переполнении — новая строка
        let padding = 1u32;
        let mut cursor_x = padding;
        let mut cursor_y = padding;
        let mut row_height = 0u32;

        for &size in preload_sizes {
            let px = size as f32;
            for ch in preload_chars.chars() {
                let (metrics, bitmap) = font.rasterize(ch, px);

                if bitmap.is_empty() {
                    advances.insert((ch, size), metrics.advance_width);
                    glyphs.insert(
                        (ch, size),
                        GlyphUv { u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0, width: 0, height: 0, offset_x: 0, offset_y: 0 },
                    );
                    continue;
                }

                let gw = metrics.width as u32;
                let gh = metrics.height as u32;

                if cursor_x + gw + padding > atlas_width {
                    cursor_y += row_height + padding;
                    cursor_x = padding;
                    row_height = 0;
                }

                if cursor_y + gh + padding > atlas_height {
                    log::warn!("Font atlas переполнен при глифе '{}' размер {}", ch, size);
                    break;
                }

                for row in 0..gh {
                    for col in 0..gw {
                        let src = (row * gw + col) as usize;
                        let dst = ((cursor_y + row) * atlas_width + cursor_x + col) as usize;
                        pixels[dst] = bitmap[src];
                    }
                }

                let u0 = cursor_x as f32 / atlas_width as f32;
                let v0 = cursor_y as f32 / atlas_height as f32;
                let u1 = (cursor_x + gw) as f32 / atlas_width as f32;
                let v1 = (cursor_y + gh) as f32 / atlas_height as f32;

                glyphs.insert(
                    (ch, size),
                    GlyphUv { u0, v0, u1, v1, width: gw, height: gh, offset_x: metrics.xmin, offset_y: metrics.ymin },
                );
                advances.insert((ch, size), metrics.advance_width);

                cursor_x += gw + padding;
                if gh > row_height {
                    row_height = gh;
                }
            }
        }

        let rgba: Vec<u8> = pixels.iter().flat_map(|&a| [255, 255, 255, a]).collect();

        Ok(Self { pixels: rgba, atlas_width, atlas_height, glyphs, advances })
    }

    pub fn get_glyph(&self, ch: char, size: u32) -> Option<&GlyphUv> {
        self.glyphs.get(&(ch, size))
    }

    pub fn get_advance(&self, ch: char, size: u32) -> f32 {
        self.advances.get(&(ch, size)).copied().unwrap_or(0.0)
    }

    /// Вычисляет ширину строки в пикселях для заданного размера шрифта
    pub fn measure_text(&self, text: &str, font_size: u32) -> f32 {
        text.chars().map(|c| self.get_advance(c, font_size)).sum()
    }

    /// Возвращает высоту строки для заданного размера (приближение через метрики шрифта)
    pub fn line_height(&self, font_size: u32) -> f32 {
        font_size as f32 * 1.2
    }
}
