use cosmic_text::CacheKey;
use etagere::{size2, AtlasAllocator, Rectangle};
use std::collections::HashMap;

pub const ATLAS_SIZE: i32 = 2048;

#[derive(Debug, Clone, Copy)]
pub struct GlyphUv {
    pub page: u32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
}

pub struct AtlasPage {
    allocator: AtlasAllocator,
    pub pixels: Vec<u8>,
    pub dirty: bool,
    pub bindless_slot: Option<u32>,
}

impl AtlasPage {
    fn new() -> Self {
        Self {
            allocator: AtlasAllocator::new(size2(ATLAS_SIZE, ATLAS_SIZE)),
            pixels: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            dirty: true,
            bindless_slot: None,
        }
    }
}

pub struct TextAtlas {
    pub pages: Vec<AtlasPage>,
    cache: HashMap<CacheKey, Option<GlyphUv>>,
}

impl TextAtlas {
    pub fn new() -> Self {
        Self { pages: vec![AtlasPage::new()], cache: HashMap::new() }
    }

    pub fn get_or_rasterize(
        &mut self,
        key: CacheKey,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
        coverage: &[u8],
    ) -> Option<GlyphUv> {
        if let Some(&cached) = self.cache.get(&key) {
            return cached;
        }

        let uv = if width == 0 || height == 0 { None } else { self.pack(width, height, left, top, coverage) };

        self.cache.insert(key, uv);
        uv
    }

    fn pack(&mut self, width: u32, height: u32, left: i32, top: i32, coverage: &[u8]) -> Option<GlyphUv> {
        let size = size2(width as i32, height as i32);

        for (page_idx, page) in self.pages.iter_mut().enumerate() {
            if let Some(alloc) = page.allocator.allocate(size) {
                blit(page, alloc.rectangle, width, height, coverage);
                return Some(make_uv(page_idx as u32, alloc.rectangle, width, height, left, top));
            }
        }

        if width as i32 > ATLAS_SIZE || height as i32 > ATLAS_SIZE {
            log::error!("TextAtlas: глиф {}x{} слишком велик для атласа", width, height);
            return None;
        }

        let mut page = AtlasPage::new();
        let alloc = page.allocator.allocate(size)?;
        blit(&mut page, alloc.rectangle, width, height, coverage);
        let page_idx = self.pages.len() as u32;
        let uv = make_uv(page_idx, alloc.rectangle, width, height, left, top);
        self.pages.push(page);
        Some(uv)
    }
}

impl Default for TextAtlas {
    fn default() -> Self {
        Self::new()
    }
}

fn blit(page: &mut AtlasPage, rect: Rectangle, width: u32, height: u32, coverage: &[u8]) {
    let stride = ATLAS_SIZE as usize;
    let ox = rect.min.x as usize;
    let oy = rect.min.y as usize;
    for row in 0..height as usize {
        let src = row * width as usize;
        let dst = (oy + row) * stride + ox;
        page.pixels[dst..dst + width as usize].copy_from_slice(&coverage[src..src + width as usize]);
    }
    page.dirty = true;
}

fn make_uv(page: u32, rect: Rectangle, width: u32, height: u32, left: i32, top: i32) -> GlyphUv {
    let s = ATLAS_SIZE as f32;
    GlyphUv {
        page,
        u0: rect.min.x as f32 / s,
        v0: rect.min.y as f32 / s,
        u1: (rect.min.x as f32 + width as f32) / s,
        v1: (rect.min.y as f32 + height as f32) / s,
        width,
        height,
        left,
        top,
    }
}