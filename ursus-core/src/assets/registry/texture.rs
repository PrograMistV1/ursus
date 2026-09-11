use crate::assets::upload::GpuUploadRequest;
use crate::assets::upload_queue::UploadQueue;
use crate::render::gfx::types::Format;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// The only source of TextureHandle in the system.
struct TextureHandleAllocator {
    next: u32,
}

impl TextureHandleAllocator {
    fn alloc(&mut self) -> TextureHandle {
        let h = TextureHandle(self.next);
        self.next += 1;
        h
    }
}
impl Default for TextureHandleAllocator {
    fn default() -> Self {
        Self { next: 1 } // 0 is reserved for the bindless white reserve.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

/// CPU-side texture registration with content-based deduplication.
#[derive(Default)]
pub struct TextureRegistry {
    handles: TextureHandleAllocator,
    dedup: TextureDedup,
    upload_queue: UploadQueue,
}

impl TextureRegistry {
    pub fn upload_rgba8(
        &mut self,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: Format,
        name: impl Into<String>,
    ) -> TextureHandle {
        match self.dedup.register(&pixels, width, height, format, &mut self.handles) {
            TextureRegistration::Existing(handle) => handle,
            TextureRegistration::New(handle) => {
                self.upload_queue.push(GpuUploadRequest::Texture {
                    handle,
                    pixels,
                    width,
                    height,
                    format,
                    name: name.into(),
                });
                handle
            }
        }
    }

    pub(crate) fn flush_uploads(&mut self, tx: &std::sync::mpsc::Sender<GpuUploadRequest>) {
        self.upload_queue.drain_to(tx);
    }
}

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
pub(crate) struct TextureDedup {
    dedup: HashMap<TextureContentKey, TextureHandle>,
}

impl TextureDedup {
    fn register(
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
