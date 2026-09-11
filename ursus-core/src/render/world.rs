use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::assets::mesh::Aabb;
use glam::{Mat4, Vec3};
use ursus_ecs::components::mesh::MeshHandle;
use ursus_materials::MaterialHandle;

pub struct RWorld {
    resources: HashMap<TypeId, Box<dyn Any + Send>>,
}

impl RWorld {
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

impl Default for RWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct ExtractedInstance {
    pub mesh: MeshHandle,
    pub material: Option<MaterialHandle>,
    pub technique: Option<String>,
    pub model: Mat4,
    pub aabb: Option<Aabb>,
}

#[derive(Default, Clone)]
pub struct ExtractedMeshes {
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
pub struct ExtractedRenderSettings {
    pub clear_color: [f32; 4],
    pub output_size: (f32, f32),
    pub exposure: f32,
    pub fsr_sharpness: f32,
    pub interpolation_alpha: f32,
}

impl Default for ExtractedRenderSettings {
    fn default() -> Self {
        Self {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            output_size: (1280.0, 720.0),
            exposure: 0.5,
            fsr_sharpness: 0.2,
            interpolation_alpha: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut rw = RWorld::new();
        rw.insert(ExtractedMeshes { instances: vec![] });
        assert!(rw.get::<ExtractedMeshes>().is_some());
        assert!(rw.get::<ExtractedCamera>().is_none());
    }

    #[test]
    fn clear_removes_resources() {
        let mut rw = RWorld::new();
        rw.insert(42u32);
        rw.clear();
        assert!(rw.get::<u32>().is_none());
    }

    #[test]
    fn insert_replaces() {
        let mut rw = RWorld::new();
        rw.insert(1u32);
        rw.insert(2u32);
        assert_eq!(*rw.get::<u32>().unwrap(), 2);
    }
}
