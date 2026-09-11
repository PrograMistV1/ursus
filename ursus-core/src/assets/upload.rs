use crate::assets::mesh::Vertex;
use crate::assets::registry::TextureHandle;
use crate::render::gfx::types::Format;
use ursus_ecs::components::mesh::MeshHandle;
use ursus_materials::MaterialHandle;

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
    /// Packed material bytes ready to be written into GPU-visible storage.
    /// `bytes.len()` is this material's stride under whichever
    /// `MaterialLayout` produced them (see `ursus_materials::pack::aos`).
    Material { handle: MaterialHandle, bytes: Vec<u8> },
}
