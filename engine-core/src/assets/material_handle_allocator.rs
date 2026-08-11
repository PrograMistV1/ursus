use crate::components::mesh::MaterialHandle;

/// The only source of `MaterialHandle` in the system.
#[derive(Default)]
pub(crate) struct MaterialHandleAllocator {
    next: u32,
}

impl MaterialHandleAllocator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn alloc(&mut self) -> MaterialHandle {
        let id = self.next;
        self.next += 1;
        MaterialHandle(id)
    }
}
