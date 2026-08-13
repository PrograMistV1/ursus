use crate::assets::asset_registry::TextureHandle;
use crate::assets::texture_handle_allocator::TextureHandleAllocator;
use crate::render::gfx::types::Format;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const TEXTURE_HASH_SAMPLE_COUNT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextureContentKey(u64, usize, u32, u32, Format);

fn hash_texture(pixels: &[u8], width: u32, height: u32, format: Format) -> TextureContentKey {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let len = pixels.len();
    if len <= TEXTURE_HASH_SAMPLE_COUNT * 2 {
        pixels.hash(&mut hasher);
    } else {
        let step = len / TEXTURE_HASH_SAMPLE_COUNT;
        let mut i = 0;
        while i < len {
            hasher.write_u8(pixels[i]);
            i += step;
        }
        hasher.write(&pixels[..32.min(len)]);
        hasher.write(&pixels[len - 32.min(len)..]);
    }
    TextureContentKey(hasher.finish(), len, width, height, format)
}

pub(crate) enum TextureRegistration {
    Existing(TextureHandle),
    New(TextureHandle),
}

/// Texture deduplication by content + handle output.
#[derive(Default)]
pub(crate) struct TextureStore {
    dedup: HashMap<TextureContentKey, TextureHandle>,
}

impl TextureStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: Format,
        handles: &mut TextureHandleAllocator,
    ) -> TextureRegistration {
        let key = hash_texture(pixels, width, height, format);
        if let Some(&handle) = self.dedup.get(&key) {
            return TextureRegistration::Existing(handle);
        }
        let handle = handles.alloc();
        self.dedup.insert(key, handle);
        TextureRegistration::New(handle)
    }
}
