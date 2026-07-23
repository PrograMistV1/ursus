use engine_macros::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct MaterialHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct MeshHandle(pub u32);

impl Default for MaterialHandle {
    fn default() -> Self {
        MaterialHandle(0)
    }
}

impl Default for MeshHandle {
    fn default() -> Self {
        MeshHandle(0)
    }
}
