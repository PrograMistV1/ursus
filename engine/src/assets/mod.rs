pub mod loader_job;
pub mod loaders;
pub mod material;
pub mod mesh;
pub mod server;
pub mod shader_registry;

pub use crate::ecs::components::{MaterialHandle, MeshHandle};
pub use material::{MaterialData, MaterialDef};
pub use mesh::{CpuMesh, GpuMesh, Vertex};
pub use server::{AssetServer, AsyncMeshHandle, LoadProgress, TextureHandle};
pub use shader_registry::{ShaderDef, ShaderHandle, ShaderRegistry, TextureSlot};
