use crate::assets::mesh::CpuMesh;
use crate::assets::upload::GpuUploadRequest;
use crate::assets::upload_queue::UploadQueue;
use ursus_ecs::components::mesh::MeshHandle;

#[derive(Default)]
struct MeshHandleAllocator {
    next: u32,
}

impl MeshHandleAllocator {
    fn alloc(&mut self) -> MeshHandle {
        let id = self.next;
        self.next += 1;
        MeshHandle(id)
    }
}

/// CPU-side mesh registration. Assigns stable handles and queues meshes
/// for GPU upload; never touches Vulkan directly.
pub struct MeshRegistry {
    handles: MeshHandleAllocator,
    upload_queue: UploadQueue,
}

impl MeshRegistry {
    pub(crate) fn new() -> Self {
        Self { handles: MeshHandleAllocator::default(), upload_queue: UploadQueue::new() }
    }

    pub fn upload(&mut self, mesh: CpuMesh) -> MeshHandle {
        let handle = self.handles.alloc();
        self.upload_queue.push(GpuUploadRequest::Mesh {
            handle,
            vertices: mesh.vertices,
            indices: mesh.indices,
            name: mesh.name,
        });
        handle
    }

    pub(crate) fn flush_uploads(&mut self, tx: &std::sync::mpsc::Sender<GpuUploadRequest>) {
        self.upload_queue.drain_to(tx);
    }
}
