use engine_macros::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Component)]
pub struct MaterialHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Component)]
pub struct MeshHandle(pub u32);
