use glam::Vec2;

#[derive(Debug, Clone)]
pub struct UiLayout {
    pub anchor: Vec2,
    pub pivot: Vec2,
    pub offset: Vec2,
}

impl UiLayout {
    pub fn top_left(offset: Vec2) -> Self {
        Self { anchor: Vec2::ZERO, pivot: Vec2::ZERO, offset }
    }
    pub fn center() -> Self {
        Self { anchor: Vec2::splat(0.5), pivot: Vec2::splat(0.5), offset: Vec2::ZERO }
    }
    pub fn top_right(offset: Vec2) -> Self {
        Self { anchor: Vec2::new(1.0, 0.0), pivot: Vec2::new(1.0, 0.0), offset }
    }
}

#[derive(Debug, Clone)]
pub struct UiText {
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
}

impl UiText {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), font_size: 14.0, color: [1.0; 4] }
    }
    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }
}

#[derive(Debug, Clone)]
pub struct UiRect {
    pub size: Vec2,
    pub color: [f32; 4],
    pub border_radius: f32,
}
