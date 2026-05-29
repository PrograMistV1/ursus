use super::components::{MaterialHandle, MeshHandle, Transform};
use super::world::GameWorld;
use crate::assets::AssetServer;

pub struct DrawCall {
    pub mesh: MeshHandle,
    pub material: Option<MaterialHandle>,
    pub transform: Transform,
}

pub fn collect_draw_calls(world: &mut GameWorld, _assets: &AssetServer) -> Vec<DrawCall> {
    let mut calls = Vec::new();

    for (mesh, transform, mat) in world
        .inner
        .query_mut::<(&MeshHandle, &Transform, Option<&MaterialHandle>)>()
    {
        calls.push(DrawCall {
            mesh: *mesh,
            material: mat.copied(),
            transform: transform.clone(),
        });
    }

    calls
}
