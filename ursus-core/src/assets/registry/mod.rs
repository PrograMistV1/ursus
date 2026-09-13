pub mod material;
pub mod mesh;
pub mod texture;

pub use material::MaterialAssetRegistry;
pub use material::MaterialRegistry;
pub use mesh::MeshRegistry;
pub use texture::{TextureHandle, TextureRegistry};

use crate::assets::upload::GpuUploadRequest;
use std::sync::mpsc::Sender;

#[derive(Default)]
pub struct AssetRegistry {
    pub meshes: MeshRegistry,
    pub textures: TextureRegistry,
    pub materials: MaterialAssetRegistry,
}

impl AssetRegistry {
    pub(crate) fn flush_uploads(&mut self, tx: &Sender<GpuUploadRequest>) {
        self.meshes.flush_uploads(tx);
        self.textures.flush_uploads(tx);
        self.materials.flush_dirty(tx);
    }
}
