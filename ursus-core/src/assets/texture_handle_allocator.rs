use crate::assets::asset_registry::TextureHandle;

/// The only source of TextureHandle in the system.
#[derive(Default)]
pub(crate) struct TextureHandleAllocator {
    next: u32,
}

impl TextureHandleAllocator {
    pub(crate) fn new() -> Self {
        Self { next: 1 } // 0 зарезервирован под bindless white fallback
    }

    pub(crate) fn alloc(&mut self) -> TextureHandle {
        let h = TextureHandle(self.next);
        self.next += 1;
        h
    }
}
