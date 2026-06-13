pub mod cpu_server;
pub mod gpu_server;
pub mod loader_job;
pub mod loaders;
pub mod material;
pub mod mesh;
pub mod pending;
pub mod shader_registry;

pub use crate::ecs::components::{MaterialHandle, MeshHandle};
pub use cpu_server::{AsyncMeshHandle, CpuAssetServer, LoadProgress, TextureHandle};
pub use material::{MaterialData, MaterialDef};
pub use mesh::{CpuMesh, GpuMesh, Vertex};
pub use shader_registry::{ShaderDef, ShaderHandle, ShaderRegistry, TextureSlot};
