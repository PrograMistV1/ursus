pub mod cpu_server;
pub mod gpu_server;
pub mod loader_job;
pub mod loader_registry;
pub mod material;
pub mod mesh;
pub mod shader_registry;
pub mod text;
pub mod upload;

pub use cpu_server::{AsyncMeshHandle, CpuAssetServer, LoadProgress, TextureHandle};
pub use loader_registry::{
    AssetLoader, LoadedMaterial, LoadedMeshSource, LoadedPrimitive, LoadedTexture, LoaderRegistry,
};
pub use material::MaterialPayload;
pub use mesh::{CpuMesh, GpuMesh, Vertex};
pub use shader_registry::{ShaderDef, ShaderHandle, ShaderRegistry};
