use crate::assets::upload::GpuUploadRequest;
use crate::assets::CpuAssetServer;
use crate::components::ui::{UiLayout, UiRect, UiText};
use crate::render::extract::ExtractSystem;
use crate::render::world::{
    ExtractedRenderSettings, ExtractedUiRect, ExtractedUiRects, ExtractedUiText, ExtractedUiTexts, RenderWorld,
};
use crate::GameWorld;
use glam::Vec2;
use std::sync::mpsc::Sender;

pub struct UiExtract;
impl ExtractSystem for UiExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut CpuAssetServer,
        _upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let (screen_w, screen_h) =
            rw.get::<ExtractedRenderSettings>().map(|s| s.output_size).unwrap_or((1280.0, 720.0));

        let mut ui_rects = ExtractedUiRects::default();
        let mut ui_texts = ExtractedUiTexts::default();

        for (layout, rect) in world.inner.query::<(&UiLayout, &UiRect)>().iter() {
            let pos = Vec2::new(
                layout.anchor.x * screen_w + layout.offset.x - layout.pivot.x * rect.size.x,
                layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * rect.size.y,
            );
            ui_rects.rects.push(ExtractedUiRect { pos, size: rect.size, color: rect.color });
        }

        for (layout, text) in world.inner.query::<(&UiLayout, &UiText)>().iter() {
            let line_height = text.font_size * 1.2;
            let pos = Vec2::new(
                layout.anchor.x * screen_w + layout.offset.x,
                layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * line_height,
            );
            ui_texts.texts.push(ExtractedUiText {
                pos,
                text: text.text.clone(),
                font_size: text.font_size,
                color: text.color,
            });
        }

        rw.insert(ui_rects);
        rw.insert(ui_texts);
    }
    fn name(&self) -> &'static str {
        "extract_ui"
    }
}
