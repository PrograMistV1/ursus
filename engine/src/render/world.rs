use std::any::{Any, TypeId};
use std::collections::HashMap;

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

use crate::assets::mesh::Aabb;
use crate::assets::MeshHandle;
use crate::ecs::components::MaterialHandle;
use crate::vulkan::resources::light_buffer::{DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use glam::{Mat4, Vec2, Vec3};

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
}

impl Default for ExtractedRenderSettings {
    fn default() -> Self {
        Self { clear_color: [0.0, 0.0, 0.0, 1.0], output_size: (1280.0, 720.0) }
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
