use crate::assets::text::{FontId, TextRenderer};
use crate::assets::texture_handle_allocator::TextureHandleAllocator;
use crate::assets::upload::GpuUploadRequest;
use crate::render::world::PreparedUiDrawList;
use cosmic_text::fontdb::Family;
use glam::Vec2;
use std::sync::mpsc::Sender;

/// Wrapper over [`TextRenderer`] that sets the engine's default font.
pub(crate) struct TextService {
    text_renderer: TextRenderer,
    default_font: FontId,
}

impl TextService {
    pub(crate) fn new() -> Self {
        let text_renderer = TextRenderer::new();
        let default_font = text_renderer
            .find_system_font(Family::Monospace)
            .or_else(|| text_renderer.find_system_font(Family::SansSerif))
            .expect("No system fonts found (Monospace/SansSerif) - install fonts in the system");

        Self { text_renderer, default_font }
    }

    pub(crate) fn default_font(&self) -> FontId {
        self.default_font
    }

    pub(crate) fn measure(&mut self, text: &str, px: f32) -> Vec2 {
        self.text_renderer.measure(self.default_font, text, px)
    }

    pub(crate) fn prepare(
        &mut self,
        text: &str,
        font_size: f32,
        pos: Vec2,
        color: [f32; 4],
        out: &mut PreparedUiDrawList,
    ) {
        self.text_renderer.prepare_text(self.default_font, text, font_size, pos, color, None, out);
    }

    pub(crate) fn flush_atlas(
        &mut self,
        texture_handles: &mut TextureHandleAllocator,
        upload_tx: &Sender<GpuUploadRequest>,
    ) {
        self.text_renderer.flush_atlas_to_channel(texture_handles, upload_tx);
    }
}
