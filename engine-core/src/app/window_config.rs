use winit::window::{Fullscreen, WindowAttributes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    BorderlessFullscreen,
    ExclusiveFullscreen,
}

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub mode: WindowMode,
    pub visible_on_start: bool,
    pub icon_rgba: Option<(Vec<u8>, u32, u32)>, // (pixels RGBA8, width, height)
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "engine-core".to_string(),
            width: 1280,
            height: 720,
            resizable: true,
            mode: WindowMode::Windowed,
            visible_on_start: true,
            icon_rgba: None,
        }
    }
}

impl WindowConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn with_mode(mut self, mode: WindowMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_visible_on_start(mut self, visible: bool) -> Self {
        self.visible_on_start = visible;
        self
    }

    pub fn with_icon(mut self, pixels: Vec<u8>, width: u32, height: u32) -> Self {
        self.icon_rgba = Some((pixels, width, height));
        self
    }

    pub(crate) fn to_winit_attributes(&self) -> WindowAttributes {
        let mut attrs = WindowAttributes::default()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::LogicalSize::new(self.width, self.height))
            .with_resizable(self.resizable)
            .with_visible(false);

        if let Some(icon) = self.build_icon() {
            attrs = attrs.with_window_icon(Some(icon));
        }

        if matches!(self.mode, WindowMode::BorderlessFullscreen) {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }

        attrs
    }

    fn build_icon(&self) -> Option<winit::window::Icon> {
        let (pixels, w, h) = self.icon_rgba.as_ref()?;
        winit::window::Icon::from_rgba(pixels.clone(), *w, *h).ok()
    }
}
