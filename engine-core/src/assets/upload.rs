use crate::assets::mesh::Vertex;
use crate::assets::TextureHandle;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::render::gfx::types::Format;

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
        format: Format,
        name: String,
    },
    Material {
        handle: MaterialHandle,
        texture_slots: Vec<(String, TextureHandle)>,
    },
    /// Packed material bytes ready to be written into GPU-visible storage.
    /// `bytes.len()` is this material's stride under whichever
    /// `MaterialLayout` produced them (see `engine_materials::pack::aos`).
    MaterialData {
        handle: engine_materials::MaterialHandle,
        bytes: Vec<u8>,
    },
}
