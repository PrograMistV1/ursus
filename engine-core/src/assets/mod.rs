pub mod asset_registry;
pub mod gpu_server;
pub mod loader_registry;
pub mod material;
pub(crate) mod material_handle_allocator;
pub mod mesh;
pub(crate) mod mesh_handle_allocator;
pub mod shader_registry;
pub mod text;
pub(crate) mod text_service;
pub(crate) mod texture_handle_allocator;
pub(crate) mod texture_store;
pub mod upload;
pub(crate) mod upload_queue;

pub use asset_registry::{AssetRegistry, TextureHandle};
pub use loader_registry::{
    AssetLoader, LoadedMaterial, LoadedMeshSource, LoadedPrimitive, LoadedTexture, LoaderRegistry,
};
pub use material::MaterialPayload;
pub use mesh::{CpuMesh, GpuMesh, Vertex};
pub use shader_registry::{ShaderDef, ShaderHandle, ShaderRegistry};
