use crate::assets::mesh::CpuMesh;
use crate::assets::shader_registry::TextureSlot;
use crate::components::transform::Transform;

pub enum PendingUpload {
    Mesh {
        path: std::path::PathBuf,
        meshes: Vec<PendingMesh>,
    },
    Texture {
        path: std::path::PathBuf,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: ash::vk::Format,
        name: String,
    },
}

pub struct PendingMesh {
    pub cpu_mesh: CpuMesh,
    pub transform: Transform,
    pub material: Option<PendingMaterial>,
}

pub struct PendingMaterial {
    pub name: String,
    pub shader_name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub textures: Vec<PendingTexture>,
}

pub struct PendingTexture {
    pub slot: TextureSlot,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: ash::vk::Format,
    pub name: String,
    pub image_index: usize,
}
