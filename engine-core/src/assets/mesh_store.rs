use crate::assets::mesh::CpuMesh;
use crate::components::mesh::MeshHandle;

/// CPU-хранилище зарегистрированных мешей.
///
/// Только хранение + выдача хендлов по индексу - ничего не знает про GPU-аплоад
/// (это дело [`UploadQueue`](crate::assets::upload_queue::UploadQueue)) и про то, откуда меш взялся (синхронная регистрация
/// или результат фонового `.gltf`/`.obj`-загрузчика - это дело [`AsyncMeshLoader`](crate::assets::upload_queue::AsyncMeshLoader)).
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
