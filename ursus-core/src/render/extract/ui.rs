use crate::render::extract::ExtractSystem;
use crate::render::world::{
    ExtractedRenderSettings, ExtractedUiRect, ExtractedUiRects, ExtractedUiText, ExtractedUiTexts, RWorld,
};
use glam::Vec2;
use ursus_ecs::components::ui::{UiLayout, UiRect, UiText};
use ursus_ecs::World;

pub struct UiExtract;
impl ExtractSystem for UiExtract {
    fn extract(&self, world: &World, r_world: &mut RWorld) {
        let (screen_w, screen_h) =
            r_world.get::<ExtractedRenderSettings>().map(|s| s.output_size).unwrap_or((1280.0, 720.0));

        let mut ui_rects = ExtractedUiRects::default();
        let mut ui_texts = ExtractedUiTexts::default();

        for (layout, rect) in world.query::<(&UiLayout, &UiRect)>().iter() {
            let pos = Vec2::new(
                layout.anchor.x * screen_w + layout.offset.x - layout.pivot.x * rect.size.x,
                layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * rect.size.y,
            );
            ui_rects.rects.push(ExtractedUiRect { pos, size: rect.size, color: rect.color });
        }

        for (layout, text) in world.query::<(&UiLayout, &UiText)>().iter() {
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

        r_world.insert(ui_rects);
        r_world.insert(ui_texts);
    }
    fn name(&self) -> &'static str {
        "extract_ui"
    }
}
