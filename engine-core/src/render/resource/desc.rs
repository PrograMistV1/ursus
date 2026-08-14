use crate::render::gfx::descriptor::ImageUsage;
use crate::render::gfx::types::Format;
use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceHandle(pub(crate) u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResourceExtent {
    Absolute(u32, u32),
    ScaleInternal(f32),
    ScaleOutput(f32),
}

impl ResourceExtent {
    pub fn resolve(&self, internal: (u32, u32), output: (u32, u32)) -> (u32, u32) {
        let scale = |(w, h): (u32, u32), s: f32| {
            (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1))
        };

        match *self {
            Self::Absolute(w, h) => (w, h),
            Self::ScaleInternal(s) => scale(internal, s),
            Self::ScaleOutput(s) => scale(output, s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Color,
    Depth,
}

impl ResourceKind {
    pub fn aspect_mask(self) -> vk::ImageAspectFlags {
        match self {
            Self::Color => vk::ImageAspectFlags::COLOR,
            Self::Depth => vk::ImageAspectFlags::DEPTH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceDesc {
    pub name: String,
    pub format: Format,
    pub extent: ResourceExtent,
    pub kind: ResourceKind,
    pub usage: ImageUsage,
}

impl ResourceDesc {
    pub fn color(name: impl Into<String>, format: Format, extent: ResourceExtent) -> Self {
        Self {
            name: name.into(),
            format,
            extent,
            kind: ResourceKind::Color,
            usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::SAMPLED,
        }
    }

    pub fn depth(name: impl Into<String>, format: Format, extent: ResourceExtent) -> Self {
        Self {
            name: name.into(),
            format,
            extent,
            kind: ResourceKind::Depth,
            usage: ImageUsage::DEPTH_ATTACHMENT | ImageUsage::SAMPLED,
        }
    }

    pub fn with_usage(mut self, flags: ImageUsage) -> Self {
        self.usage |= flags;
        self
    }
}

pub struct ExternalImageDesc {
    pub name: String,
    pub format: vk::Format,
    pub kind: ResourceKind,
    pub initial_layout: vk::ImageLayout,
    pub final_layout: vk::ImageLayout,
}
