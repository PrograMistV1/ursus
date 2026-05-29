pub mod server;
pub mod mesh;
pub mod material;
pub mod loaders;

pub use server::AssetServer;
pub use mesh::{CpuMesh, GpuMesh, Vertex};
pub use material::{Material, MaterialDef};
pub use crate::ecs::components::{MeshHandle, MaterialHandle};
