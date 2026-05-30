use super::shader_registry::{ShaderHandle, TextureSlot, MAX_TEXTURE_SLOTS};
use crate::assets::TextureHandle;
use glam::Vec4;

#[derive(Debug, Clone)]
pub struct MaterialDef {
    pub name: String,
    pub shader: ShaderHandle,
    pub base_color: Vec4,
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: Vec4,
    textures: [TextureHandle; MAX_TEXTURE_SLOTS],
}

impl MaterialDef {
    pub fn new(name: impl Into<String>, shader: ShaderHandle) -> Self {
        Self {
            name: name.into(),
            shader,
            base_color: Vec4::ONE,
            metallic: 0.0,
            roughness: 0.5,
            emissive: Vec4::ZERO,
            textures: [TextureHandle(0); MAX_TEXTURE_SLOTS],
        }
    }

    pub fn with_texture(mut self, slot: TextureSlot, handle: TextureHandle) -> Self {
        self.textures[slot.index()] = handle;
        self
    }

    pub fn set_texture(&mut self, slot: TextureSlot, handle: TextureHandle) {
        self.textures[slot.index()] = handle;
    }

    pub fn get_texture(&self, slot: TextureSlot) -> TextureHandle {
        self.textures[slot.index()]
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

    pub fn to_gpu_data(&self) -> MaterialData {
        let t = &self.textures;
        MaterialData {
            base_color: self.base_color.into(),
            emissive: self.emissive.into(),
            metallic: self.metallic,
            roughness: self.roughness,
            _pad: [0.0; 2],
            // uvec4: diffuse, normal, metallic_roughness, emissive
            tex_indices0: [t[0].0, t[1].0, t[2].0, t[3].0],
            // occlusion + 3 pad
            tex_indices1: [t[4].0, 0, 0, 0],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MaterialData {
    pub base_color: [f32; 4],   // 16 B
    pub emissive: [f32; 4],     // 16 B
    pub metallic: f32,          //  4 B
    pub roughness: f32,         //  4 B
    pub _pad: [f32; 2],         //  8 B
    pub tex_indices0: [u32; 4], // 16 B  diffuse | normal | metallic_roughness | emissive
    pub tex_indices1: [u32; 4], // 16 B  occlusion | pad | pad | pad
} // = 80 B

impl MaterialData {
    pub fn default_white() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0; 4],
            metallic: 0.0,
            roughness: 0.5,
            _pad: [0.0; 2],
            tex_indices0: [0; 4],
            tex_indices1: [0; 4],
        }
    }
}

unsafe impl bytemuck::Pod for MaterialData {}
unsafe impl bytemuck::Zeroable for MaterialData {}

pub use crate::ecs::components::MaterialHandle;
