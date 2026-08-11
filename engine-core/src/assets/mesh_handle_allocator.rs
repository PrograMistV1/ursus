use crate::components::mesh::MeshHandle;

/// The only source of `MeshHandle` in the system.
#[derive(Default)]
pub(crate) struct MeshHandleAllocator {
    next: u32,
}

impl MeshHandleAllocator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn alloc(&mut self) -> MeshHandle {
        let id = self.next;
        self.next += 1;
        MeshHandle(id)
    }
}
