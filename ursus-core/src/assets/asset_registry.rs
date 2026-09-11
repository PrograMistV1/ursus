use crate::assets::registry::{MaterialAssetRegistry, MeshRegistry, TextureRegistry};
use crate::assets::upload::GpuUploadRequest;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

pub struct AssetRegistry {
    pub meshes: MeshRegistry,
    pub textures: TextureRegistry,
    pub materials: MaterialAssetRegistry,
}

impl AssetRegistry {
    pub(crate) fn new() -> Self {
        Self { meshes: MeshRegistry::new(), textures: TextureRegistry::new(), materials: MaterialAssetRegistry::new() }
    }

    pub(crate) fn flush_uploads(&mut self, tx: &Sender<GpuUploadRequest>) {
        self.meshes.flush_uploads(tx);
        self.textures.flush_uploads(tx);
        self.materials.flush_dirty(tx);
    }
}
