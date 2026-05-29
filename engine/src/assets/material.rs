use glam::Vec4;

#[derive(Debug, Clone)]
pub struct MaterialDef {
    pub name: String,
    pub shader: String,
    pub base_color: Vec4,
    pub metallic: f32,
    pub roughness: f32,
}

impl MaterialDef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            shader: "pbr".to_string(),
            base_color: Vec4::ONE,
            metallic: 0.0,
            roughness: 0.5,
        }
    }

    pub fn with_shader(mut self, shader: impl Into<String>) -> Self {
        self.shader = shader.into();
        self
    }

    pub fn with_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.base_color = Vec4::new(r, g, b, a);
        self
    }

    pub fn with_metallic(mut self, v: f32) -> Self {
        self.metallic = v;
        self
    }

    pub fn with_roughness(mut self, v: f32) -> Self {
        self.roughness = v;
        self
    }
}

pub use crate::ecs::components::MaterialHandle as Material;
