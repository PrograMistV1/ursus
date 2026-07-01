use crate::assets::upload::GpuUploadRequest;
use crate::assets::CpuAssetServer;
use crate::render::extract::ExtractSystem;
use crate::render::world::{ExtractedUiRects, ExtractedUiTexts, PreparedUiDrawList, RenderWorld};
use crate::GameWorld;
use std::sync::mpsc::Sender;

pub struct ShapeUiSystem;

impl ExtractSystem for ShapeUiSystem {
    fn extract(
        &self,
        _world: &GameWorld,
        rw: &mut RenderWorld,
        cpu_assets: &mut CpuAssetServer,
        upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let mut draw_list = PreparedUiDrawList::default();

        // прямоугольники — никакой CPU работы, просто перекладываем
        if let Some(rects) = rw.get::<ExtractedUiRects>() {
            for r in &rects.rects.clone() {
                draw_list.push_rect(r.pos, r.size, r.color, 0.0);
            }
        }

        // текст — шейпинг + растеризация в атлас
        if let Some(texts) = rw.get::<ExtractedUiTexts>() {
            let font = cpu_assets.default_font;
            for t in &texts.texts.clone() {
                cpu_assets.text_renderer.prepare_text(font, &t.text, t.font_size, t.pos, t.color, None, &mut draw_list);
            }
        }

        cpu_assets.flush_text_atlas(upload_tx);

        rw.insert(draw_list);
    }

    fn name(&self) -> &'static str {
        "shape_ui"
    }
}
