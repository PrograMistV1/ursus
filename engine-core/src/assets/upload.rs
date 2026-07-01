use crate::assets::mesh::Vertex;
use crate::assets::shader_registry::TextureSlot;
use crate::assets::TextureHandle;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use ash::vk;

#[derive(Debug)]
pub enum GpuUploadRequest {
    Mesh {
        handle: MeshHandle,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        name: String,
    },
    Texture {
        handle: TextureHandle,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: vk::Format,
        name: String,
    },
    Material {
        handle: MaterialHandle,
        base_color: [f32; 4],
        metallic: f32,
        roughness: f32,
        emissive: [f32; 4],
        texture_slots: Vec<(TextureSlot, TextureHandle)>,
        name: String,
    },
}
