pub mod gpu_texture_store;
pub mod loader_registry;
pub(crate) mod material_handle_allocator;
pub mod material_registry;
pub mod mesh;
pub(crate) mod mesh_handle_allocator;
pub mod registry;
pub mod storage;
pub(crate) mod texture_handle_allocator;
pub mod upload;
pub(crate) mod upload_queue;

pub use gpu_texture_store::GpuTextureStore;
pub use loader_registry::{AssetLoader, LoadedMeshSource, LoadedPrimitive, LoadedTexture, LoaderRegistry};
pub use material_registry::MaterialRegistry;
pub use mesh::{CpuMesh, GpuMesh, Vertex};
pub use storage::shader::{ShaderDef, ShaderHandle, ShaderRegistry};
