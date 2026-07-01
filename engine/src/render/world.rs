use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::assets::mesh::Aabb;
use crate::assets::TextureHandle;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::vulkan::resources::light_buffer::{DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use glam::{Mat4, Vec2, Vec3};

pub struct RenderWorld {
    resources: HashMap<TypeId, Box<dyn Any + Send>>,
}

impl RenderWorld {
    pub fn new() -> Self {
        Self { resources: HashMap::new() }
    }

    pub fn insert<T: Send + 'static>(&mut self, val: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(val));
    }

    pub fn get<T: Send + 'static>(&self) -> Option<&T> {
        self.resources.get(&TypeId::of::<T>()).and_then(|b| b.downcast_ref::<T>())
    }

    pub fn get_mut<T: Send + 'static>(&mut self) -> Option<&mut T> {
        self.resources.get_mut(&TypeId::of::<T>()).and_then(|b| b.downcast_mut::<T>())
    }

    pub fn clear(&mut self) {
        self.resources.clear();
    }
}

impl Default for RenderWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct ExtractedInstance {
    pub mesh: MeshHandle,
    pub material: Option<MaterialHandle>,
    pub model: Mat4,
    pub aabb: Option<Aabb>,
}

#[derive(Default, Clone)]
pub struct ExtractedMeshes {
    pub instances: Vec<ExtractedInstance>,
}

#[derive(Default, Clone)]
pub struct ExtractedShadowMeshes {
    pub instances: Vec<ExtractedInstance>,
}

#[derive(Clone)]
pub struct ExtractedCamera {
    pub eye: Vec3,
    pub view: Mat4,
    pub proj: Mat4,
    pub view_proj: Mat4,
}

impl Default for ExtractedCamera {
    fn default() -> Self {
        Self { eye: Vec3::ZERO, view: Mat4::IDENTITY, proj: Mat4::IDENTITY, view_proj: Mat4::IDENTITY }
    }
}

#[derive(Clone)]
pub struct ExtractedLights {
    pub directional: DirectionalLight,
    pub point_lights: [GpuPointLight; MAX_POINT_LIGHTS],
    pub point_light_count: u32,
    pub light_view_proj: Mat4,
}

impl Default for ExtractedLights {
    fn default() -> Self {
        Self {
            directional: DirectionalLight { direction: [-0.3, -1.0, -0.2, 0.0], color: [1.0, 0.95, 0.85, 2.0] },
            point_lights: [GpuPointLight { position: [0.0; 4], color: [0.0; 4] }; MAX_POINT_LIGHTS],
            point_light_count: 0,
            light_view_proj: Mat4::IDENTITY,
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiPrimitive {
    Rect {
        pos: Vec2,
        size: Vec2,
        color: [f32; 4],
        border_radius: f32,
    },
    TexturedRect {
        pos: Vec2,
        size: Vec2,
        color: [f32; 4],
        bindless_slot: u32,
        uv: [f32; 4],
    },
    GlyphRect {
        pos: Vec2,
        size: Vec2,
        color: [f32; 4],
        texture_handle: TextureHandle,
        uv: [f32; 4],
    },
}

#[derive(Debug, Clone, Default)]
pub struct PreparedUiDrawList {
    pub primitives: Vec<UiPrimitive>,
}

impl PreparedUiDrawList {
    pub fn push_rect(&mut self, pos: Vec2, size: Vec2, color: [f32; 4], border_radius: f32) {
        self.primitives.push(UiPrimitive::Rect { pos, size, color, border_radius });
    }
    pub fn push_textured_rect(&mut self, pos: Vec2, size: Vec2, color: [f32; 4], bindless_slot: u32, uv: [f32; 4]) {
        self.primitives.push(UiPrimitive::TexturedRect { pos, size, color, bindless_slot, uv });
    }

    pub fn push_glyph(&mut self, pos: Vec2, size: Vec2, color: [f32; 4], texture_handle: TextureHandle, uv: [f32; 4]) {
        self.primitives.push(UiPrimitive::GlyphRect { pos, size, color, texture_handle, uv });
    }
}

#[derive(Default, Clone)]
pub struct ExtractedUiRects {
    pub rects: Vec<ExtractedUiRect>,
}

#[derive(Clone)]
pub struct ExtractedUiRect {
    pub pos: Vec2,
    pub size: Vec2,
    pub color: [f32; 4],
}

#[derive(Default, Clone)]
pub struct ExtractedUiTexts {
    pub texts: Vec<ExtractedUiText>,
}

#[derive(Clone)]
pub struct ExtractedUiText {
    pub pos: Vec2,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
}

#[derive(Clone)]
pub struct ExtractedRenderSettings {
    pub clear_color: [f32; 4],
    pub output_size: (f32, f32),
    pub exposure: f32,
    pub fsr_sharpness: f32,
}

impl Default for ExtractedRenderSettings {
    fn default() -> Self {
        Self { clear_color: [0.0, 0.0, 0.0, 1.0], output_size: (1280.0, 720.0), exposure: 0.5, fsr_sharpness: 0.2 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut rw = RenderWorld::new();
        rw.insert(ExtractedMeshes { instances: vec![] });
        assert!(rw.get::<ExtractedMeshes>().is_some());
        assert!(rw.get::<ExtractedCamera>().is_none());
    }

    #[test]
    fn clear_removes_resources() {
        let mut rw = RenderWorld::new();
        rw.insert(42u32);
        rw.clear();
        assert!(rw.get::<u32>().is_none());
    }

    #[test]
    fn insert_replaces() {
        let mut rw = RenderWorld::new();
        rw.insert(1u32);
        rw.insert(2u32);
        assert_eq!(*rw.get::<u32>().unwrap(), 2);
    }
}
