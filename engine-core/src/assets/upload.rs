use crate::assets::material::MaterialPayload;
use crate::assets::mesh::Vertex;
use crate::assets::TextureHandle;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use ash::vk;

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
        payload: Box<dyn MaterialPayload>,
        texture_slots: Vec<(String, TextureHandle)>,
    },
}
