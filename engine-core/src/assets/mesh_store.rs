use crate::assets::mesh::CpuMesh;
use crate::components::mesh::MeshHandle;

/// CPU-side storage for registered meshes.
///
/// Only handles storage and retrieval by index - it knows nothing about GPU uploads
/// (that is handled by [`UploadQueue`](crate::assets::upload_queue::UploadQueue))
/// or where the mesh came from (synchronous registration or the result of a background
/// `.gltf`/`.obj` loader - that is handled by
/// [`AsyncMeshLoader`](crate::assets::upload_queue::AsyncMeshLoader)).

#[derive(Default)]
pub(crate) struct MeshStore {
    cpu_meshes: Vec<CpuMesh>,
}

impl MeshStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, mesh: CpuMesh) -> MeshHandle {
        let id = self.cpu_meshes.len() as u32;
        self.cpu_meshes.push(mesh);
        MeshHandle(id)
    }
}
