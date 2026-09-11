pub mod loader_registry;
pub mod mesh;
pub mod registry;
pub mod storage;
pub mod upload;
pub(crate) mod upload_queue;

pub use loader_registry::{AssetLoader, LoadedMeshSource, LoadedPrimitive, LoadedTexture, LoaderRegistry};
pub use mesh::{CpuMesh, Vertex};
pub use storage::shader::{ShaderDef, ShaderHandle, ShaderRegistry};
