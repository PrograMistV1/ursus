use crate::ecs::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub u32);

impl Component for MaterialHandle {}
impl Default for MaterialHandle {
    fn default() -> Self {
        MaterialHandle(0)
    }
}

impl Component for MeshHandle {}
impl Default for MeshHandle {
    fn default() -> Self {
        MeshHandle(0)
    }
}
