use crate::assets::asset_registry::TextureHandle;
use crate::assets::texture_handle_allocator::TextureHandleAllocator;
use crate::assets::texture_store::{TextureRegistration, TextureStore};
use crate::assets::upload::GpuUploadRequest;
use crate::assets::upload_queue::UploadQueue;
use crate::render::gfx::types::Format;

/// CPU-side texture registration with content-based deduplication.
pub struct TextureRegistry {
    handles: TextureHandleAllocator,
    dedup: TextureStore,
    upload_queue: UploadQueue,
}

impl TextureRegistry {
    pub(crate) fn new() -> Self {
        Self { handles: TextureHandleAllocator::new(), dedup: TextureStore::new(), upload_queue: UploadQueue::new() }
    }

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
